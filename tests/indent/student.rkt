;; ***************************************************
;;    Student Name (12345678)
;;    CS 145 Fall 2026
;;    Assignment 05, Problem 1
;; ***************************************************
#lang racket

(require racket/list
         racket/string)
(provide count-leaves
         tree-sum)

;; (count-leaves t) produces the number of leaves in t.
;; count-leaves: Tree -> Nat
;; Examples:
(check-expect (count-leaves empty) 0)
(check-expect (count-leaves (list 1 (list 2 3)))
              3)

(define (count-leaves t)
  (cond
    [(empty? t) 0]
    [(cons? (first t))
     (+ (count-leaves (first t))
        (count-leaves (rest t)))]
    [else (add1 (count-leaves (rest t)))]))

(define-struct node (key left right))

(define (tree-sum t)
  (cond [(empty? t) 0]
        [else
         (+ (node-key t)
            (tree-sum (node-left t))
            (tree-sum (node-right t)))]))

(define (insert-all lst acc)
  (local [(define (helper x so-far)
            (cond [(empty? so-far) (list x)]
                  [(< x (first so-far)) (cons x so-far)]
                  [else (cons (first so-far)
                              (helper x (rest so-far)))]))]
    (foldr helper acc lst)))

(define (make-adder n)
  (lambda (x)
    (+ x n)))

(define (classify n)
  (if (even? n)
      'even
      'odd))

(define (sum-to n)
  (let loop ([i 0]
             [acc 0])
    (if (> i n)
        acc
        (loop (add1 i) (+ acc i)))))

(define (distance p q)
  (let* ([dx (- (posn-x p) (posn-x q))]
         [dy (- (posn-y p) (posn-y q))])
    (sqrt (+ (* dx dx)
             (* dy dy)))))

(define table
  '((apple 1)
    (banana 2)
    (cherry 3)))

(define long-string "first line
second line keeps its indentation
  third line too")

(when (positive? 1)
  (display "positive")
  (newline))

(check-expect
 (map (lambda (x) (* x x))
      (list 1 2 3))
 (list 1 4 9))
