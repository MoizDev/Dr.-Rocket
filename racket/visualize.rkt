#lang racket/base
;; Visualizer: records a program's execution as a sequence of snapshots for the IDE to replay.
;;
;; The fully expanded program is annotated (only code from the user's own file): function
;; bodies report calls and returns, `if` tests (`cond` questions) report their answers, and
;; definitions, local bindings and `set!` are recorded. At each step the shadow call stack,
;; the global definitions and every value reachable from them are serialized. Heap objects
;; (cons cells, structs, vectors, boxes, hashes, images) get stable ids across steps, and an
;; object is only sent again when its contents change, so the IDE can animate it.

(require racket/list
         racket/string
         syntax/kerncase
         json
         "common.rkt")

(provide viz-annotate
         viz-annotate-top
         viz-start!
         viz-finish!
         set-viz-emitter!
         set-viz-image-encoder!
         set-viz-max-steps!
         viz-note-output!
         viz-note-result!)

;; ---------------------------------------------------------------------------
;; Configuration (set by the runner)

(define emitter emit)
(define (set-viz-emitter! f) (set! emitter f))
;; value -> (or/c #f (hasheq 'png .. 'width .. 'height ..))
(define image-encoder (λ (v) #f))
(define (set-viz-image-encoder! f) (set! image-encoder f))
(define max-steps 1500)
(define (set-viz-max-steps! n) (set! max-steps n))

(define source #f)
(define source-text "")
;; struct name (string) -> field names, from the program's define-struct / struct forms
(define struct-fields (make-hash (list (cons "posn" '("x" "y")))))

;; ---------------------------------------------------------------------------
;; Annotation

(define (from-user-file? stx)
  (and (syntax-position stx)
       (syntax-span stx)
       (same-source? (syntax-source stx) source)))

;; The program text a syntax object covers (#f if it is not from the user's file). Teaching
;; languages rebuild user code through macros, so `syntax-original?` is unreliable; instead
;; code counts as the user's when the file really contains it at that location.
(define (user-text stx)
  (and (from-user-file? stx)
       (let ([start (sub1 (syntax-position stx))] [end (+ (sub1 (syntax-position stx)) (syntax-span stx))])
         (and (<= 0 start end (string-length source-text))
              (substring source-text start end)))))

;; An identifier the user wrote (not one generated from their code, like `make-node`).
(define (user-syntax? id)
  (equal? (user-text id) (symbol->string (syntax-e id))))

(define (pos-of stx) (adjust-pos (syntax-source stx) (syntax-position stx)))
(define (span-of stx) (or (syntax-span stx) 0))

(define (rebuild stx datum)
  (datum->syntax stx datum stx stx))

(define (formals-ids formals)
  (let loop ([f (syntax-e formals)])
    (cond
      [(null? f) '()]
      [(pair? f) (cons (car f) (loop (let ([d (cdr f)]) (if (syntax? d) (syntax-e d) d))))]
      [(identifier? formals) (list formals)]
      [(syntax? f) (if (identifier? f) (list f) (loop (syntax-e f)))]
      [else '()])))

(define (getters ids)
  #`(#%plain-app list #,@(for/list ([id (in-list ids)])
                           #`(#%plain-app cons '#,(symbol->string (syntax-e id)) (#%plain-lambda () #,id)))))

(define (viz-annotate expanded src orig text)
  (set! source src)
  (set! source-text text)
  (collect-struct-fields! orig)
  (syntax-case expanded ()
    [(mod name lang mb)
     (let ([mb-parts (syntax->list #'mb)])
       (rebuild expanded
                (list #'mod #'name #'lang
                      (rebuild #'mb (cons (car mb-parts) (map annotate-top (cdr mb-parts)))))))]
    [_ expanded]))

;; One top-level form of a top-level language (R5RS, Pretty Big), already expanded.
(define (viz-annotate-top expanded src orig text)
  (set! source src)
  (set! source-text text)
  (collect-struct-fields! orig)
  (annotate-top expanded))

(define (annotate-top form)
  (kernel-syntax-case form #f
    [(define-values (id ...) e)
     (let* ([ids (syntax->list #'(id ...))]
            [shown (filter user-syntax? ids)]
            [name (and (= 1 (length ids)) (car ids))]
            [def (rebuild form (list (car (syntax-e form)) #'(id ...) (annotate-expr #'e name)))])
       (if (null? shown)
           def
           #`(begin #,def
                    (#%plain-app '#,viz-define! #,(getters shown) '#,(pos-of (car shown)) '#,(span-of (car shown))))))]
    [(begin e ...)
     (rebuild form (cons (car (syntax-e form)) (map annotate-top (syntax->list #'(e ...)))))]
    [(define-syntaxes . _) form]
    [(begin-for-syntax . _) form]
    [(#%require . _) form]
    [(#%provide . _) form]
    [(#%declare . _) form]
    [(module . _) form]
    [(module* . _) form]
    [_ (annotate-expr form #f)]))

;; Functions worth showing as frames: ones bound to a name the user wrote, and `lambda`s the
;; user wrote. (Teaching languages also generate lambdas, e.g. for check-expect.)
(define (user-function? whole name)
  (or (and name (user-syntax? name))
      (let ([t (user-text whole)])
        (and t (regexp-match? #rx"^[(\\[](lambda|λ)[ \n\t\r]" t)))))

(define (annotate-body body tail?)
  ;; The last expression of a body inherits the body's tail position.
  (for/list ([b (in-list body)] [k (in-naturals 1)])
    (annotate-expr b #f (and tail? (= k (length body))))))

(define (annotate-lambda-clause formals body name whole)
  (define bodies (annotate-body body #t))
  (cond
    [(and (user-function? whole name) (pair? body))
     (define first-body (last body))
     (define fname (if name (syntax-e name) 'λ))
     (list formals
           #`(#%plain-app '#,viz-call
                          '#,(format "~a" fname)
                          #,(getters (formals-ids formals))
                          '#,(pos-of first-body) '#,(span-of first-body)
                          (#%plain-lambda () #,@bodies)))]
    [else (cons formals bodies)]))

(define (annotate-bindings stx ids-list body tail?)
  ;; let-values / letrec-values: record user-visible local names in the current frame.
  (define shown (filter user-syntax? (append* ids-list)))
  (define bodies (annotate-body body tail?))
  (if (null? shown)
      bodies
      (cons #`(#%plain-app '#,viz-bind! #,(getters shown)) bodies)))

;; `tail?`: e is in tail position of a user function, so a call here replaces the caller's
;; frame (Racket runs tail calls without growing the stack, and so does the visualizer).
(define (annotate-expr e name [tail? #f])
  (kernel-syntax-case e #f
    [id (identifier? e) e]
    [(#%plain-lambda formals body ...)
     (let ([lam (rebuild e (cons (car (syntax-e e)) (annotate-lambda-clause #'formals (syntax->list #'(body ...)) name e)))])
       (if (user-function? e name)
           ;; Register the closure as a user function (so applications of it aren't recorded
           ;; as primitive operations); keep its name for printing and error messages.
           #`(#%plain-app '#,viz-register! #,(if name (syntax-property lam 'inferred-name (syntax-e name)) lam))
           lam))]
    [(case-lambda [formals body ...] ...)
     (rebuild e (cons (car (syntax-e e))
                      (for/list ([f (in-list (syntax->list #'(formals ...)))]
                                 [b (in-list (syntax->list #'((body ...) ...)))])
                        (annotate-lambda-clause f (syntax->list b) name e))))]
    [(if t a b)
     (let ([t* (annotate-expr #'t #f)])
       (rebuild e (list (car (syntax-e e))
                        (let ([site (question-site #'t)])
                          (if site
                              #`(#%plain-app '#,viz-test #,t* '#,(pos-of site) '#,(span-of site))
                              t*))
                        (annotate-expr #'a #f tail?)
                        (annotate-expr #'b #f tail?))))]
    [(begin x ...) (rebuild e (cons (car (syntax-e e)) (annotate-body (syntax->list #'(x ...)) tail?)))]
    [(begin0 x ...)
     ;; `(begin0 e)` (how teaching languages wrap function bodies) keeps e in tail position.
     (let ([xs (syntax->list #'(x ...))])
       (rebuild e (cons (car (syntax-e e)) (for/list ([x (in-list xs)]) (annotate-expr x #f (and tail? (= 1 (length xs))))))))]
    [(let-values ([(id ...) rhs] ...) body ...)
     (let ([idss (map syntax->list (syntax->list #'((id ...) ...)))])
       (rebuild e (list* (car (syntax-e e))
                         (rebind #'([(id ...) rhs] ...) idss)
                         (annotate-bindings e idss (syntax->list #'(body ...)) tail?))))]
    [(letrec-values ([(id ...) rhs] ...) body ...)
     (let ([idss (map syntax->list (syntax->list #'((id ...) ...)))])
       (rebuild e (list* (car (syntax-e e))
                         (rebind #'([(id ...) rhs] ...) idss)
                         (annotate-bindings e idss (syntax->list #'(body ...)) tail?))))]
    [(set! id rhs)
     (let ([set (rebuild e (list (car (syntax-e e)) #'id (annotate-expr #'rhs #f)))])
       (if (user-syntax? #'id)
           #`(begin #,set (#%plain-app '#,viz-set! '#,(symbol->string (syntax-e #'id)) (#%plain-lambda () id) '#,(pos-of e) '#,(span-of e)))
           set))]
    [(with-continuation-mark k v b)
     (rebuild e (list (car (syntax-e e)) (annotate-expr #'k #f) (annotate-expr #'v #f) (annotate-expr #'b #f tail?)))]
    [(#%plain-app x ...)
     (let ([parts (map (λ (x) (annotate-expr x #f)) (syntax->list #'(x ...)))]
           [text (user-text e)])
       ;; Only applications the user wrote: definition forms like define-struct generate
       ;; applications that carry the form's location but aren't calls in the program.
       (if (and text (regexp-match? #rx"^[(]" text) (not (definition-text? text)) (pair? parts))
           ;; Remember the call site (after the arguments are evaluated) for the call step.
           (let ([temps (generate-temporaries parts)])
             #`(let-values #,(for/list ([t (in-list temps)] [p (in-list parts)]) #`[(#,t) #,p])
                 (#%plain-app '#,viz-site! '#,(pos-of e) '#,(span-of e) '#,tail?)
                 ;; Primitive operations are recorded with their arguments and result; calls
                 ;; to the user's functions run directly (keeping tail calls tail calls).
                 (if (#%plain-app '#,viz-primitive? #,(car temps))
                     (#%plain-app '#,viz-compute '#,(pos-of e) '#,(span-of e) #,(car temps) (#%plain-app list #,@(cdr temps)))
                     #,(rebuild e (cons (car (syntax-e e)) temps)))))
           (rebuild e (cons (car (syntax-e e)) parts))))]
    [(#%expression x) (rebuild e (list (car (syntax-e e)) (annotate-expr #'x #f tail?)))]
    [_ e]))

(define (rebind clauses idss)
  (rebuild clauses
           (for/list ([c (in-list (syntax->list clauses))]
                      [ids (in-list idss)])
             (syntax-case c ()
               [(lhs rhs)
                (rebuild c (list #'lhs (annotate-expr #'rhs (and (= 1 (length ids)) (car ids)))))]))))

;; A `cond` / `if` question the user wrote (not a test generated by a macro).
(define (question? t)
  (let ([text (user-text t)])
    (and text
         (not (literal? t))
         (not (regexp-match? #rx"^[[]" text))
         (not (regexp-match? #rx"^[(](cond|if|and|or|when|unless|define|define-struct|local|let|let[*]|letrec)[ \n\t\r]" text)))))

;; Where an `if` test came from: the user's question itself, or the question inside the
;; teaching languages' `(verify-boolean question 'cond)` wrapper.
(define (question-site t)
  (cond
    [(question? t) t]
    [else
     (syntax-case t ()
       [(app f inner . _)
        (and (identifier? #'f) (eq? (syntax-e #'f) 'verify-boolean) (question? #'inner))
        #'inner]
       [_ #f])]))

(define (definition-text? text)
  (regexp-match? #rx"^[(](define|define-struct|define-values|define-syntax|define-syntax-rule|struct|define-record-procedures|define-datatype|check-expect|check-within|check-error|check-random|check-satisfied|check-member-of|check-range)[] \n\t\r)]" text))

(define (literal? stx)
  (kernel-syntax-case stx #f
    [(quote _) #t]
    [_ #f]))

(define (collect-struct-fields! orig)
  (define (field-name f)
    (syntax-case f ()
      [(id . _) (identifier? #'id) (symbol->string (syntax-e #'id))]
      [id (identifier? #'id) (symbol->string (syntax-e #'id))]
      [_ #f]))
  (define (scan form)
    (syntax-case form ()
      [(head name (field ...) . _)
       (and (identifier? #'head) (memq (syntax-e #'head) '(define-struct struct define-record)))
       (let ([n (syntax-case #'name () [(n super) #'n] [n #'n])])
         (when (identifier? n)
           (hash-set! struct-fields (symbol->string (syntax-e n))
                      (filter values (map field-name (syntax->list #'(field ...)))))))]
      [(head . rest)
       (and (identifier? #'head) (memq (syntax-e #'head) '(module #%module-begin begin)))
       (for-each scan (or (syntax->list #'rest) '()))]
      [(a ...) (for-each scan (syntax->list #'(a ...)))]
      [_ (void)]))
  (scan orig))

;; ---------------------------------------------------------------------------
;; Recording

(struct frame (name args [locals #:mutable] pos span [replaced? #:mutable #:auto]) #:auto-value #f)
;; The most recent user-written application about to run: (list pos span tail?).
(define current-site #f)
(define (viz-site! pos span tail?) (set! current-site (list pos span tail?)))

;; The user's functions (closures of instrumented lambdas); everything else applied at a user
;; call site is a primitive operation.
(define user-procs (make-weak-hasheq))
(define (viz-register! f) (hash-set! user-procs f #t) f)
(define (viz-primitive? f)
  (and recording? (procedure? f) (not (hash-ref user-procs f #f))))

;; Primitive operations since the last step: (list pos span args result), newest first.
(define pending-evals '())
(define (viz-compute pos span f args)
  (call-with-values
   (λ () (apply f args))
   (λ results
     (when (and recording? (= 1 (length results)))
       (set! pending-evals (take (cons (list pos span args (car results)) pending-evals)
                                 (min 12 (add1 (length pending-evals))))))
     (apply values results))))

;; Program output since the last step.
(define pending-output (open-output-string))
(define (viz-note-output! str)
  (when recording? (write-string str pending-output)))
(define stack '()) ; newest first
(define globals '()) ; newest first: (name . getter)
(define recording? #f)
(define step-count 0)
(define truncated? #f)

(define (viz-start!)
  (set! stack '())
  (set! globals '())
  (set! step-count 0)
  (set! truncated? #f)
  (set! pending-evals '())
  (set! pending-output (open-output-string))
  (set! recording? #t)
  (emitter (hasheq 'ev "viz-start" 'max_steps max-steps)))

(define (viz-finish! failed?)
  (when recording?
    (record! "end" #f #f #f (hasheq 'failed failed?))
    (set! recording? #f)
    (emitter (hasheq 'ev "viz-end" 'steps step-count 'truncated truncated?))))

(define (viz-call name args pos span thunk)
  (cond
    [(not recording?) (thunk)]
    [else
     (define site (or current-site (list pos span #f)))
     (set! current-site #f)
     (define f (frame name args '() (car site) (cadr site)))
     ;; A tail call replaces the caller's frame (it returns whatever this call returns).
     (when (and (caddr site) (pair? stack))
       (set-frame-replaced?! (car stack) #t))
     (set! stack (cons f stack))
     (record! "call" (frame-pos f) (frame-span f) name #f)
     (apply values
            (dynamic-wind
             void
             (λ ()
               (define vals (call-with-values thunk list))
               (unless (frame-replaced? f)
                 (record! "return" (frame-pos f) (frame-span f) name (if (= 1 (length vals)) (hasheq 'value (car vals)) #f)))
               vals)
             (λ () (set! stack (remq f stack)))))]))

(define (viz-test v pos span)
  (when recording?
    (record! "question" pos span #f (hasheq 'value v)))
  v)

(define (viz-define! gets pos span)
  (when recording?
    (set! globals (append (reverse gets) globals))
    (record! "define" pos span (string-join (map car gets) ", ") #f)))

(define (viz-bind! gets)
  (when recording?
    (if (null? stack)
        (set! globals (append (reverse gets) globals))
        (set-frame-locals! (car stack) (append (frame-locals (car stack)) gets)))))

;; A top-level expression's value, as the program prints it.
(define (viz-note-result! v)
  (when recording?
    (record! "result" #f #f #f (hasheq 'value v))))

(define (viz-set! name get pos span)
  (when recording?
    (record! "set" pos span name (hasheq 'value (get)))))

;; ---------------------------------------------------------------------------
;; Snapshots

(define object-ids (make-weak-hasheq))
(define next-id 1)
(define sent (make-hash)) ; id -> content jsexpr last sent
(define object-budget 600)

(define (object-id v)
  (or (hash-ref object-ids v #f)
      (let ([id next-id])
        (set! next-id (add1 next-id))
        (hash-set! object-ids v id)
        id)))

(define (record! kind pos span name extra)
  (cond
    [(>= step-count max-steps)
     (unless truncated?
       (set! truncated? #t))]
    [else
     (define new-objects '())
     (define seen (make-hasheq))
     (define budget object-budget)
     (define (ref v)
       (cond
         [(primitive v) => values]
         [(hash-ref seen v #f) (hasheq 'ref (object-id v))]
         [(<= budget 0) (hasheq 'v "…" 'k "more")]
         [else
          (set! budget (sub1 budget))
          (hash-set! seen v #t)
          (define id (object-id v))
          (define content (describe v ref))
          (unless (equal? (hash-ref sent id #f) content)
            (hash-set! sent id content)
            (set! new-objects (cons (hash-set content 'id id) new-objects)))
          (hasheq 'ref id)]))
     (define (vars gets)
       (for/list ([g (in-list gets)])
         (list (car g) (ref (with-handlers ([(λ (e) #t) (λ (e) (void))]) ((cdr g)))))))
     (define globals-json (vars (reverse globals)))
     (define frames-json
       ;; Frames replaced by a tail call are sent too (marked), so the IDE can show where the
       ;; running call came from without pretending the stack grew.
       (for/list ([f (in-list (reverse stack))])
         (hasheq 'name (frame-name f)
                 'replaced (frame-replaced? f)
                 'nargs (length (frame-args f))
                 'pos (or (frame-pos f) (json-null))
                 'span (or (frame-span f) 0)
                 'vars (vars (append (frame-args f) (frame-locals f))))))
     (define evals
       (for/list ([e (in-list (reverse pending-evals))])
         (hasheq 'pos (or (car e) (json-null)) 'span (cadr e)
                 'args (map ref (caddr e)) 'value (ref (cadddr e)))))
     (set! pending-evals '())
     (define output (get-output-string pending-output))
     (set! pending-output (open-output-string))
     (define msg
       (for/fold ([h (hasheq 'ev "viz-step"
                             'index step-count
                             'kind kind
                             'pos (or pos (json-null))
                             'span (or span 0)
                             'name (or name (json-null))
                             'globals globals-json
                             'frames frames-json
                             'evals evals
                             'output output)])
                 ([(k v) (in-hash (or extra (hasheq)))])
         (hash-set h k (if (eq? k 'value) (ref v) v))))
     (set! step-count (add1 step-count))
     (emitter (hash-set msg 'objects (reverse new-objects)))]))

;; A primitive's description, or #f for heap objects.
(define (primitive v)
  (cond
    [(and (number? v) (exact? v) (integer? v)) (hasheq 'v (number->string v) 'k "num")]
    [(and (number? v) (exact? v) (real? v)) (hasheq 'v (exact->display v) 'k "num")]
    [(number? v) (hasheq 'v (format "#i~a" v) 'k "num")]
    [(string? v) (hasheq 'v (format "~s" v) 'k "str")]
    [(symbol? v) (hasheq 'v (format "'~a" v) 'k "sym")]
    [(boolean? v) (hasheq 'v (if v "#true" "#false") 'k "bool")]
    [(char? v) (hasheq 'v (format "~s" v) 'k "char")]
    [(null? v) (hasheq 'v "'()" 'k "empty")]
    [(void? v) (hasheq 'v "(void)" 'k "void")]
    [(procedure? v) (hasheq 'v (format "~a" (or (object-name v) "λ")) 'k "fn")]
    [(keyword? v) (hasheq 'v (format "~s" v) 'k "sym")]
    [(or (pair? v) (mpair? v) (vector? v) (box? v) (hash? v) (struct-object? v) (image-encoder v)) #f]
    [else (hasheq 'v (truncate-text (format "~v" v)) 'k "other")]))

(define (exact->display q)
  ;; Teaching languages show terminating fractions as decimals.
  (define d (denominator q))
  (let loop ([n d])
    (cond
      [(= n 1) (number->string (exact->inexact q))]
      [(zero? (remainder n 2)) (loop (quotient n 2))]
      [(zero? (remainder n 5)) (loop (quotient n 5))]
      [else (number->string q)])))

(define (truncate-text s)
  (if (> (string-length s) 60) (string-append (substring s 0 57) "…") s))

(define (struct-object? v)
  (and (struct? v)
       (let ([vec (struct->vector v)])
         (not (and (= (vector-length vec) 2) (eq? (vector-ref vec 1) '...))))))

(define (describe v ref)
  (cond
    [(image-encoder v) => (λ (img) (hasheq 'k "image" 'png (hash-ref img 'png) 'width (hash-ref img 'width) 'height (hash-ref img 'height)))]
    [(pair? v) (hasheq 'k "cons" 'first (ref (car v)) 'rest (ref (cdr v)))]
    ;; R5RS lists are mutable pairs.
    [(mpair? v) (hasheq 'k "cons" 'first (ref (mcar v)) 'rest (ref (mcdr v)))]
    [(vector? v) (hasheq 'k "vector" 'items (for/list ([x (in-vector v)] [_ (in-range 60)]) (ref x)) 'len (vector-length v))]
    [(box? v) (hasheq 'k "box" 'items (list (ref (unbox v))))]
    [(hash? v)
     (hasheq 'k "hash"
             'pairs (for/list ([(k x) (in-hash v)] [_ (in-range 60)]) (list (ref k) (ref x))))]
    [(struct-object? v)
     (define vec (struct->vector v))
     (define name (regexp-replace #rx"^struct:" (symbol->string (vector-ref vec 0)) ""))
     (define names (hash-ref struct-fields name #f))
     ;; Teaching-language structs carry hidden slots after the declared fields.
     (define all (cdr (vector->list vec)))
     (define vals (if (and names (< (length names) (length all))) (take all (length names)) all))
     (hasheq 'k "struct"
             'name name
             'fields (for/list ([x (in-list vals)] [k (in-naturals)])
                       (list (if (and names (< k (length names))) (list-ref names k) (format "field~a" k))
                             (ref x))))]
    [else (hasheq 'k "other" 'text (truncate-text (format "~v" v)))]))
