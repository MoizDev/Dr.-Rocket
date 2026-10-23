// Every file the demos show, as it appears in the screenshots. Demo data, not real coursework.
import { IMAGE_CHAR } from "../racket-lexer";

export const GRADES = `;; average : (listof Number) -> Number
;; the mean of a non-empty list of marks
(define (average marks)
  (/ (total marks) (length marks)))

;; total : (listof Number) -> Number
(define (total marks)
  (cond [(empty? marks) 0]
        [else (+ (first marks) (total (rest marks)))]))

(check-expect (total (list 80 90 70)) 240)
(check-expect (average (list 80 90 70)) 80)

;; letter : Number -> String
;; the letter grade for a mark out of 100
(define (letter mark)
  (cond [(>= mark 80) "A"]
        [(>= mark 70) "B"]
        [(>= mark 60) "C"]
        [else "F"]))

(check-expect (letter 92) "A")
(check-expect (letter 64) "C")

(define marks (list 92 78 85 64))
(average marks)
(map letter marks)
`;

export const SQUARES = `;; sum-squares : (listof Number) -> Number
;; adds up the square of every number in the list
(define (sum-squares lon)
  (cond [(empty? lon) 0]
        [else (+ (sqr (first lon))
                 (sum-squares (rest lon)))]))

(check-expect (sum-squares (cons 1 (cons 2 empty))) 5)

(sum-squares (cons 3 (cons 4 empty)))
`;

export const POSTER = `(require 2htdp/image)

;; a picture pasted straight into DrRacket
(define rocket ${IMAGE_CHAR})

;; poster : String -> Image
(define (poster title)
  (above rocket
         (overlay (text title 16 "white")
                  (rectangle 140 34 "solid" "navy"))))

(poster "Launch day")
(beside rocket rocket rocket)
`;

export const BST = `(define-struct node (key left right))
;; A BST is one of:
;; - empty
;; - (make-node Number BST BST)

;; insert : Number BST -> BST
(define (insert k t)
  (cond [(empty? t) (make-node k empty empty)]
        [(< k (node-key t))
         (make-node (node-key t)
                    (insert k (node-left t))
                    (node-right t))]
        [else
         (make-node (node-key t)
                    (node-left t)
                    (insert k (node-right t)))]))

;; tree-sum : BST -> Number
(define (tree-sum t)
  (cond [(empty? t) 0]
        [else (+ (node-key t)
                 (tree-sum (node-left t))
                 (tree-sum (node-right t)))]))

(define tree (foldl insert empty (list 5 3 8 1 4 9)))
(tree-sum tree)
`;

export const SCRATCH = `#lang racket

;; Count the words in a string, most frequent first.
(define (word-counts text)
  (define words (string-split (string-downcase text)))
  (define counts
    (for/fold ([h (hash)]) ([w (in-list words)])
      (hash-update h w add1 0)))
  (sort (hash->list counts) > #:key cdr))

(word-counts "the cat and the hat")
`;

export const QUEUE = `#lang racket
;; A queue as two lists: items enter at the back and leave from the front.
(struct queue (front back) #:transparent)

(define empty-queue (queue '() '()))

(define (queue-push q x)
  (queue (queue-front q) (cons x (queue-back q))))

(define (queue-pop q)
  (match q
    [(queue '() '()) (error 'queue-pop "empty queue")]
    [(queue '() back) (queue-pop (queue (reverse back) '()))]
    [(queue (cons _ more) back) (queue more back)]))

(define q (queue-push (queue-push empty-queue 1) 2))
`;

export const DEGREE = `#lang racket
;; Degree of every vertex in an undirected graph, given as an edge list.
(define edges '((a b) (a c) (b c) (c d)))

(define (degrees es)
  (for/fold ([h (hash)]) ([e (in-list es)])
    (for/fold ([h h]) ([v (in-list e)])
      (hash-update h v add1 0))))

(sort (hash->list (degrees edges)) symbol<? #:key car)
`;

export const README = `# CS135

Assignments for CS135. Each folder holds one assignment;
run the tests with raco test.
`;

export const A05_STUB = `;; a05 : coming soon
;; (demo placeholder: this assignment isn't part of the demo)
`;

export const DEMO_STUB = (name: string) => `#lang racket
;; ${name}: part of this demo's project tree only.
`;
