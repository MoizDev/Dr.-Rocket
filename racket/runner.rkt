#lang racket/base
;; Racketeer runner process: runs one program (DrRacket's "Run"), then serves interactions
;; in the program's namespace. One process per Run; the IDE kills it on the next Run.
;; See DESIGN.md ("runner") for the protocol.

(require json
         racket/async-channel
         racket/list
         racket/port
         racket/pretty
         racket/string
         net/base64
         syntax/modread
         setup/dirs
         "common.rkt")

;; ---------------------------------------------------------------------------
;; Protocol plumbing

(define protocol-in (current-input-port))

;; Everything user code writes is buffered and coalesced into `output` events. Any other
;; event flushes the buffer first so the IDE sees output and results in program order.
(define out-lock (make-semaphore 1))
(define pending-stream #f)
(define pending (open-output-string))
(define pending-size 0)
(define flush-threshold 65536)

(define (flush-pending!/locked)
  (when (and pending-stream (> pending-size 0))
    (emit (hasheq 'ev "output" 'stream pending-stream 'text (bytes->string/utf-8 (get-output-bytes pending #t) #\?))))
  (set! pending-size 0)
  (set! pending-stream #f))

(define (flush-output-events!)
  (call-with-semaphore out-lock flush-pending!/locked))

(define (buffer-output! stream str)
  (unless (string=? str "")
    (call-with-semaphore
     out-lock
     (λ ()
       (unless (equal? stream pending-stream)
         (flush-pending!/locked)
         (set! pending-stream stream))
       (write-string str pending)
       (set! pending-size (+ pending-size (string-length str)))
       (when (> pending-size flush-threshold)
         (flush-pending!/locked))))))

;; emit* : like emit, but keeps ordering with buffered output.
(define (emit* msg)
  (flush-output-events!)
  (emit msg))

(define (start-flusher!)
  (thread (λ () (let loop () (sleep 0.025) (flush-output-events!) (loop)))))

;; ---------------------------------------------------------------------------
;; Values: images and number markup
;;
;; These are looked up in the *user's* namespace so struct properties and predicates are
;; the same instances the program's values carry.

(define user-namespace #f)
(define convertible?* (λ (v) #f))
(define convert* (λ (v fmt) #f))
(define number-markup?* (λ (v) #f))
(define number-markup-number* #f)

(define (user-dynamic-require mod sym [fail #f])
  (with-handlers ([exn:fail? (λ (e) fail)])
    (parameterize ([current-namespace user-namespace])
      (dynamic-require mod sym))))

(define (init-value-support!)
  (set! convertible?* (user-dynamic-require 'file/convertible 'convertible? convertible?*))
  (set! convert* (user-dynamic-require 'file/convertible 'convert convert*)))

;; After the program is loaded, pick up number markup if the program's language uses it.
(define (init-markup-support!)
  (when (module-declared?* 'simple-tree-text-markup/data)
    (set! number-markup?* (user-dynamic-require 'simple-tree-text-markup/data 'number-markup? number-markup?*))
    (set! number-markup-number* (user-dynamic-require 'simple-tree-text-markup/data 'number-markup-number #f))))

(define (module-declared?* mod)
  (parameterize ([current-namespace user-namespace])
    (module-declared? mod #f)))

(define image-cache (make-weak-hasheq))

;; value->image : any -> (or/c #f hash?)  -- an image segment
(define (value->image v)
  (cond
    [(or (string? v) (bytes? v) (number? v) (symbol? v) (boolean? v) (null? v) (pair? v)) #f]
    [(hash-ref image-cache v #f) => (λ (x) (and (hash? x) x))]
    [else
     (define seg
       (and (convertible?* v)
            (let ([png (with-handlers ([(λ (e) #t) (λ (e) #f)]) (convert* v 'png-bytes))])
              (and (bytes? png) (png-segment png)))))
     (hash-set! image-cache v (or seg 'none))
     seg]))

(define (png-segment png)
  (define-values (w h)
    (if (>= (bytes-length png) 24)
        (values (integer-bytes->integer png #f #t 16 20) (integer-bytes->integer png #f #t 20 24))
        (values 0 0)))
  (hasheq 'png (bytes->string/latin-1 (base64-encode png #"")) 'width w 'height h))

(define (number-markup->text v)
  (define n (and number-markup-number* (number-markup-number* v)))
  (cond
    [(not (number? n)) (format "~a" v)]
    [(and (exact? n) (rational? n) (not (integer? n)) (eq? fraction-view 'decimal))
     (or (rational->decimal-string n) (number->string n))]
    [else (number->string n)]))

;; 'decimal or 'mixed, from the teaching language's settings (pretty-print-exact-as-decimal).
(define fraction-view 'decimal)

;; ---------------------------------------------------------------------------
;; User ports

(define (utf-8-complete-prefix-length bs)
  ;; Length of the longest prefix of `bs` that does not end inside a UTF-8 sequence.
  (define n (bytes-length bs))
  (let loop ([i (sub1 n)] [back 0])
    (cond
      [(or (< i 0) (= back 4)) n]
      [else
       (define b (bytes-ref bs i))
       (cond
         [(< b #x80) n]
         [(= (bitwise-and b #xC0) #x80) (loop (sub1 i) (add1 back))]
         [else
          (define need (cond [(>= b #xF0) 4] [(>= b #xE0) 3] [else 2]))
          (if (>= (- n i) need) n i)])])))

;; make-user-output-port : string -> output-port
(define (make-user-output-port stream)
  (define leftover #"")
  (define (write-out bs start end non-block? breakable?)
    (define chunk (bytes-append leftover (subbytes bs start end)))
    (define k (utf-8-complete-prefix-length chunk))
    (set! leftover (subbytes chunk k))
    (buffer-output! stream (bytes->string/utf-8 (subbytes chunk 0 k) replacement-char))
    (- end start))
  (define (write-special v non-block? breakable?)
    (cond
      [(value->image v)
       => (λ (img)
            (emit* (hasheq 'ev "output" 'stream stream 'text "" 'image img)))]
      [(number-markup?* v) (buffer-output! stream (number-markup->text v))]
      [else (buffer-output! stream (format "~a" v))])
    #t)
  (define p (make-output-port stream always-evt write-out void write-special))
  ;; Images displayed or written directly go out as images.
  (define (image-aware handler)
    (λ (v port)
      (if (value->image v) (write-special v #f #f) (handler v port))))
  (port-display-handler p (image-aware (port-display-handler p)))
  (port-write-handler p (image-aware (port-write-handler p)))
  (port-print-handler p (let ([h (port-print-handler p)])
                          (case-lambda
                            [(v port) (if (value->image v) (write-special v #f #f) (h v port))]
                            [(v port depth) (if (value->image v) (write-special v #f #f) (h v port depth))])))
  p)

;; Input: a pipe fed by `input` commands. When user code blocks on an empty pipe we tell
;; the IDE so it can show an input box.
(define-values (input-pipe-in input-pipe-out) (make-pipe))
(define input-requested? #f)

(define (request-input!)
  (unless input-requested?
    (set! input-requested? #t)
    (emit* (hasheq 'ev "input-wanted"))))

(define user-input-port
  (make-input-port
   'stdin
   (λ (bs)
     (define n (read-bytes-avail!* bs input-pipe-in))
     (cond
       [(and (exact-integer? n) (zero? n))
        (request-input!)
        (wrap-evt input-pipe-in (λ (_) 0))]
       [else n]))
   #f
   void))

(define (feed-input! text)
  (set! input-requested? #f)
  (write-string text input-pipe-out)
  (flush-output input-pipe-out))

;; ---------------------------------------------------------------------------
;; Errors

(define runner-dir
  (let-values ([(base name dir?) (split-path (variable-reference->module-source (#%variable-reference)))])
    base))

(define errortrace-key* #f)

;; Frames from Racketeer itself and from installed Racket libraries are not the user's code;
;; DrRacket hides them from the stack as well.
(define internal-roots
  (for/list ([p (list* runner-dir (find-collects-dir) (find-pkgs-dir) (find-user-pkgs-dir)
                       (find-system-path 'collects-dir) (get-collects-search-dirs))]
             #:when (path? p))
    (path->string (simplify-path (path->complete-path p)))))

(define (internal-runner-source? src)
  (and (path? src) (string-prefix? (path->string src) (path->string runner-dir))))

(define (internal-source? src)
  (and (path? src)
       (let ([s (path->string src)])
         (for/or ([root (in-list internal-roots)]) (string-prefix? s root)))))

(define (exn-kind e)
  (cond
    [(exn:fail:read? e) "read"]
    [(exn:fail:syntax? e) "syntax"]
    [(exn:break? e) "break"]
    [else "runtime"]))

(define (exn-srclocs e)
  (if (exn:srclocs? e)
      (for/list ([sl (in-list ((exn:srclocs-accessor e) e))] #:when (srcloc? sl))
        (srcloc->jsexpr sl))
      '()))

(define (exn-stack e)
  (define marks (exn-continuation-marks e))
  (define et
    (if (and errortrace-key* marks)
        (for/list ([m (in-list (continuation-mark-set->list marks errortrace-key*))]
                   #:when (and (list? m) (= (length m) 6) (not (internal-source? (cadr m)))))
          (define-values (datum src line col pos span) (apply values m))
          (hash-set (srcloc->jsexpr (srcloc src line col pos span))
                    'name (truncate-string (format "~s" datum) 80)))
        '()))
  (cond
    [(pair? et) et]
    [marks
     (for/list ([f (in-list (continuation-mark-set->context marks))]
                #:when (and (cdr f) (not (internal-source? (srcloc-source (cdr f))))))
       (hash-set (srcloc->jsexpr (cdr f)) 'name (if (car f) (symbol->string (car f)) (json-null))))]
    [else '()]))

(define rewrite-error-message #f)

(define (report-error! e)
  (cond
    [(exn? e)
     (define msg (if rewrite-error-message
                     (with-handlers ([(λ (x) #t) (λ (x) (exn-message e))]) (rewrite-error-message e))
                     (exn-message e)))
     (define stack (if (or (exn:fail:syntax? e) (exn:fail:read? e)) '() (exn-stack e)))
     (define srclocs
       (let ([direct (exn-srclocs e)])
         (cond
           [(pair? direct) direct]
           [(pair? stack) (list (hash-remove (car stack) 'name))]
           [else '()])))
     (emit* (hasheq 'ev "error" 'message msg 'kind (exn-kind e) 'srclocs srclocs 'stack stack))]
    [else
     (emit* (hasheq 'ev "error" 'message (format "uncaught exception: ~e" e) 'kind "runtime"
                    'srclocs '() 'stack '()))]))

;; ---------------------------------------------------------------------------
;; Value printing

(define default-print (current-print))
(define default-global-print (global-port-print-handler))
(define print-width 80)

(define (capture-printed thunk)
  ;; Run `thunk` with output going to a port that records text and image segments.
  (define segments '())
  (define text (open-output-string))
  (define (flush-text!)
    (define s (bytes->string/utf-8 (get-output-bytes text #t) #\?))
    (unless (string=? s "")
      (set! segments (cons (hasheq 'text s) segments))))
  (define port
    (make-output-port
     'value
     always-evt
     (λ (bs start end non-block? breakable?)
       (write-bytes bs text start end)
       (- end start))
     void
     (λ (v non-block? breakable?)
       (cond
         [(value->image v) => (λ (img) (flush-text!) (set! segments (cons img segments)))]
         [(number-markup?* v) (write-string (number-markup->text v) text)]
         [else (write-string (format "~a" v) text)])
       #t)))
  (parameterize ([current-output-port port])
    (thunk))
  (flush-output port)
  (flush-text!)
  (reverse segments))

(define (image-size-hook old)
  (λ (v display? port)
    (if (and (port-writes-special? port) (value->image v)) 1 (old v display? port))))

(define (image-print-hook old)
  (λ (v display? port)
    (if (and (port-writes-special? port) (value->image v))
        (write-special v port)
        (old v display? port))))

;; make-value-printer : (any -> void) -> (any -> void)
(define (make-value-printer lang-print)
  (define default? (eq? lang-print default-print))
  (λ (v)
    (unless (void? v)
      (define img (value->image v))
      (define segments
        (if img
            (list img)
            (capture-printed
             (λ ()
               (parameterize ([pretty-print-columns print-width]
                              [pretty-print-size-hook (image-size-hook (pretty-print-size-hook))]
                              [pretty-print-print-hook (image-print-hook (pretty-print-print-hook))])
                 (cond
                   [(not default?) (lang-print v)]
                   ;; A language that installs its own print handler (the teaching
                   ;; languages do) is printed through it; otherwise pretty-print.
                   [(eq? (global-port-print-handler) default-global-print) (pretty-print v)]
                   [else (print v)]))))))
      (define trimmed (trim-trailing-newline segments))
      (emit* (hasheq 'ev "value"
                     'text (apply string-append (for/list ([s trimmed]) (hash-ref s 'text "")))
                     'segments trimmed)))))

(define (trim-trailing-newline segs)
  (cond
    [(null? segs) segs]
    [else
     (define last-seg (last segs))
     (define t (hash-ref last-seg 'text #f))
     (cond
       [(and t (string-suffix? t "\n"))
        (define s (substring t 0 (sub1 (string-length t))))
        (append (drop-right segs 1) (if (string=? s "") '() (list (hash-set last-seg 'text s))))]
       [else segs])]))

;; ---------------------------------------------------------------------------
;; Test results (htdp/deinprogramm test engine)
;;
;; The teaching languages report check-expect results as "markup" through
;; display-test-results-parameter. We render it to plain text ourselves (frames become
;; inline values) and pick out each failure's message and source location.

(define tests-hooked? #f)
(define test-api (make-hasheq))

(define (test-fn name) (hash-ref test-api name #f))

(define (hook-test-display!)
  (when (and (not tests-hooked?) (module-declared?* 'test-engine/test-markup))
    (set! tests-hooked? #t)
    (for ([spec (in-list '((test-engine/test-markup display-test-results-parameter
                                                    get-rewritten-error-message)
                           (test-engine/test-engine current-test-object test-object-tests
                                                    test-object-failed-checks test-object-copy
                                                    test-object=? run-tests!)
                           (test-engine/syntax test-display-results!)
                           (simple-tree-text-markup/data
                            empty-markup? horizontal-markup? horizontal-markup-markups
                            vertical-markup? vertical-markup-markups srcloc-markup?
                            srcloc-markup-srcloc srcloc-markup-markup framed-markup?
                            framed-markup-markup image-markup? image-markup-alt-markup
                            number-markup? number-markup-number number-markup-fraction-view)))])
      (for ([name (in-list (cdr spec))])
        (hash-set! test-api name (user-dynamic-require (car spec) name))))
    (when (test-fn 'get-rewritten-error-message)
      (set! rewrite-error-message (test-fn 'get-rewritten-error-message)))
    (define param (test-fn 'display-test-results-parameter))
    (when (and param (test-fn 'current-test-object) (test-fn 'vertical-markup?))
      (param report-test-results!))))

(define (report-test-results! markup)
  (define to ((test-fn 'current-test-object)))
  (define total (length ((test-fn 'test-object-tests) to)))
  (define failed (length ((test-fn 'test-object-failed-checks) to)))
  (emit* (hasheq 'ev "tests"
                 'total total
                 'passed (max 0 (- total failed))
                 'text (markup->text markup)
                 'failures (markup->failures markup))))

;; markup->text : markup -> string
(define (markup->text m)
  (define (call name . args) (apply (test-fn name) args))
  (let render ([m m])
    (cond
      [(string? m) m]
      [(call 'empty-markup? m) ""]
      [(call 'horizontal-markup? m)
       (apply string-append (map render (call 'horizontal-markup-markups m)))]
      [(call 'vertical-markup? m)
       (string-join (map render (call 'vertical-markup-markups m)) "\n")]
      [(call 'srcloc-markup? m) (srcloc->text (call 'srcloc-markup-srcloc m))]
      [(call 'framed-markup? m) (render (call 'framed-markup-markup m))]
      [(call 'image-markup? m) (render (call 'image-markup-alt-markup m))]
      [(call 'number-markup? m)
       (define n (call 'number-markup-number m))
       (if (and (exact? n) (rational? n) (not (integer? n))
                (eq? (call 'number-markup-fraction-view m) 'decimal))
           (or (rational->decimal-string n) (number->string n))
           (number->string n))]
      [else (format "~a" m)])))

;; Verticals are flattened by the markup constructors, so a failure is the run of
;; non-text lines immediately preceding its srcloc link.
(define (markup->failures markup)
  (define (call name . args) (apply (test-fn name) args))
  (define failures '())
  (let walk ([m markup])
    (cond
      [(call 'vertical-markup? m)
       (let loop ([kids (call 'vertical-markup-markups m)] [run '()])
         (unless (null? kids)
           (define k (car kids))
           (cond
             [(call 'srcloc-markup? k)
              (define msg (string-join (for/list ([r (in-list (reverse run))])
                                         (string-trim (markup->text r)))
                                       "\n"))
              (set! failures (cons (hasheq 'message msg
                                           'srcloc (srcloc->jsexpr (call 'srcloc-markup-srcloc k)))
                                   failures))
              (loop (cdr kids) '())]
             [(or (string? k) (call 'empty-markup? k)
                  (string=? (string-trim (markup->text k)) ""))
              (loop (cdr kids) '())]
             [else (loop (cdr kids) (cons k run))])))]
      [(call 'horizontal-markup? m) (for-each walk (call 'horizontal-markup-markups m))]
      [else (void)]))
  (reverse failures))

;; ---------------------------------------------------------------------------
;; Commands

(define eval-requests (make-async-channel))
(define debug-actions (make-async-channel))
(define user-thread #f)
(define run-started (make-semaphore 0))
(define run-command #f)
(define repl-namespace #f)

(define (reader-loop)
  (let loop ()
    (define cmd (read-command protocol-in))
    (cond
      [(eof-object? cmd)
       (flush-output-events!)
       (exit 0)]
      [else
       (case (hash-ref cmd 'cmd #f)
         [("run")
          (unless run-command
            (set! run-command cmd)
            (semaphore-post run-started))]
         [("eval") (async-channel-put eval-requests cmd)]
         [("input") (feed-input! (string-of-source (hash-ref cmd 'text "")))]
         [("input-eof") (close-output-port input-pipe-out)]
         [("break") (when user-thread (break-thread user-thread))]
         [("debug") (debug-command! (hash-ref cmd 'action "continue"))]
         [("breakpoints") (set-breakpoints! (hash-ref cmd 'positions '()))]
         [("complete") (complete! cmd)]
         [else (void)])
       (loop)])))

(define (complete! cmd)
  (define prefix (string-of-source (hash-ref cmd 'prefix "")))
  (define ns repl-namespace)
  (define items
    (if ns
        (sort (for/list ([s (in-list (namespace-mapped-symbols ns))]
                         #:when (string-prefix? (symbol->string s) prefix))
                (symbol->string s))
              string<?)
        '()))
  (emit* (hasheq 'ev "completions" 'id (hash-ref cmd 'id 0) 'items (take items (min 200 (length items))))))

;; ---------------------------------------------------------------------------
;; Debugger (gui-debugger annotator)

(define debug-mode 'run)        ; 'run | 'step-into | (cons 'over depth) | (cons 'out depth) | 'pause
(define breakpoints (make-hash)) ; racket 1-based positions
(define requested-breakpoints '())
(define break-positions '())     ; valid break positions from the annotator
(define top-level-vars '())     ; (list (cons id rd/wr))
(define debug-key* #f)
(define mark-source* #f)
(define mark-bindings* #f)
(define mark-binding-binding* #f)
(define mark-binding-value* #f)

(define (debug-command! action)
  (define a (string->symbol action))
  (if (eq? a 'pause)
      (set! debug-mode 'pause)
      (async-channel-put debug-actions a)))

(define (snap-breakpoint pos)
  ;; IDE position (0-based, visible buffer) -> nearest valid racket position at or after it.
  (define rp (+ pos 1 (source-offset)))
  (define candidates (filter (λ (p) (>= p rp)) break-positions))
  (if (null? candidates) rp (apply min candidates)))

(define (set-breakpoints! positions)
  (set! requested-breakpoints (filter exact-nonnegative-integer? (if (list? positions) positions '())))
  (hash-clear! breakpoints)
  (for ([p (in-list requested-breakpoints)])
    (hash-set! breakpoints (snap-breakpoint p) #t)))

(define (load-debugger!)
  (define marks (λ (s) (dynamic-require 'gui-debugger/marks s)))
  (set! debug-key* (marks 'debug-key))
  (set! mark-source* (marks 'mark-source))
  (set! mark-bindings* (marks 'mark-bindings))
  (set! mark-binding-binding* (marks 'mark-binding-binding))
  (set! mark-binding-value* (marks 'mark-binding-value)))

(define (value->short-string v)
  (define o (open-output-string))
  (with-handlers ([(λ (e) #t) (λ (e) (write-string "#<unprintable>" o))])
    (parameterize ([print-graph #f] [pretty-print-columns 'infinity])
      (print v o)))
  (truncate-string (get-output-string o) 300))

(define (pause! marks result)
  (define top (car marks))
  (define src (mark-source* top))
  (define locals
    (append
     (for/list ([b (in-list (mark-bindings* top))])
       (hasheq 'name (format "~a" (syntax-e (mark-binding-binding* b)))
               'value (value->short-string (mark-binding-value* b))))
     (for/list ([tv (in-list (reverse top-level-vars))])
       (hasheq 'name (format "~a" (syntax-e (car tv)))
               'value (with-handlers ([(λ (e) #t) (λ (e) "#<undefined>")])
                        (value->short-string ((cdr tv))))))))
  (define stack
    (for/list ([m (in-list marks)])
      (define s (mark-source* m))
      (hasheq 'name (syntax->label s)
              'srcloc (syntax->srcloc-jsexpr s))))
  (emit* (hasheq 'ev "paused"
                 'srcloc (syntax->srcloc-jsexpr src)
                 'locals locals
                 'stack stack
                 'result (if result
                             (string-join (map value->short-string result) " ")
                             (json-null))))
  (define action (async-channel-get debug-actions))
  (define depth (length marks))
  (set! debug-mode
        (case action
          [(step-into) 'step-into]
          [(step-over) (cons 'over depth)]
          [(step-out) (cons 'out depth)]
          [else 'run]))
  (emit* (hasheq 'ev "resumed")))

(define (should-pause? marks pos)
  (define depth (length marks))
  (or (and pos (hash-ref breakpoints pos #f))
      (match-mode depth)))

(define (match-mode depth)
  (define m debug-mode)
  (cond
    [(memq m '(step-into pause)) #t]
    [(and (pair? m) (eq? (car m) 'over)) (<= depth (cdr m))]
    [(and (pair? m) (eq? (car m) 'out)) (< depth (cdr m))]
    [else #f]))

(define (debug-annotate expanded source-name)
  (define annotate-for-single-stepping
    (dynamic-require 'gui-debugger/annotator 'annotate-for-single-stepping))
  (define (break? src)
    (λ (pos) (or (not (eq? debug-mode 'run)) (hash-ref breakpoints pos #f))))
  (define (break-before top-mark ccm)
    (define marks (cons top-mark (continuation-mark-set->list ccm debug-key*)))
    (define pos (syntax-position (mark-source* top-mark)))
    (when (should-pause? marks pos)
      (pause! marks #f))
    #f)
  (define (break-after top-mark ccm . vals)
    (define marks (cons top-mark (continuation-mark-set->list ccm debug-key*)))
    (when (match-mode (length marks))
      (pause! marks vals))
    (apply values vals))
  (define-values (annotated posns)
    (annotate-for-single-stepping
     expanded break? break-before break-after
     (λ (type bound binding) (void))
     (λ (mod var rd/wr) (set! top-level-vars (cons (cons var rd/wr) top-level-vars)))
     source-name))
  (set! break-positions (sort posns <))
  (set-breakpoints! requested-breakpoints)
  annotated)

;; ---------------------------------------------------------------------------
;; Running the program

(define module-language-error
  (string-append "Module Language: only a module expression is allowed, either\n"
                 "    #lang <language-name>\n or\n    (module <name> <language> ...)\n"))

(define (read-module-syntax port source-name)
  (define stx
    (parameterize ([read-accept-reader #t]
                   [read-accept-lang #t])
      (read-syntax source-name port)))
  (when (eof-object? stx)
    (raise (exn:fail module-language-error (current-continuation-marks))))
  (define rest (parameterize ([read-accept-reader #t] [read-accept-lang #t])
                 (read-syntax source-name port)))
  (unless (eof-object? rest)
    (raise-syntax-error #f (string-trim module-language-error) rest))
  (set! coverage-body-start (module-body-start stx))
  (or (with-handlers ([exn:fail? (λ (e) #f)])
        (check-module-form stx 'ignored (and (path? source-name) source-name)))
      (raise-syntax-error #f (string-trim module-language-error) stx)))

;; Raw position where the module body begins; coverage ignores the `#lang` line.
(define coverage-body-start 0)

(define (module-body-start stx)
  (define parts (or (syntax->list stx) '()))
  (define body
    (cond
      [(< (length parts) 4) '()]
      [else
       (define rest (cdddr parts))
       (define first-form (and (pair? rest) (syntax->list (car rest))))
       (if (and (= (length rest) 1) first-form (pair? first-form)
                (identifier? (car first-form))
                (eq? (syntax-e (car first-form)) '#%module-begin))
           (cdr first-form)
           rest)]))
  (define positions (filter values (map syntax-position body)))
  (if (null? positions) 0 (apply min positions)))

(define (run-configure-runtime! mod-path)
  (define submod `(submod ,mod-path configure-runtime))
  (cond
    [(module-declared? submod #t) (dynamic-require submod #f)]
    [else
     (define info (module->language-info mod-path #t))
     (when (vector? info)
       (define get-info ((dynamic-require (vector-ref info 0) (vector-ref info 1)) (vector-ref info 2)))
       (define configs (get-info 'configure-runtime '()))
       (for ([c (in-list configs)])
         ((dynamic-require (vector-ref c 0) (vector-ref c 1)) (vector-ref c 2))))]))

(define (instrument-namespace! instrument)
  (when (memq instrument '(errortrace coverage))
    (define make-handler (dynamic-require 'errortrace/errortrace-lib 'make-errortrace-compile-handler))
    (set! errortrace-key* (dynamic-require 'errortrace/errortrace-key 'errortrace-key))
    ((dynamic-require 'errortrace/errortrace-lib 'instrumenting-enabled) #t)
    (when (eq? instrument 'coverage)
      ((dynamic-require 'errortrace/errortrace-lib 'coverage-counts-enabled) #t))
    (current-compile (make-handler))))

;; Like DrRacket, a character's coverage is that of the innermost instrumented expression
;; containing it. We paint expressions outermost-first and report maximal runs.
(define (emit-coverage!)
  (define get-coverage (dynamic-require 'errortrace/errortrace-lib 'get-coverage))
  (define-values (all executed) (get-coverage))
  (define (main? r) (same-source? (car r) (main-source)))
  (define ranges (filter (λ (r) (and (main? r) (>= (cadr r) coverage-body-start))) all))
  (define executed* (filter main? executed))
  (define hit (for/hash ([r (in-list executed*)]) (values (cdr r) #t)))
  ;; Forms such as `define` are never "executed" themselves; they count as covered when
  ;; anything inside them ran.
  (define (covered? r)
    (or (hash-ref hit (cdr r) #f)
        (for/or ([e (in-list executed*)])
          (and (<= (cadr r) (cadr e)) (<= (caddr e) (caddr r))))))
  (cond
    [(null? ranges)
     (emit* (hasheq 'ev "coverage" 'covered '() 'uncovered '()))]
    [else
     (define lo (apply min (map cadr ranges)))
     (define hi (apply max (map caddr ranges)))
     (define paint (make-vector (- hi lo) #f)) ; #f = not code, 'yes, 'no
     (for ([r (in-list (sort ranges > #:key (λ (r) (- (caddr r) (cadr r)))))])
       (define v (if (covered? r) 'yes 'no))
       (for ([i (in-range (- (cadr r) lo) (- (caddr r) lo))])
         (vector-set! paint i v)))
     (define (runs want)
       (let loop ([i 0] [start #f] [acc '()])
         (define here (and (< i (vector-length paint)) (eq? (vector-ref paint i) want)))
         (cond
           [(and here (not start)) (loop (add1 i) i acc)]
           [here (loop (add1 i) start acc)]
           [start
            (define pos (adjust-pos (main-source) (+ lo start)))
            (define acc* (cons (list pos (- i start)) acc))
            (if (< i (vector-length paint)) (loop (add1 i) #f acc*) (reverse acc*))]
           [(< i (vector-length paint)) (loop (add1 i) #f acc)]
           [else (reverse acc)])))
     (emit* (hasheq 'ev "coverage" 'covered (runs 'yes) 'uncovered (runs 'no)))]))

(define (profile-thunk* thunk)
  (define profile-thunk (dynamic-require 'profile 'profile-thunk))
  (define-values (profile-total-time profile-nodes node-id node-src node-self node-total
                                     node-callers node-callees edge-caller edge-callee)
    (apply values
           (for/list ([s '(profile-total-time profile-nodes node-id node-src node-self node-total
                                              node-callers node-callees edge-caller edge-callee)])
             (dynamic-require 'profile/structs s))))
  (profile-thunk
   thunk
   #:delay 0.001
   #:render
   (λ (prof order)
     (define nodes (filter (λ (n) (not (and (srcloc? (node-src n)) (internal-runner-source? (srcloc-source (node-src n)))))) (profile-nodes prof)))
     (define index (for/hasheq ([n (in-list nodes)] [i (in-naturals)]) (values n i)))
     (define (idxs edges get)
       (for*/list ([e (in-list edges)] [i (in-value (hash-ref index (get e) #f))] #:when i) i))
     (emit* (hasheq 'ev "profile"
                    'total_ms (exact->inexact (profile-total-time prof))
                    'nodes (for/list ([n (in-list nodes)])
                             (hasheq 'name (if (node-id n) (format "~a" (node-id n)) "???")
                                     'srcloc (if (srcloc? (node-src n)) (srcloc->jsexpr (node-src n)) (json-null))
                                     'self_ms (exact->inexact (node-self n))
                                     'total_ms (exact->inexact (node-total n))
                                     'callers (idxs (node-callers n) edge-caller)
                                     'callees (idxs (node-callees n) edge-callee))))))))

(define (setting h key default)
  (define v (hash-ref h key default))
  (if (eq? v (json-null)) default v))

;; The program text, for labelling debugger frames with what the user wrote.
(define program-source "")

(define (syntax->label s)
  (define pos (syntax-position s))
  (define span (syntax-span s))
  (define text
    (if (and pos span (same-source? (syntax-source s) (main-source))
             (<= (+ pos -1 span) (string-length program-source)))
        (substring program-source (sub1 pos) (+ pos -1 span))
        (format "~s" (syntax->datum s))))
  (truncate-string (regexp-replace* #px"\\s+" text " ") 60))

(define (run-program! cmd)
  (define source (string-of-source (hash-ref cmd 'source "")))
  (set! program-source source)
  (define offset (let ([o (hash-ref cmd 'offset 0)]) (if (exact-nonnegative-integer? o) (min o (string-length source)) 0)))
  (define source-name (make-source-name (hash-ref cmd 'path #f)))
  (define language (let ([l (hash-ref cmd 'language (hasheq))]) (if (hash? l) l (hasheq))))
  (define options (let ([o (hash-ref cmd 'options (hasheq))]) (if (hash? o) o (hasheq))))
  (define instrument (string->symbol (setting options 'instrument "errortrace")))
  (define submodules (map string->symbol (filter string? (setting options 'submodules '("main" "test")))))
  (set! print-width (let ([w (setting options 'print_width 80)]) (if (exact-positive-integer? w) w 80)))
  (set! requested-breakpoints (filter exact-nonnegative-integer? (setting options 'breakpoints '())))
  (source-offset offset)
  (source-line-offset (for/sum ([c (in-string source 0 offset)]) (if (char=? c #\newline) 1 0)))
  (main-source source-name)

  (define dir (if (path? source-name)
                  (let-values ([(base name dir?) (split-path source-name)]) (if (path? base) base (current-directory)))
                  (current-directory)))
  (define ns (make-base-empty-namespace))
  (set! user-namespace ns)
  ;; Share the printer with the program so language print settings and our image hooks
  ;; act on the same parameters.
  (namespace-attach-module (variable-reference->namespace (#%variable-reference)) 'racket/pretty ns)
  (set! repl-namespace ns)

  (define user-out (make-user-output-port "out"))
  (define user-err (make-user-output-port "err"))
  (define cust (make-custodian))
  (define memory-limit (setting options 'memory_limit_mb #f))
  (when (and (exact-positive-integer? memory-limit) (custodian-memory-accounting-available?))
    (custodian-limit-memory cust (* memory-limit 1024 1024) cust))

  (emit* (hasheq 'ev "started" 'racket_version (version)
                 'language (format "~a" (setting language 'kind "module"))))
  (parameterize ([current-custodian cust])
    (set! user-thread
          (thread
           (λ ()
             (parameterize ([current-namespace ns]
                            [current-output-port user-out]
                            [current-error-port user-err]
                            [current-input-port user-input-port]
                            [current-directory dir]
                            [current-load-relative-directory dir]
                            [current-command-line-arguments (vector)]
                            [error-print-source-location #f]
                            [exit-handler (λ (code)
                                            (flush-output user-out)
                                            (flush-output user-err)
                                            (emit* (hasheq 'ev "exit" 'code (if (exact-integer? code) code 0)))
                                            (exit (if (byte? code) code 0)))])
               (parameterize-break #t
                 (run-user! source source-name language instrument submodules user-out user-err)))))))
  (thread-wait user-thread)
  ;; The user thread only ends when its custodian is shut down (memory limit) or after an
  ;; internal failure.
  (emit* (hasheq 'ev "error" 'kind "runtime" 'srclocs '() 'stack '()
                 'message (if (custodian-shut-down? cust)
                              "The program ran out of memory and was stopped."
                              "The evaluation thread is no longer running.")))
  (emit* (hasheq 'ev "exit" 'code 1))
  (exit 1))

;; Runs in the user thread.
(define (run-user! source source-name language instrument submodules user-out user-err)
  (with-handlers ([(λ (e) #t) report-error!])
    (init-value-support!)
    (when (eq? instrument 'debug) (load-debugger!))
    (instrument-namespace! instrument))
  (define kind (setting language 'kind "module"))
  (define mod-name (if (path? source-name) source-name (string->symbol "unsaved-editor")))
  (define mod-path (if (path? source-name) source-name `(quote ,mod-name)))
  (define (guard thunk)
    (with-handlers ([(λ (e) #t) (λ (e) (report-error! e) #f)])
      (thunk)
      #t))
  (cond
    [(equal? kind "toplevel")
     (guard
      (λ ()
        (define init (string->symbol (setting language 'init "r5rs")))
        (namespace-require init)
        (when (eq? init 'r5rs)
          (with-handlers ([exn:fail? void]) (namespace-require 'r5rs/init)))
        (current-print (make-value-printer (current-print)))))
     (define port (open-input-string source))
     (port-count-lines! port)
     (guard
      (λ ()
        (for ([stx (in-producer (λ () (read-syntax source-name port)) eof-object?)])
          (eval-top-level stx))))]
    [else
     (define port (open-input-string source))
     (port-count-lines! port)
     (define loaded?
       (guard
        (λ ()
          (define stx (read-module-syntax port source-name))
          (parameterize ([current-module-declare-name (make-resolved-module-path mod-name)])
            (eval (if (eq? instrument 'debug)
                      (debug-annotate (expand stx) source-name)
                      stx)))
          (run-configure-runtime! mod-path)
          (set! fraction-view (if (pretty-print-exact-as-decimal) 'decimal 'mixed))
          (current-print (make-value-printer (current-print)))
          (init-markup-support!)
          (hook-test-display!)
          (if (eq? instrument 'profile)
              (profile-thunk* (λ () (dynamic-require mod-path #f)))
              (dynamic-require mod-path #f)))))
     (when loaded?
       (hook-test-display!)
       (for ([sub (in-list submodules)])
         (define sp `(submod ,mod-path ,sub))
         (guard (λ () (when (module-declared? sp #t) (dynamic-require sp #f))))))
     (when (eq? instrument 'coverage) (guard emit-coverage!))
     (set! repl-namespace
           (or (with-handlers ([exn:fail? (λ (e) #f)]) (module->namespace mod-path))
               (current-namespace)))])
  (current-namespace repl-namespace)
  (flush-output user-out)
  (flush-output user-err)
  (emit* (hasheq 'ev "ready" 'id (json-null)))
  (interaction-loop instrument user-out user-err))

(define (eval-top-level stx)
  (define form (namespace-syntax-introduce stx))
  (call-with-values (λ () (eval form)) (λ vals (for-each (current-print) vals))))

(define (eval-interaction stx)
  (define form (namespace-syntax-introduce
                (datum->syntax #f (cons (datum->syntax #f '#%top-interaction) stx) stx)))
  (call-with-values (λ () (eval form)) (λ vals (for-each (current-print) vals))))

(define (interaction-loop instrument user-out user-err)
  (let loop ()
    (define req (parameterize-break #f (async-channel-get eval-requests)))
    (define id (hash-ref req 'id (json-null)))
    (define port (open-input-string (string-of-source (hash-ref req 'source ""))))
    (port-count-lines! port)
    (define snapshot (test-object-snapshot))
    (with-handlers ([(λ (e) #t) report-error!])
      (parameterize ([main-source 'interactions])
        (for ([stx (in-producer (λ () (read-syntax 'interactions port)) eof-object?)])
          (eval-interaction stx))))
    (rerun-tests-if-changed snapshot)
    (when (eq? instrument 'coverage) (with-handlers ([(λ (e) #t) void]) (emit-coverage!)))
    (flush-output user-out)
    (flush-output user-err)
    (emit* (hasheq 'ev "ready" 'id id))
    (loop)))

;; Like DrRacket's teaching languages: check-expects typed at the prompt run immediately.
(define (test-object-snapshot)
  (and tests-hooked?
       (test-fn 'current-test-object)
       (test-fn 'test-object-copy)
       ((test-fn 'test-object-copy) ((test-fn 'current-test-object)))))

(define (rerun-tests-if-changed snapshot)
  (when (and snapshot (test-fn 'test-object=?) (test-fn 'run-tests!) (test-fn 'test-display-results!))
    (unless ((test-fn 'test-object=?) snapshot ((test-fn 'current-test-object)))
      (with-handlers ([(λ (e) #t) report-error!])
        ((test-fn 'run-tests!))
        ((test-fn 'test-display-results!))))))

;; ---------------------------------------------------------------------------

(define (main)
  (start-flusher!)
  (thread reader-loop)
  (semaphore-wait run-started)
  (run-program! run-command)
  ;; Keep serving (e.g. big-bang windows run in their own eventspace) until stdin closes.
  (sync never-evt))

(define replacement-char (integer->char #xFFFD))

;; "in file.rkt, line 4, column 0", with lines relative to the visible buffer.
(define (srcloc->text sl)
  (define j (srcloc->jsexpr sl))
  (define line (hash-ref j 'line))
  (define col (hash-ref j 'col))
  (define src (srcloc-source sl))
  (define where (format "line ~a, column ~a"
                        (if (eq? line (json-null)) "?" line)
                        (if (eq? col (json-null)) "?" col)))
  (if (path? src)
      (let-values ([(base name dir?) (split-path src)])
        (format "in ~a, ~a" name where))
      (format "at ~a" where)))

(module+ main (main))
