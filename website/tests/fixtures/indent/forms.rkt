#lang racket/base
;; Forms from DrRacket's indentation table and its special cases.

(provide
 foo
 (contract-out
  [bar (-> number? number?)]))

(define-values (a b)
  (values 1
          2))

(define (f #:key [key 1]
           #:other other)
  (list key
        other))

(f #:key 1
   #:other 2)

(for/fold ([acc 0])
          ([x (in-list '(1 2 3))])
  (+ acc x))

(for/fold ([acc 0]
           [n 0])
          ([x (in-list '(1 2 3))])
  (values (+ acc x) (add1 n)))

(for/list ([i 3]
           #:when (odd? i))
  (* i i))

(for ([i 3])
  (displayln i))

(with-handlers ([exn:fail? (lambda (e) #f)])
  (error 'oops))

(call-with-values
 (lambda () (values 1 2))
 list)

(dynamic-wind
 void
 (lambda () 1)
 void)

(parameterize ([current-output-port (current-error-port)])
  (displayln "hi"))

(match '(1 2)
  [(list x y) (+ x y)]
  [_ 0])

(case 3
  [(1 2) 'small]
  [else 'big])

(begin
  (displayln 1)
  (displayln 2))

(begin (displayln 1)
       (displayln 2))

(lambda
    (x)
  x)

(define-syntax-rule (swap! a b)
  (let ([tmp a])
    (set! a b)
    (set! b tmp)))

(syntax-rules ()
  [(_ x ...)
   (list x ...)])

(define (g x ...
           y)
  x)

(list 1 2
 ---
 3)

`(a ,(+ 1 2)
    ,@(list 3 4))

#(1 2
    3)

(#%app list
       1)

((compose add1 add1)
 1)

(hash 'a 1
      'b 2)

(struct point (x y)
  #:transparent)

(module+ test
  (require rackunit)
  (check-equal? 1 1))

(unless #f
  (void))

(let-values ([(q r) (quotient/remainder 7 2)])
  (list q r))

(cond [#t
       1]
      [else
       2])
