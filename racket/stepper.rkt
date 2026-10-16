#lang racket/base
;; Racketeer stepper process: runs DrRacket's algebraic stepper model headlessly and streams
;; each reduction step as JSON. See DESIGN.md ("stepper").

(require json
         racket/match
         racket/async-channel
         racket/port
         racket/pretty
         racket/string
         mzlib/pconvert
         stepper/private/model
         stepper/private/model-settings
         stepper/private/shared-typed
         stepper/private/syntax-hider
         stepper/private/syntax-property
         lang/run-teaching-program
         htdp/bsl/runtime
         "common.rkt")

;; level id -> (values language-module reader-params abbreviate? use-function-output?
;;                     output-function-instead-of-lambda? show-lambdas-as-lambdas? sharing?)
(struct level-info (module accept-quasiquote? abbreviate? function-output? output-function?
                     lambdas? sharing?))

(define levels
  (hash "beginner"            (level-info '(lib "htdp-beginner.ss" "lang") #f #f #f #t #f #f)
        "beginner-abbr"       (level-info '(lib "htdp-beginner-abbr.ss" "lang") #t #t #f #t #f #f)
        "intermediate"        (level-info '(lib "htdp-intermediate.ss" "lang") #t #t #t #t #f #f)
        "intermediate-lambda" (level-info '(lib "htdp-intermediate-lambda.ss" "lang") #t #t #t #f #t #f)
        "advanced"            (level-info '(lib "htdp-advanced.ss" "lang") #t #t #t #f #t #t)))

;; `#lang htdp/bsl` and friends map onto the same levels.
(define lang-line-levels
  '(("htdp/bsl" . "beginner") ("htdp/bsl+" . "beginner-abbr") ("htdp/isl" . "intermediate")
    ("htdp/isl+" . "intermediate-lambda") ("htdp/asl" . "advanced")))

(define (lang-line-level source)
  (define m (regexp-match #px"^\\s*(?:;[^\n]*\n\\s*)*#lang\\s+(\\S+)" source))
  (and m (let ([p (assoc (cadr m) lang-line-levels)]) (and p (cdr p)))))

(define commands (make-async-channel))
(define more-sema (make-semaphore 0))
(define steps-per-batch 500)

(define (main)
  (define in (current-input-port))
  (thread
   (λ ()
     (let loop ()
       (define cmd (read-command in))
       (cond
         [(eof-object? cmd) (async-channel-put commands cmd)]
         [else
          (case (hash-ref cmd 'cmd #f)
            [("step") (async-channel-put commands cmd)]
            [("more") (semaphore-post more-sema)]
            [else (void)])
          (loop)]))))
  (let loop ()
    (define cmd (async-channel-get commands))
    (unless (eof-object? cmd)
      (run-stepper cmd)
      (loop))))

(define (setting settings key default)
  (define v (hash-ref settings key default))
  (if (eq? v (json-null)) default v))

(define (run-stepper cmd)
  (define source (string-of-source (hash-ref cmd 'source "")))
  (define requested-level (hash-ref cmd 'level "beginner"))
  (define module-source? (equal? requested-level "module"))
  (define level (if module-source? (or (lang-line-level source) "beginner") requested-level))
  (define info (hash-ref levels level #f))
  (define settings (let ([s (hash-ref cmd 'settings (hasheq))]) (if (hash? s) s (hasheq))))
  (define width (let ([w (hash-ref cmd 'width 72)]) (if (exact-positive-integer? w) w 72)))
  (define source-name (make-source-name (hash-ref cmd 'path #f)))
  (define teachpacks (parse-teachpacks (hash-ref cmd 'teachpacks '())))
  (cond
    [(not info)
     (emit (hasheq 'ev "error" 'message (format "The stepper does not support the language level ~s." level)))
     (emit (hasheq 'ev "finished" 'count 0))]
    [else
     (define done (make-semaphore 0))
     (define cust (make-custodian))
     (parameterize ([current-custodian cust])
       (thread
        (λ ()
          (with-handlers ([(λ (e) #t)
                           (λ (e)
                             (emit (hasheq 'ev "error" 'message (exn->message e)))
                             (emit (hasheq 'ev "finished" 'count 0)))])
            (step-program source source-name info settings teachpacks width module-source?))
          (semaphore-post done))))
     (semaphore-wait done)
     (custodian-shutdown-all cust)]))

(define (parse-teachpacks tps)
  (for/list ([tp (in-list (if (list? tps) tps '()))]
             #:when (string? tp))
    (with-handlers ([exn:fail? (λ (e) (string->symbol tp))])
      (define v (read (open-input-string tp)))
      (if (eof-object? v) (string->symbol tp) v))))

(define (step-program source source-name info settings teachpacks width module-source?)
  (define dir (if (path? source-name)
                  (let-values ([(base name dir?) (split-path source-name)]) (if (path? base) base (current-directory)))
                  (current-directory)))
  (define ns (make-base-namespace))
  (define printing-style
    (string->symbol (setting settings 'printing_style "constructor")))
  (define fraction-style
    (string->symbol (setting settings 'fraction_style "repeating-decimal")))
  (define rt-settings
    (sl-runtime-settings printing-style
                         fraction-style
                         (setting settings 'show_sharing (level-info-sharing? info))
                         (setting settings 'insert_newlines #t)
                         #f
                         (setting settings 'true_false_empty_as_ids #f)
                         (level-info-abbreviate? info)
                         (level-info-function-output? info)
                         (level-info-output-function? info)))
  (define step-count 0)
  (parameterize ([current-namespace ns]
                 [current-directory dir]
                 [current-load-relative-directory dir]
                 [current-output-port (open-output-nowhere)]
                 [main-source source-name]
                 [read-case-sensitive (setting settings 'case_sensitive #t)]
                 [read-accept-quasiquote (level-info-accept-quasiquote? info)]
                 [read-decimal-as-inexact #f])
    (configure/settings rt-settings)
    (define size-hook (pretty-print-size-hook))
    (define print-hook (pretty-print-print-hook))
    (define (render-to-string v)
      (define o (open-output-string))
      (sl-render-value/format v o 'infinity)
      (get-output-string o))
    (define (render-to-sexp v)
      (or (and (procedure? v) (object-name v))
          (print-convert v)))
    (define render-settings
      (get-render-settings render-to-string render-to-sexp #t #t (level-info-lambdas? info)))
    (define (render-exps sstxs)
      (for/list ([s (in-list sstxs)])
        (define table (make-hasheq))
        (define datum (strip-to-sexp (sstx-s s) table))
        (define-values (text ranges)
          (parameterize ([pretty-print-show-inexactness #t]
                         [print-boolean-long-form #t]
                         [read-case-sensitive #t])
            (pretty-with-highlights datum table width #:size-hook size-hook #:print-hook print-hook)))
        (hasheq 'text text 'highlights ranges)))
    (define (posn-info->jsexpr p)
      (if (and (Posn-Info? p) (Posn-Info-posn p))
          (hasheq 'pos (adjust-pos source-name (Posn-Info-posn p))
                  'span (or (Posn-Info-span p) 0))
          (json-null)))
    (define (receive-result r)
      (match r
        [(Before-After-Result pre post kind pre-src post-src)
         (emit (hasheq 'ev "step" 'index step-count 'kind (format "~a" kind)
                       'before (render-exps pre) 'after (render-exps post)
                       'before_src (posn-info->jsexpr pre-src)
                       'after_src (posn-info->jsexpr post-src)))
         (set! step-count (add1 step-count))
         (when (zero? (modulo step-count steps-per-batch))
           (emit (hasheq 'ev "paused" 'count step-count))
           (semaphore-wait more-sema))]
        [(Before-Error-Result pre msg pre-src)
         (emit (hasheq 'ev "step-error" 'index step-count 'before (render-exps pre)
                       'message msg 'before_src (posn-info->jsexpr pre-src)))
         (set! step-count (add1 step-count))]
        [(Error-Result msg)
         (emit (hasheq 'ev "error" 'message msg))]
        ['finished-stepping
         (emit (hasheq 'ev "finished" 'count step-count))]
        [_ (void)]))
    (define port (open-input-string source))
    (port-count-lines! port)
    (define next
      (if module-source?
          (module-expander port source-name)
          (expand-teaching-program port
                                   (λ (src p) (read-syntax src p))
                                   (level-info-module info)
                                   teachpacks)))
    (go (λ (init iter)
          (init)
          (error-value->string-handler
           (λ (v len) (truncate-string (render-to-string v) len)))
          (current-print void)
          (let loop () (iter (next) loop)))
        void
        receive-result
        render-settings)))

;; For `#lang htdp/...` buffers: expand the module, then require it, mirroring DrRacket's
;; module-language program expander.
(define (module-expander port source-name)
  (define state 'module)
  (define modname #f)
  (λ ()
    (case state
      [(module)
       (set! state 'require)
       (define stx (parameterize ([read-accept-reader #t] [read-accept-lang #t])
                     (read-syntax source-name port)))
       (syntax-case stx ()
         [(module name . _)
          (set! modname (syntax-e #'name))])
       (expand stx)]
      [(require)
       (set! state 'done)
       (stepper-syntax-property
        (datum->syntax #f `(dynamic-require '',modname #f))
        'stepper-skip-completely #t)]
      [else eof])))

;; strip-to-sexp : syntax hash -> datum
;; Like the stepper GUI's strip-to-sexp: highlighted pairs are recorded as-is; highlighted
;; atoms are replaced by unique placeholder symbols mapped to (list atom).
(define (strip-to-sexp stx table)
  (let strip ([stx stx])
    (define inner
      (cond
        [(pair? stx) (cons (strip (car stx)) (strip (cdr stx)))]
        [(syntax? stx) (strip (syntax-e stx))]
        [else stx]))
    (cond
      [(and (syntax? stx) (stepper-syntax-property stx 'stepper-highlight))
       (cond
         [(pair? inner) (hash-set! table inner #t) inner]
         [else
          (define placeholder (string->uninterned-symbol "-placeholder"))
          (hash-set! table placeholder (list inner))
          placeholder])]
      [else inner])))

(module+ main (main))
