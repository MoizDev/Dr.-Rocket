#lang racket/base
;; Racketeer language server: one long-lived process answering requests about programs:
;; check syntax (binding arrows, tooltips, docs, jump targets), blueboxes, documentation
;; search, completions, the macro stepper, and module graphs. See DESIGN.md ("server").
;;
;; Every request runs in its own thread under its own custodian, so slow requests never
;; block fast ones and any request can be cancelled.

(provide run-check-syntax)

(require json
         racket/async-channel
         racket/class
         racket/list
         racket/match
         racket/port
         racket/pretty
         racket/promise
         racket/string
         syntax/modread
         net/base64
         drracket/check-syntax
         "common.rkt")

;; ---------------------------------------------------------------------------
;; Request dispatch

(define active (make-hash))          ; id -> custodian
(define active-by-path (make-hash))  ; path -> id of the running check-syntax
(define active-lock (make-semaphore 1))

(define (main)
  ;; Includes the shared loader and docs workers, not just request custodians.
  (custodian-limit-memory (current-custodian) (* 512 1024 1024))
  (define in (current-input-port))
  ;; Check syntax's documentation lookups run in a helper thread created when its xref
  ;; module is instantiated; instantiate it here, under the main custodian, so cancelling
  ;; a request can never kill it. Then warm the documentation index in the background.
  (thread (λ ()
            ;; Everything the docs worker will touch is instantiated up front by the loader,
            ;; so the two workers never instantiate modules concurrently.
            (on-loader (λ ()
                         ;; Includes the modules that deserializing the documentation index
                         ;; loads (found by tracing `current-load/use-compiled`).
                         (for ([m '(drracket/private/syncheck/xref setup/xref scribble/xref
                                    scribble/blueboxes scribble/manual-struct racket/serialize
                                    scribble/core scribble/html-properties scribble/latex-properties
                                    scribble/render-struct scribble/private/literal-anchor
                                    scribble/private/manual-class-struct setup/infotab setup/getinfo)])
                           (dynamic-require m #f))))
            (on-docs (λ ()
                       (force xref-promise)
                       (force index-promise)))))
  (let loop ()
    (define req (read-command in))
    (unless (eof-object? req)
      (dispatch req)
      (loop)))
  (exit 0))

(define (dispatch req)
  (define id (hash-ref req 'id #f))
  (define cmd (hash-ref req 'cmd #f))
  (cond
    [(equal? cmd "cancel") (cancel! (hash-ref req 'id #f))]
    [(not id) (void)]
    [else
     (when (equal? cmd "check-syntax")
       (define path (hash-ref req 'path #f))
       (define previous (call-with-semaphore active-lock (λ () (hash-ref active-by-path path #f))))
       (when previous (cancel! previous))
       (call-with-semaphore active-lock (λ () (hash-set! active-by-path path id))))
     (define cust (make-custodian))
     (call-with-semaphore active-lock (λ () (hash-set! active id cust)))
     (parameterize ([current-custodian cust])
       (thread
        (λ ()
          (define result
            (with-handlers ([(λ (e) #t)
                             (λ (e) (hasheq 'id id 'ok #f 'error (exn->message e)))])
              (hasheq 'id id 'ok #t 'result (handle cmd req))))
          (call-with-semaphore active-lock
                               (λ ()
                                 (hash-remove! active id)
                                 (for ([(p i) (in-hash active-by-path)] #:when (equal? i id))
                                   (hash-remove! active-by-path p))))
          (emit result)
          (custodian-shutdown-all cust))))]))

(define (cancel! id)
  (define cust (call-with-semaphore active-lock (λ () (begin0 (hash-ref active id #f)
                                                              (hash-remove! active id)))))
  (when cust
    (emit (hasheq 'id id 'ok #f 'error "cancelled"))
    (custodian-shutdown-all cust)))

(define (handle cmd req)
  (match cmd
    ["check-syntax" (check-syntax req)]
    ["bluebox" (bluebox req)]
    ["docs-search" (docs-search req)]
    ["completions" (completions req)]
    ["macro-steps" (macro-steps req)]
    ["module-graph" (module-graph req)]
    ["jump-target" (jump-target req)]
    ;; Library loading (wxme, racket/snip, racket/gui) must happen on the loader thread.
    ["wxme-read" (on-loader (λ () (wxme-read req)))]
    ["wxme-write" (on-loader (λ () (wxme-write req)))]
    [_ (error 'server "unknown command: ~a" cmd)]))

(define (req-ref req key default)
  (define v (hash-ref req key default))
  (if (eq? v (json-null)) default v))

;; ---------------------------------------------------------------------------
;; Workers
;;
;; Instantiating library modules from several threads at once, or killing a thread in the
;; middle of an instantiation (a cancelled request), corrupts the module registry. The
;; documentation xref objects are not thread-safe either. So library loading happens on one
;; long-lived worker thread and xref access on another, both owned by the main custodian;
;; requests send them thunks and wait for the answer. Keeping docs on their own worker means
;; loading the documentation index never delays check syntax.

;; make-worker : -> (values thread ((-> any) -> any))
(define (make-worker)
  (define requests (make-channel))
  (define worker
    (thread
     (λ ()
       (let loop ()
         (define job (channel-get requests))
         (define result
           (with-handlers ([(λ (e) #t) (λ (e) (cons 'raise e))])
             (cons 'ok (call-with-values (car job) list))))
         (async-channel-put (cdr job) result)
         (loop)))))
  (values worker
          (λ (thunk)
            (if (eq? (current-thread) worker)
                (thunk)
                (let ([reply (make-async-channel)])
                  (channel-put requests (cons thunk reply))
                  (define result (async-channel-get reply))
                  (if (eq? (car result) 'ok)
                      (apply values (cdr result))
                      (raise (cdr result))))))))

;; on-loader : (-> any) -> any   -- module instantiation
(define-values (loader-thread on-loader) (make-worker))
;; on-docs : (-> any) -> any     -- documentation xref access
(define-values (docs-thread on-docs) (make-worker))

;; lib : module-path symbol -> any   -- dynamic-require through the loader
(define (lib mod name)
  (on-loader (λ () (dynamic-require mod name))))

;; ---------------------------------------------------------------------------
;; Shared library namespace
;;
;; Language modules are instantiated once here and attached to each fresh expansion
;; namespace, so repeated check-syntax runs don't reload `racket` or the teaching
;; languages from disk.

(define lib-ns (make-base-namespace))

(define (fresh-namespace-for lang)
  (define ns (make-base-namespace))
  (when (and lang (module-path? lang))
    (with-handlers ([exn:fail? void])
      (on-loader (λ ()
                   (parameterize ([current-namespace lib-ns])
                     (dynamic-require lang (void)))))
      (namespace-attach-module lib-ns lang ns)))
  ns)

(define (module-language stx)
  (syntax-case stx ()
    [(_ name lang . body) (let ([d (syntax->datum #'lang)]) (and (module-path? d) d))]
    [_ #f]))

(define (directory-of source-name)
  (if (path? source-name)
      (let-values ([(base name dir?) (split-path source-name)])
        (if (path? base) base (current-directory)))
      (current-directory)))

;; read-program : string source -> syntax
(define current-images (make-parameter (hash)))
(define (read-program source source-name)
  (define port (open-source-port source (current-images)))
  (port-count-lines! port)
  (with-module-reading-parameterization
   (λ () (read-syntax source-name port))))

(define (with-program req proc)
  (define source (string-of-source (req-ref req 'source "")))
  (define offset (let ([o (req-ref req 'offset 0)])
                   (if (exact-nonnegative-integer? o) (min o (string-length source)) 0)))
  (define source-name (make-source-name (req-ref req 'path #f)))
  (parameterize ([main-source source-name]
                 [current-images (decode-source-images (req-ref req 'images #f))]
                 [source-offset offset]
                 [source-line-offset (for/sum ([c (in-string source 0 offset)])
                                       (if (char=? c #\newline) 1 0))])
    (proc source source-name offset)))

;; ---------------------------------------------------------------------------
;; Check syntax

(define (strip-style name)
  (define s (format "~a" name))
  (cond
    [(regexp-match #rx"^drracket:check-syntax:(.*)$" s) => cadr]
    [else s]))

(define collector%
  (class (annotations-mixin object%)
    (init-field src offset)
    (super-new)
    (define arrows '())
    (define tooltips '())
    (define colors '())
    (define unused-requires '())
    (define docs '())
    (define jumps '())
    (define definitions '())
    (define require-opens '())

    (define/private (mine? obj) (eq? obj src))
    (define/private (adj p) (max 0 (- p offset)))
    (define/private (range s e) (list (adj s) (adj e)))

    (define/override (syncheck:find-source-object stx)
      (and (same-source? (syntax-source stx) src) src))

    (define/override (syncheck:add-arrow/name-dup/pxpy start-text start-l start-r start-px start-py
                                                      end-text end-l end-r end-px end-py
                                                      actual? level require-arrow? name-dup?)
      ;; Zero-width targets are implicit forms such as #%app; DrRacket doesn't draw them.
      (when (and (mine? start-text) (mine? end-text) (eqv? level 0)
                 (< start-l start-r) (< end-l end-r))
        (set! arrows (cons (hasheq 'from (range start-l start-r)
                                   'to (range end-l end-r)
                                   'kind (if require-arrow? "imported" "lexical")
                                   'actual (and actual? #t))
                           arrows))))

    (define/override (syncheck:add-tail-arrow from-text from-pos to-text to-pos)
      (when (and (mine? from-text) (mine? to-text))
        (set! arrows (cons (hasheq 'from (range from-pos (add1 from-pos))
                                   'to (range to-pos (add1 to-pos))
                                   'kind "tail"
                                   'actual #t)
                           arrows))))

    (define/override (syncheck:add-mouse-over-status text l r str)
      (when (and (mine? text) (< l r))
        (set! tooltips (cons (hasheq 'start (adj l) 'end (adj r) 'text str) tooltips))))

    (define/override (syncheck:color-range source start finish style-name)
      (when (and (mine? source) (< start finish))
        (set! colors (cons (hasheq 'start (adj start) 'end (adj finish) 'style (strip-style style-name))
                           colors))))

    (define/override (syncheck:unused-binder source l r)
      (when (mine? source)
        (set! colors (cons (hasheq 'start (adj l) 'end (adj r) 'style "unused-identifier") colors))))

    (define/override (syncheck:add-unused-require src l r)
      (when (mine? src)
        (set! unused-requires (cons (range l r) unused-requires))))

    (define/override (syncheck:add-docs-menu text start end key label path definition-tag tag)
      (when (and (mine? text) (< start end))
        (set! docs (cons (hasheq 'start (adj start) 'end (adj end)
                                 'label (format "~a" label)
                                 'path (path->jsexpr path)
                                 'anchor (if tag (format "~a" tag) (json-null))
                                 'tag definition-tag)
                         docs))))

    (define/override (syncheck:add-jump-to-definition/phase-level+space text start end id filename submods phase)
      (when (and (mine? text) (< start end))
        (set! jumps (cons (hasheq 'start (adj start) 'end (adj end)
                                  'path (path->jsexpr filename)
                                  'id (format "~a" id))
                          jumps))))

    (define/override (syncheck:add-definition-target/phase-level+space source start end id mods phase)
      (when (mine? source)
        (set! definitions (cons (hasheq 'start (adj start) 'end (adj end) 'name (format "~a" id))
                                definitions))))

    (define/override (syncheck:add-require-open-menu text start end file)
      (when (mine? text)
        (set! require-opens (cons (hasheq 'start (adj start) 'end (adj end) 'path (path->jsexpr file))
                                  require-opens))))

    (define/public (doc-entries) docs)
    (define/public (definition-entries) definitions)

    (define/public (result errors)
      (hasheq 'arrows (reverse arrows)
              'tooltips (reverse tooltips)
              'colors (reverse colors)
              'unused_requires (reverse unused-requires)
              'docs (for/list ([d (in-list (reverse docs))]) (hash-remove d 'tag))
              'jumps (reverse jumps)
              'definitions (reverse definitions)
              'require_opens (reverse require-opens)
              'errors errors))))

;; Last successful docs annotations per path, for bluebox lookups by position.
(define docs-cache (make-hash))

(define (run-check-syntax source source-name offset)
  (define collector (new collector% [src source-name] [offset offset]))
  (define dir (directory-of source-name))
  (define errors
    (with-handlers ([(λ (e) #t)
                     (λ (e)
                       (list (hasheq 'message (exn->message e)
                                     'srclocs (if (exn:srclocs? e)
                                                  (for/list ([sl ((exn:srclocs-accessor e) e)]
                                                             #:when (srcloc? sl))
                                                    (srcloc->jsexpr sl))
                                                  '()))))])
      (define stx (parameterize ([current-load-relative-directory dir]) (read-program source source-name)))
      (when (eof-object? stx) (error 'check-syntax "the program is empty"))
      (define ns (fresh-namespace-for (module-language stx)))
      (parameterize ([current-annotations collector]
                     [current-namespace ns]
                     [current-load-relative-directory dir]
                     [current-directory dir]
                     [error-print-source-location #f]
                     [current-output-port (open-output-nowhere)])
        (define-values (expanded-expression expansion-completed) (make-traversal ns dir))
        (expanded-expression (expand stx))
        (expansion-completed))
      '()))
  (values collector errors))

(define (check-syntax req)
  (with-program
   req
   (λ (source source-name offset)
     (define-values (collector errors) (run-check-syntax source source-name offset))
     (hash-set! docs-cache source-name (send collector doc-entries))
     (send collector result errors))))

;; ---------------------------------------------------------------------------
;; Documentation

;; Our own xref instance: the global one from load-collections-xref is used concurrently by
;; check syntax's lookup thread, and xref objects are not safe for concurrent loading.
(define xref-promise
  (delay ((lib 'setup/xref 'make-collections-xref))))

(define index-promise
  (delay
   (let ([xref (force xref-promise)]
         [xref-index (lib 'scribble/xref 'xref-index)])
     (xref-index xref))))

(define blueboxes-cache
  (delay ((lib 'scribble/blueboxes 'make-blueboxes-cache) #t)))

(define (tag->doc tag)
  (define-values (path anchor)
    (on-docs (λ () ((lib 'scribble/xref 'xref-tag->path+anchor) (force xref-promise) tag))))
  (if path
      (hasheq 'path (path->jsexpr path) 'anchor (if anchor (format "~a" anchor) (json-null)))
      (json-null)))

(define (bluebox-for-tag tag)
  (define strs
    (on-docs (λ () ((lib 'scribble/blueboxes 'fetch-blueboxes-strs)
                      tag #:blueboxes-cache (force blueboxes-cache)))))
  (and strs (pair? strs)
       ;; Scribble lays blue boxes out with non-breaking spaces; plain spaces render the same.
       (hasheq 'lines (for/list ([l (in-list (cdr strs))])
                        (string-replace l (string (integer->char #xA0)) " "))
               'kind (car strs)
               'doc (tag->doc tag))))

;; The identifier text around `pos` (0-based, visible buffer).
(define (identifier-at source pos)
  (define delim? (λ (c) (or (char-whitespace? c) (memv c '(#\( #\) #\[ #\] #\{ #\} #\" #\, #\' #\` #\;)))))
  (define n (string-length source))
  (and (<= 0 pos n)
       (let* ([start (let loop ([i pos]) (if (and (> i 0) (not (delim? (string-ref source (sub1 i))))) (loop (sub1 i)) i))]
              [end (let loop ([i pos]) (if (and (< i n) (not (delim? (string-ref source i)))) (loop (add1 i)) i))])
         (and (< start end) (substring source start end)))))

(define (tag-for-name name lang)
  (define sym (string->symbol name))
  (define ns (fresh-namespace-for lang))
  (define binding
    (parameterize ([current-namespace ns])
      (with-handlers ([exn:fail? (λ (e) #f)])
        (namespace-require lang)
        (identifier-binding (namespace-symbol->identifier sym)))))
  (and (list? binding)
       (on-docs (λ () ((lib 'scribble/xref 'xref-binding->definition-tag)
                         (force xref-promise) (list (car binding) (cadr binding)) #f)))))

(define (bluebox req)
  (with-program
   req
   (λ (source source-name offset)
     (define pos (req-ref req 'pos #f))
     (define name (req-ref req 'name #f))
     (cond
       [(and (exact-nonnegative-integer? pos) (not name))
        (define entries
          (or (hash-ref docs-cache source-name #f)
              (let-values ([(collector errors) (run-check-syntax source source-name offset)])
                (define d (send collector doc-entries))
                (hash-set! docs-cache source-name d)
                d)))
        (define hit (for/first ([d (in-list entries)]
                                #:when (and (<= (hash-ref d 'start) pos) (< pos (hash-ref d 'end))))
                      d))
        (or (and hit (hash-ref hit 'tag #f) (bluebox-for-tag (hash-ref hit 'tag)))
            (let ([id (identifier-at (substring source offset) pos)])
              (and id (lookup-bluebox-by-name id source source-name)))
            (json-null))]
       [(string? name) (or (lookup-bluebox-by-name name source source-name) (json-null))]
       [else (json-null)]))))

(define (lookup-bluebox-by-name name source source-name)
  (define lang
    (with-handlers ([(λ (e) #t) (λ (e) 'racket)])
      (or (module-language (read-program source source-name)) 'racket)))
  (define tag (tag-for-name name lang))
  (and tag (bluebox-for-tag tag)))

(define (docs-search req)
  (define query (string-downcase (string-trim (format "~a" (req-ref req 'query "")))))
  (define limit (let ([l (req-ref req 'limit 50)]) (if (exact-positive-integer? l) l 50)))
  (cond
    [(string=? query "") '()]
    [else
     (define entry-words (lib 'scribble/xref 'entry-words))
     (define entry-tag (lib 'scribble/xref 'entry-tag))
     (define entry-desc (lib 'scribble/xref 'entry-desc))
     (define exported-index-desc? (lib 'scribble/manual-struct 'exported-index-desc?))
     (define exported-index-desc-name (lib 'scribble/manual-struct 'exported-index-desc-name))
     (define exported-index-desc-from-libs (lib 'scribble/manual-struct 'exported-index-desc-from-libs))
     (define scored
       (for*/list ([e (in-list (on-docs (λ () (force index-promise))))]
                   [name (in-value (let ([d (entry-desc e)])
                                     (if (exported-index-desc? d)
                                         (format "~a" (exported-index-desc-name d))
                                         (string-join (entry-words e) " "))))]
                   [lname (in-value (string-downcase name))]
                   [score (in-value (cond
                                      [(string=? lname query) 0]
                                      [(string-prefix? lname query) 1]
                                      [(string-contains? lname query) 2]
                                      [else #f]))]
                   #:when score)
         ;; Rank: match quality, then the core libraries first, then shorter names.
         (define core? (let ([d (entry-desc e)])
                         (and (exported-index-desc? d)
                              (for/or ([l (exported-index-desc-from-libs d)])
                                (memq l '(racket/base racket))))))
         (list (+ (* 2 score) (if core? 0 1)) (string-length name) name e)))
     (for/list ([s (in-list (take (sort scored (λ (a b) (or (< (car a) (car b))
                                                              (and (= (car a) (car b))
                                                                   (< (cadr a) (cadr b))))))
                                  (min limit (length scored))))])
       (define e (cadddr s))
       (define d (entry-desc e))
       (define doc (tag->doc (entry-tag e)))
       (hasheq 'name (caddr s)
               'libs (if (exported-index-desc? d)
                         (for/list ([l (exported-index-desc-from-libs d)]) (format "~s" l))
                         '())
               'kind (desc-kind d)
               'path (if (hash? doc) (hash-ref doc 'path) (json-null))
               'anchor (if (hash? doc) (hash-ref doc 'anchor) (json-null))))]))

(define (desc-kind d)
  (define (is? name) ((lib 'scribble/manual-struct name) d))
  (define extras-kind
    (and (is? 'exported-index-desc*?)
         (let ([k (hash-ref ((lib 'scribble/manual-struct 'exported-index-desc*-extras) d) 'kind #f)])
           (and (string? k) k))))
  (cond
    [extras-kind extras-kind]
    [(is? 'procedure-index-desc?) "procedure"]
    [(is? 'form-index-desc?) "syntax"]
    [(is? 'struct-index-desc?) "struct"]
    [(is? 'thing-index-desc?) "value"]
    [(is? 'module-path-index-desc?) "module"]
    [(is? 'class-index-desc?) "class"]
    [else "other"]))

;; ---------------------------------------------------------------------------
;; Completions

(define (language->module-path lang)
  (cond
    [(not (string? lang)) 'racket]
    [else
     (define v (with-handlers ([exn:fail? (λ (e) #f)]) (read (open-input-string lang))))
     (if (module-path? v) v 'racket)]))

(define (completions req)
  (define mod (language->module-path (req-ref req 'language "racket")))
  (on-loader
   (λ ()
     (parameterize ([current-namespace lib-ns])
       (dynamic-require mod (void))
       (define-values (vars stxs) (module->exports mod))
       (define names
         (for*/list ([phase+ids (in-list (append vars stxs))]
                     #:when (eqv? (car phase+ids) 0)
                     [entry (in-list (cdr phase+ids))])
           (symbol->string (car entry))))
       (sort (remove-duplicates names) string<?)))))

;; ---------------------------------------------------------------------------
;; Macro stepper

(define (macro-steps req)
  (with-program
   req
   (λ (source source-name offset)
     (define trace* (lib 'macro-debugger/model/trace 'trace*))
     (define reductions (lib 'macro-debugger/model/reductions 'reductions))
     (define macro-policy (lib 'macro-debugger/model/reductions-config 'macro-policy))
     (define policy->predicate (lib 'macro-debugger/model/hiding-policies 'policy->predicate))
     (define step? (lib 'macro-debugger/model/steps 'step?))
     (define misstep? (lib 'macro-debugger/model/steps 'misstep?))
     (define protostep-type (lib 'macro-debugger/model/steps 'protostep-type))
     (define protostep-s1 (lib 'macro-debugger/model/steps 'protostep-s1))
     (define step-s2 (lib 'macro-debugger/model/steps 'step-s2))
     (define state-foci (lib 'macro-debugger/model/steps 'state-foci))
     (define step-term1 (lib 'macro-debugger/model/steps 'step-term1))
     (define step-term2 (lib 'macro-debugger/model/steps 'step-term2))
     (define misstep-exn (lib 'macro-debugger/model/steps 'misstep-exn))
     (define misstep-term1 (lib 'macro-debugger/model/steps 'misstep-term1))
     (define step-type->string (lib 'macro-debugger/model/steps 'step-type->string))
     (define width (let ([w (req-ref req 'width 80)]) (if (exact-positive-integer? w) w 80)))
     (define show-all? (equal? (req-ref req 'policy "standard") "all"))
     (define dir (directory-of source-name))
     (define stx (parameterize ([current-load-relative-directory dir]) (read-program source source-name)))
     (define ns (fresh-namespace-for (module-language stx)))
     (define-values (result events derivp)
       (parameterize ([current-namespace ns]
                      [current-load-relative-directory dir]
                      [current-directory dir]
                      [current-output-port (open-output-nowhere)])
         (trace* stx)))
     (define deriv (force derivp))
     (define steps
       (parameterize ([macro-policy (if show-all? (λ (id) #t) (policy->predicate 'standard))])
         (filter (λ (s) (or (misstep? s)
                            (and (step? s)
                                 ;; Scope bookkeeping steps never change the printed term.
                                 (not (memq (protostep-type s) '(resolve-variable))))))
                 (reductions deriv))))
     (for/list ([s (in-list (take steps (min 400 (length steps))))])
       (cond
         [(step? s)
          (define-values (before before-hl) (render-term (step-term1 s) (state-foci (protostep-s1 s)) width))
          (define-values (after after-hl) (render-term (step-term2 s) (state-foci (step-s2 s)) width))
          (hasheq 'type (format "~a" (protostep-type s))
                  'name (step-type->string (protostep-type s))
                  'before before 'after after
                  'foci_before before-hl 'foci_after after-hl)]
         [else
          (define-values (before before-hl) (render-term (misstep-term1 s) (state-foci (protostep-s1 s)) width))
          (hasheq 'type "error"
                  'name (exn->message (misstep-exn s))
                  'before before 'after ""
                  'foci_before before-hl 'foci_after '())])))))

;; render-term : syntax (listof syntax) natural -> (values string (listof (list s e)))
(define (render-term stx foci width)
  (define table (make-hasheq))
  (define focus? (λ (x) (and (syntax? x) (memq x foci))))
  (define datum
    (let strip ([x stx])
      (define inner
        (cond
          [(syntax? x) (strip (syntax-e x))]
          [(pair? x) (cons (strip (car x)) (strip (cdr x)))]
          [(vector? x) (for/vector ([e (in-vector x)]) (strip e))]
          [(box? x) (box (strip (unbox x)))]
          [else x]))
      (cond
        [(focus? x)
         (cond
           [(pair? inner) (hash-set! table inner #t) inner]
           [else
            (define placeholder (string->uninterned-symbol "-placeholder"))
            (hash-set! table placeholder (list inner))
            placeholder])]
        [else inner])))
  (pretty-with-highlights datum table width))

;; ---------------------------------------------------------------------------
;; Module graph

(define (module-graph req)
  (with-program
   req
   (λ (source source-name offset)
     (define dir (directory-of source-name))
     (define ns (make-base-namespace))
     (define include-libs? (and (req-ref req 'include_libs #f) #t))
     (define root
       (parameterize ([current-namespace ns]
                      [current-load-relative-directory dir]
                      [current-directory dir]
                      [current-output-port (open-output-nowhere)])
         (cond
           [(string=? source "")
            (unless (path? source-name) (error 'module-graph "no file"))
            (module-declared? source-name #t)
            source-name]
           [else
            (define stx (read-program source source-name))
            (define name (if (path? source-name) source-name 'unsaved-editor))
            (parameterize ([current-module-declare-name (make-resolved-module-path name)])
              (eval (check-module-form stx 'ignored (and (path? source-name) source-name))))
            (if (path? source-name) source-name `(quote ,name))])))
     (define index (make-hash))
     (define nodes '())
     (define edges '())
     (define (node-id resolved)
       (or (hash-ref index resolved #f)
           (let ([i (hash-count index)])
             (hash-set! index resolved i)
             (set! nodes (cons (resolved->node resolved) nodes))
             i)))
     (parameterize ([current-namespace ns])
       (define root-name (resolved-module-path-name (module-path-index-resolve (module-path-index-join root #f))))
       (let visit ([name root-name] [mp root])
         (define from (node-id name))
         (when (< (hash-count index) 2000)
           (for* ([phase+imports (in-list (with-handlers ([exn:fail? (λ (e) '())]) (module->imports mp)))]
                  [mpi (in-list (cdr phase+imports))])
             (define resolved (with-handlers ([exn:fail? (λ (e) #f)])
                                (resolved-module-path-name (module-path-index-resolve mpi))))
             (when (and resolved (or include-libs? (not (library-module? resolved))))
               (define seen? (hash-ref index resolved #f))
               (define to (node-id resolved))
               (unless (member (list from to) edges)
                 (set! edges (cons (list from to) edges)))
               (unless seen?
                 (visit resolved (resolved->module-path resolved))))))))
     (hasheq 'nodes (reverse nodes) 'edges (reverse edges)))))

(define (resolved->module-path r)
  (cond
    [(path? r) r]
    [(symbol? r) `(quote ,r)]
    [(and (pair? r) (path? (car r))) `(submod ,(car r) ,@(cdr r))]
    [(pair? r) `(submod (quote ,(car r)) ,@(cdr r))]
    [else r]))

(define library-roots
  (let ([find-collects-dir (dynamic-require 'setup/dirs 'find-collects-dir)]
        [find-pkgs-dir (dynamic-require 'setup/dirs 'find-pkgs-dir)]
        [find-user-pkgs-dir (dynamic-require 'setup/dirs 'find-user-pkgs-dir)])
    (for/list ([p (list (find-collects-dir) (find-pkgs-dir) (find-user-pkgs-dir))] #:when (path? p))
      (path->string (simplify-path p)))))

(define (library-module? r)
  (define p (if (pair? r) (car r) r))
  (or (symbol? p)
      (and (path? p)
           (let ([s (path->string p)])
             (for/or ([root (in-list library-roots)]) (string-prefix? s root))))))

(define (resolved->node r)
  (define p (if (pair? r) (car r) r))
  (hasheq 'path (if (path? p) (path->string p) (json-null))
          'label (cond
                   [(path? p)
                    (define-values (base name dir?) (split-path p))
                    (define file (path->string name))
                    (if (pair? r) (format "~a ~a" file (string-join (map ~a* (cdr r)) " ")) file)]
                   [else (format "~a" r)])
          'lib (library-module? r)))

(define (~a* v) (format "~a" v))

;; ---------------------------------------------------------------------------
;; Jump to a definition in another file

(define (jump-target req)
  (define path (req-ref req 'path #f))
  (define id (format "~a" (req-ref req 'name "")))
  (unless (and (string? path) (file-exists? path))
    (error 'jump-target "no such file: ~a" path))
  (define text (call-with-input-file path port->string))
  (define header (drracket-header-length text))
  (define source-name (make-source-name path))
  (parameterize ([main-source source-name]
                 [source-offset header])
    (define-values (collector errors) (run-check-syntax text source-name header))
    (define hit (for/first ([d (in-list (send collector definition-entries))]
                            #:when (equal? (hash-ref d 'name) id))
                  d))
    (if hit
        (let ([pos (hash-ref hit 'start)])
          (hasheq 'pos pos
                  'header_length header
                  'line (add1 (for/sum ([c (in-string text header (min (string-length text) (+ header pos)))])
                                (if (char=? c #\newline) 1 0)))))
        (json-null))))

;; DrRacket's teaching-language files start with a 3-line metadata header that the editor
;; hides; positions we report for such files are relative to the text after it.
(define (drracket-header-length text)
  (define m (regexp-match-positions #rx"^;;[^\n]*\n;;[^\n]*\n#reader[^\n]*\n" text))
  (if m (cdar m) 0))

(module+ main (main))

;; ---------------------------------------------------------------------------
;; DrRacket's editor format (WXME): files with pasted images, comment boxes, fractions...
;;
;; wxme-read turns such a file into program text for the IDE: images become characters from
;; Supplementary Private Use Area-A plus their PNG data (see common.rkt), comment boxes become
;; #| |# comments, and number snips become their value. Boxes the IDE cannot edit (XML,
;; Racket and test boxes) are listed in `unsupported` so the file opens read-only.

(struct comment-box-text (text))

(define (wxme-read req)
  (define path (req-ref req 'path #f))
  (define wxme-port->port (dynamic-require 'wxme 'wxme-port->port))
  (define readable<%> (dynamic-require 'wxme 'readable<%>))
  (define image-snip% (dynamic-require 'racket/snip 'image-snip%))
  (define out (open-output-string))
  (define images (make-hasheq))
  (define next #xF0000)
  (define comments 0)
  (define unsupported '())
  (call-with-input-file path
    (λ (in)
      ;; Comment boxes must be caught here: past the filter they are only special comments.
      (define p (wxme-port->port in #t (λ (s)
                                          (if (and (object? s) (object-method-arity-includes? s 'get-content-port 0))
                                              (comment-box-text (port->string (send s get-content-port)))
                                              s))))
      (let loop ([pos 0])
        (define c (read-char-or-special p))
        (cond
          [(eof-object? c) (void)]
          [(char? c) (write-char c out) (loop (add1 pos))]
          [(is-a? c image-snip%)
           (define bm (send c get-bitmap))
           (define png (let ([o (open-output-bytes)]) (and bm (send bm save-file o 'png) (get-output-bytes o))))
           (cond
             [png
              (define ch (integer->char next))
              (set! next (add1 next))
              (hash-set! images (string->symbol (number->string (char->integer ch))) (bytes->string/latin-1 (base64-encode png #"")))
              (write-char ch out)]
             [else (set! unsupported (cons "image" unsupported))])
           (loop (add1 pos))]
          [(comment-box-text? c)
           (define text (comment-box-text-text c))
           (set! comments (add1 comments))
           (write-string (string-append "#| " (string-replace text "|#" "| #") " |#") out)
           (loop (add1 pos))]
          [(is-a? c readable<%>)
           (define v (with-handlers ([(λ (e) #t) (λ (e) e)]) (send c read-special path #f #f pos)))
           (cond
             [(number? v) (write-string (number->string v) out)]
             [else (set! unsupported (cons (format "~a" (object-name c)) unsupported))])
           (loop (add1 pos))]
          [else
           (set! unsupported (cons (format "~a" c) unsupported))
           (loop (add1 pos))]))))
  (hasheq 'text (get-output-string out)
          'images images
          'comments comments
          'unsupported (remove-duplicates (reverse unsupported))))

;; wxme-write writes program text with embedded images back in DrRacket's format.
(define (wxme-write req)
  (define path (req-ref req 'path #f))
  (define text (string-of-source (req-ref req 'text "")))
  (define images (decode-source-images (req-ref req 'images #f)))
  (define text% (dynamic-require 'racket/gui/base 'text%))
  (define image-snip% (dynamic-require 'racket/gui/base 'image-snip%))
  (define read-bitmap (dynamic-require 'racket/draw 'read-bitmap))
  (define t (new text%))
  (define start 0)
  (define (flush! k)
    (when (> k start) (send t insert (substring text start k) (send t last-position))))
  (for ([c (in-string text)] [k (in-naturals)])
    (when (and (image-char? c) (hash-ref images c #f))
      (flush! k)
      (set! start (add1 k))
      (send t insert (make-object image-snip% (read-bitmap (open-input-bytes (hash-ref images c)) 'png)) (send t last-position))))
  (flush! (string-length text))
  (unless (send t save-file path 'standard #t)
    (error 'wxme-write "could not save ~a" path))
  (hasheq 'saved #t))
