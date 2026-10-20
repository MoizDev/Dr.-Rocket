#lang racket/base
;; Racketeer stepper process: runs DrRacket's algebraic stepper model headlessly and streams
;; each reduction step as JSON. See DESIGN.md ("stepper").

(require json
         racket/match
         racket/async-channel
         racket/port
         racket/pretty
         racket/list
         racket/string
         mzlib/pconvert
         stepper/private/model
         stepper/private/model-settings
         stepper/private/shared-typed
         stepper/private/syntax-hider
         stepper/private/syntax-property
         lang/run-teaching-program
         htdp/bsl/runtime
         net/base64
         "common.rkt"
         "teaching.rkt")

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

(define cmd-images (hasheq))
(define (run-stepper cmd)
  (set! cmd-images cmd)
  (define raw-source (string-of-source (hash-ref cmd 'source "")))
  (define requested-level (hash-ref cmd 'level "beginner"))
  (define module-source? (equal? requested-level "module"))
  ;; `#lang htdp/bsl` buffers are stepped as the matching teaching level. The `#lang` line
  ;; is blanked out (not removed) so source positions stay the same.
  (define level (if module-source? (lang-line-level raw-source) requested-level))
  (define source (if (and module-source? level) (blank-lang-line raw-source) raw-source))
  (define info (hash-ref levels level #f))
  (define settings (let ([s (hash-ref cmd 'settings (hasheq))]) (if (hash? s) s (hasheq))))
  (define width (let ([w (hash-ref cmd 'width 72)]) (if (exact-positive-integer? w) w 72)))
  (define source-name (make-source-name (hash-ref cmd 'path #f)))
  (define teachpacks (parse-teachpacks (hash-ref cmd 'teachpacks '())))
  (cond
    [(not info)
     (emit (hasheq 'ev "error" 'message (if module-source?
                                                     "The stepper only works with the teaching languages (How to Design Programs and DeinProgramm)."
                                                     (format "The stepper does not support the language level ~s." level))))
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
            (step-program source source-name info settings teachpacks width))
          (semaphore-post done))))
     (semaphore-wait done)
     (custodian-shutdown-all cust)]))

(define (parse-teachpacks tps)
  (for/list ([tp (in-list (if (list? tps) tps '()))]
             #:when (string? tp))
    (with-handlers ([exn:fail? (λ (e) (string->symbol tp))])
      (define v (read (open-input-string tp)))
      (if (eof-object? v) (string->symbol tp) v))))

(define (step-program source source-name info settings teachpacks width)
  (define dir (if (path? source-name)
                  (let-values ([(base name dir?) (split-path source-name)]) (if (path? base) base (current-directory)))
                  (current-directory)))
  (define ns (make-base-namespace))
  ;; Embedded image snips must use the same class instance as the source-port helpers.
  (namespace-attach-module (variable-reference->namespace (#%variable-reference)) 'racket/class ns)
  (define sdp? (sdp-level-info? info))
  (define rt-settings
    (and (not sdp?) (apply sl-runtime-settings (htdp-runtime-settings-args info settings))))
  (define step-count 0)
  (parameterize ([current-namespace ns]
                 [current-directory dir]
                 [current-load-relative-directory dir]
                 [current-output-port (open-output-nowhere)]
                 [main-source source-name]
                 [read-case-sensitive (setting settings 'case_sensitive (not sdp?))]
                 [read-accept-quasiquote (level-info-accept-quasiquote? info)]
                 [read-decimal-as-inexact #f])
    (define images (decode-source-images (hash-ref cmd-images 'images #f)))
    (define port (open-source-port source images))
    (port-count-lines! port)
    (define selection (setting settings 'selection #f))
    (define-values (reader context-names)
      (if selection
          (selection-reader source source-name port images selection)
          (values read-syntax '())))
    (define (visible-exps exps)
      (if (null? context-names) exps
          (filter (λ (s) (not (member (definition-name (sstx-s s)) context-names))) exps)))
    (define render-value/format
      (cond
        [sdp?
         (define runtime 'deinprogramm/sdp/private/runtime)
         (define make-settings (dynamic-require runtime 'sdp-runtime-settings))
         ((dynamic-require runtime 'configure/settings)
          (apply make-settings (sdp-runtime-settings-args settings (sdp-level-info-writing-style info))))
         (dynamic-require runtime 'sdp-render-value/format)]
        [else
         (configure/settings rt-settings)
         sl-render-value/format]))
    (define size-hook (pretty-print-size-hook))
    (define print-hook (pretty-print-print-hook))
    (define (render-to-string v)
      (define o (open-output-string))
      (render-value/format v o 'infinity)
      (get-output-string o))
    ;; DrRacket builds the stepper's render settings in its GUI thread, where print-convert
    ;; abbreviates cons chains as `list` at every level (the language's own printing settings
    ;; are installed in another thread). So BSL steps show (cons 'b '()) -> (list 'b).
    (define (render-to-sexp v)
      (parameterize ([abbreviate-cons-as-list #t])
        (cond
          [(procedure? v) (or (object-name v) (print-convert v))]
          ;; Images stay values; strip-to-sexp turns them into placeholders with their PNGs.
          [(not (eq? (image->placeholder v) v)) v]
          [else (print-convert v)])))
    (define render-settings
      (parameterize ([abbreviate-cons-as-list #t])
        (get-render-settings render-to-string render-to-sexp #t #t (level-info-lambdas? info))))
    (define (render-exps sstxs)
      (define visible (if (setting settings 'functional_focus #f) (focused-exps sstxs) sstxs))
      (for/list ([s (in-list visible)])
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
         (define visible-pre (visible-exps pre))
         (define visible-post (visible-exps post))
         (unless (and selection
                      (or (and (null? visible-pre) (null? visible-post))
                          (and (pair? context-names)
                               (ormap (λ (s) (and (member (definition-name (sstx-s s)) context-names)
                                                  (pair? (highlighted-exps (sstx-s s))))) pre))))
           (reset-step-images!)
           (define before (render-exps visible-pre))
           (define after (render-exps visible-post))
           (emit (hasheq 'ev "step" 'index step-count 'kind (format "~a" kind)
                         'images step-images
                         'before before 'after after
                         'before_src (posn-info->jsexpr pre-src)
                         'after_src (posn-info->jsexpr post-src)))
           (set! step-count (add1 step-count))
           (when (zero? (modulo step-count steps-per-batch))
             (emit (hasheq 'ev "paused" 'count step-count))
             (semaphore-wait more-sema)))]
        [(Before-Error-Result pre msg pre-src)
         (reset-step-images!)
         (define before (render-exps pre))
         (emit (hasheq 'ev "step-error" 'index step-count 'images step-images 'before before
                       'message msg 'before_src (posn-info->jsexpr pre-src)))
         (set! step-count (add1 step-count))]
        [(Error-Result msg)
         (emit (hasheq 'ev "error" 'message msg))]
        ['finished-stepping
         (emit (hasheq 'ev "finished" 'count step-count))]
        [_ (void)]))
    ((dynamic-require 'test-engine/racket-tests 'test-execute) (setting settings 'include_tests #t))
    (define next
      (expand-teaching-program port
                               reader
                               (level-info-module info)
                               teachpacks))
    (go (λ (init iter)
          (init)
          (error-value->string-handler
           (λ (v len) (truncate-string (render-to-string v) len)))
          (current-print void)
          (let loop () (iter (next) loop)))
        void
        receive-result
        render-settings)))

(define (definition-name stx)
  (define name
    (match (syntax->datum stx)
      [(list 'define (cons name _) _ ...) name]
      [(list (or 'define 'define-struct) name _ ...) name]
      [_ #f]))
  ;; The stepper can reconstruct function names as uninterned symbols.
  (and (symbol? name) (symbol->string name)))

;; Parse with Racket's reader and keep original source locations, including embedded images.
(define (selection-reader source source-name port images selection)
  (match-define (list start end)
    (match selection
      [(list (? exact-nonnegative-integer? a) (? exact-nonnegative-integer? z))
       #:when (< a z (add1 (string-length source))) selection]
      [_ (error 'stepper "Select a nonempty range of complete expressions.")]))
  (define (read-all p)
    (parameterize ([read-accept-lang #f])
      (for/list ([s (in-port (λ (p) (read-syntax source-name p)) p)]) s)))
  (define forms (read-all port))
  (define selected-port (open-source-port (substring source start end) images))
  (port-count-lines! selected-port)
  (define location-port (open-input-string source))
  (port-count-lines! location-port)
  (read-string start location-port)
  (define-values (line column position) (port-next-location location-port))
  (set-port-next-location! selected-port line column position)
  (define selected (read-all selected-port))
  (when (null? selected)
    (error 'stepper "The selection contains no expressions."))
  ;; A readable substring can still cut through a token, string, or comment.
  (define spans (make-hash))
  (define (collect v)
    (cond [(syntax? v)
           (hash-set! spans (cons (syntax-position v) (syntax-span v)) #t)
           (collect (syntax-e v))]
          [(pair? v) (collect (car v)) (collect (cdr v))]))
  (for-each collect forms)
  (unless (andmap (λ (s) (hash-ref spans (cons (syntax-position s) (syntax-span s)) #f)) selected)
    (error 'stepper "Select complete expressions, without cutting through a token or comment."))
  (define (inside? s)
    (define pos (sub1 (syntax-position s)))
    (and (<= start pos) (<= (+ pos (syntax-span s)) end)))
  (define context
    (filter (λ (s)
              (define parts (syntax->list s))
              (and parts (pair? parts)
                   (memq (syntax-e (car parts)) '(define define-struct require :))
                   (not (inside? s))))
            forms))
  (define remaining (sort (append context selected) < #:key syntax-position))
  (values (λ (_src _port)
            (if (null? remaining) eof
                (begin0 (car remaining) (set! remaining (cdr remaining)))))
          (filter values (map definition-name context))))

;; Keep the stepper's active redex, not the surrounding recursive continuation.
(define (highlighted-exps v)
  (cond
    [(and (syntax? v) (stepper-syntax-property v 'stepper-highlight)) (list (sstx v))]
    [(syntax? v) (highlighted-exps (syntax-e v))]
    [(pair? v) (append (highlighted-exps (car v)) (highlighted-exps (cdr v)))]
    [else '()]))

(define (focused-exps sstxs)
  (define active (append-map (λ (s) (highlighted-exps (sstx-s s))) sstxs))
  (cond [(pair? active) active]
        [(pair? sstxs) (list (last sstxs))]
        [else '()]))

(define (blank-lang-line source)
  (define m (regexp-match-positions #px"#lang\\s+\\S+" source))
  (if m
      (string-append (substring source 0 (caar m))
                     (make-string (- (cdar m) (caar m)) #\space)
                     (substring source (cdar m)))
      source))

;; strip-to-sexp : syntax hash -> datum
;; Like the stepper GUI's strip-to-sexp: highlighted pairs are recorded as-is; highlighted
;; atoms are replaced by unique placeholder symbols mapped to (list atom).
;; Images in steps: each becomes a placeholder character (as in the IDE's program text),
;; with its PNG sent alongside the step. Reset per step.
(define step-images (make-hasheq))  ; code-point symbol -> base64 png
(define step-image-codes (make-hash)) ; png -> char
(define (reset-step-images!)
  (set! step-images (make-hasheq))
  (set! step-image-codes (make-hash)))
(define (image->placeholder v)
  (define png (and (not (or (number? v) (symbol? v) (string? v) (boolean? v) (null? v) (char? v) (keyword? v)))
                   (image-value->png v)))
  (cond
    [png
     (define c (hash-ref! step-image-codes png (λ () (integer->char (+ #xF0000 (hash-count step-image-codes))))))
     (hash-set! step-images (string->symbol (number->string (char->integer c)))
                (bytes->string/latin-1 (base64-encode png #"")))
     (string->symbol (string c))]
    [else v]))

(define (strip-to-sexp stx table)
  (let strip ([stx stx])
    (define inner
      (cond
        [(pair? stx) (cons (strip (car stx)) (strip (cdr stx)))]
        [(syntax? stx) (strip (syntax-e stx))]
        [else (image->placeholder stx)]))
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
