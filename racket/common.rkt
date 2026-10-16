#lang racket/base
;; Shared helpers for Racketeer's backend processes: the JSON-lines protocol, source
;; positions, reading program text, and pretty-printing with highlighted subterms.

(require json
         racket/class
         net/base64
         racket/port
         racket/pretty
         racket/string)

(provide protocol-out
         emit
         read-command
         source-offset
         source-line-offset
         main-source
         set-module-offset!
         simplify-path*
         same-source?
         srcloc->jsexpr
         syntax->srcloc-jsexpr
         adjust-pos
         path->jsexpr
         string-of-source
         make-source-name
         read-all-syntax
         truncate-string
         pretty-with-highlights
         exn->message
         rational->decimal-string
         image-char?
         decode-source-images
         open-source-port
         png->image-value
         image-value->png)

;; The real stdout, captured at startup before user code can redirect it.
(define protocol-out (current-output-port))
(define emit-lock (make-semaphore 1))

;; emit : jsexpr -> void
;; Write one protocol message. Thread-safe and atomic with respect to breaks.
(define (emit msg)
  (parameterize-break #f
    (call-with-semaphore
     emit-lock
     (λ ()
       (write-string (jsexpr->string msg) protocol-out)
       (newline protocol-out)
       (flush-output protocol-out)))))

;; read-command : input-port -> (or/c hash? eof)
;; Skips malformed lines instead of dying.
(define (read-command in)
  (let loop ()
    (define line (read-line in 'any))
    (cond
      [(eof-object? line) line]
      [(string=? (string-trim line) "") (loop)]
      [else
       (with-handlers ([exn:fail? (λ (e)
                                    (eprintf "racketeer: bad command ~s: ~a\n" line (exn-message e))
                                    (loop))])
         (define v (string->jsexpr line))
         (if (hash? v) v (loop)))])))

;; ---------------------------------------------------------------------------
;; Positions
;;
;; The IDE may prefix the program with a hidden DrRacket metadata header. `source-offset`
;; is its length in characters; `source-line-offset` its number of lines. Positions and
;; lines reported for the main source are shifted back so they refer to the visible buffer.

(define source-offset (make-parameter 0))
(define source-line-offset (make-parameter 0))
;; The source name used when reading the main program (a path or a symbol).
(define main-source (make-parameter #f))

;; Hidden-header offsets of other files (debugged teaching-language modules):
;; simplified path -> (cons char-offset line-offset).
(define module-offsets (make-hash))
(define (set-module-offset! path chars lines)
  (hash-set! module-offsets (simplify-path* path) (cons chars lines)))
(define (module-offset src)
  (and (path? src) (hash-ref module-offsets (simplify-path* src) #f)))

(define (same-source? a b)
  (cond
    [(and (path? a) (path? b))
     (or (equal? a b)
         (equal? (simplify-path* a) (simplify-path* b)))]
    [else (equal? a b)]))

(define (simplify-path* p)
  (with-handlers ([exn:fail? (λ (e) p)])
    (normal-case-path (simplify-path (path->complete-path p)))))

(define (path->jsexpr src)
  (cond
    [(path? src) (path->string src)]
    [(string? src) src]
    [else (json-null)]))

;; adjust-pos : source (or/c #f exact-positive-integer?) -> (or/c 'null natural)
;; Convert Racket's 1-based position into a 0-based visible-buffer position.
(define (adjust-pos src pos)
  (cond
    [(not pos) (json-null)]
    [(same-source? src (main-source)) (max 0 (- pos 1 (source-offset)))]
    [(module-offset src) => (λ (o) (max 0 (- pos 1 (car o))))]
    [else (- pos 1)]))

(define (adjust-line src line)
  (cond
    [(not line) (json-null)]
    [(same-source? src (main-source)) (max 1 (- line (source-line-offset)))]
    [(module-offset src) => (λ (o) (max 1 (- line (cdr o))))]
    [else line]))

(define (srcloc-source->jsexpr src)
  (if (same-source? src (main-source))
      (path->jsexpr (and (path? src) src))
      (path->jsexpr src)))

(define (make-srcloc-jsexpr src line col pos span)
  (hasheq 'path (srcloc-source->jsexpr src)
          'line (adjust-line src line)
          'col (or col (json-null))
          'pos (adjust-pos src pos)
          'span (or span (json-null))))

(define (srcloc->jsexpr sl)
  (make-srcloc-jsexpr (srcloc-source sl) (srcloc-line sl) (srcloc-column sl)
                      (srcloc-position sl) (srcloc-span sl)))

(define (syntax->srcloc-jsexpr stx)
  (make-srcloc-jsexpr (syntax-source stx) (syntax-line stx) (syntax-column stx)
                      (syntax-position stx) (syntax-span stx)))

;; ---------------------------------------------------------------------------
;; Reading programs

(define (string-of-source v)
  (if (string? v) v ""))

;; make-source-name : (or/c string? 'null) -> (or/c path? symbol?)
(define (make-source-name path)
  (if (and (string? path) (not (string=? path "")))
      (simplify-path (path->complete-path (string->path path)))
      'unsaved-editor))

;; read-all-syntax : input-port source -> (listof syntax?)
(define (read-all-syntax port source)
  (let loop ([acc '()])
    (define stx (read-syntax source port))
    (if (eof-object? stx)
        (reverse acc)
        (loop (cons stx acc)))))

(define (truncate-string s n)
  (if (> (string-length s) n)
      (string-append (substring s 0 (max 0 (- n 1))) "…")
      s))

(define (exn->message e)
  (if (exn? e) (exn-message e) (format "~a" e)))

;; ---------------------------------------------------------------------------
;; Pretty printing with highlights
;;
;; pretty-with-highlights : any (hash/eq) natural [#:size-hook #:print-hook] -> (values string (listof (list s e)))
;; `datum` is printed with `pretty-write`. Keys of `highlights` are subterms of `datum`
;; (compared with eq?) whose printed extents are recorded. Atomic subterms that should be
;; highlighted must be replaced by unique placeholders (see the stepper) whose table
;; value is (list original-atom); they are printed as the original atom.
;; Ranges are 0-based character offsets into the returned string.

(define (pretty-with-highlights datum highlights width
                                #:size-hook [size-hook (pretty-print-size-hook)]
                                #:print-hook [print-hook (pretty-print-print-hook)]
                                #:write? [write? #t])
  (define out (open-output-string))
  (define port (make-counting-port out))
  (define starts '())
  (define ranges '())
  (define (pos) (let-values ([(l c p) (port-next-location port)]) (sub1 p)))
  (define (placeholder v) (let ([x (hash-ref highlights v #f)]) (and (pair? x) x)))
  (parameterize ([pretty-print-columns (max 20 width)]
                 [pretty-print-size-hook
                  (λ (v display? p)
                    (cond
                      [(placeholder v)
                       => (λ (x)
                            (or (size-hook (car x) display? p)
                                (string-length (format "~s" (car x)))))]
                      [else (size-hook v display? p)]))]
                 [pretty-print-print-hook
                  (λ (v display? p)
                    (cond
                      [(placeholder v)
                       => (λ (x)
                            (if (size-hook (car x) display? p)
                                (print-hook (car x) display? p)
                                (write-string (format "~s" (car x)) p)))]
                      [else (print-hook v display? p)]))]
                 [pretty-print-pre-print-hook
                  (λ (v p)
                    (when (hash-ref highlights v #f)
                      (flush-output port)
                      (set! starts (cons (pos) starts))))]
                 [pretty-print-post-print-hook
                  (λ (v p)
                    (when (and (hash-ref highlights v #f) (pair? starts))
                      (flush-output port)
                      (set! ranges (cons (list (car starts) (pos)) ranges))
                      (set! starts (cdr starts))))])
    (if write?
        (pretty-write datum port)
        (pretty-print datum port)))
  (flush-output port)
  (define text (string-trim (get-output-string out) "\n" #:left? #f))
  (values text
          (sort (for/list ([r (in-list ranges)]
                           #:when (< (car r) (cadr r)))
                  (list (car r) (min (cadr r) (string-length text))))
                < #:key car)))

;; A port that forwards characters to `out` and counts lines/positions. It deliberately
;; does not accept specials, so language print hooks fall back to plain text (images print
;; as "#<image>", exact fractions through number markup's text rendering) and every
;; printed character is counted.
(define (make-counting-port out)
  (define p
    (make-output-port
     'pretty
     always-evt
     (λ (bs start end non-block? breakable?)
       (write-bytes bs out start end)
       (- end start))
     void))
  (port-count-lines! p)
  p)

;; rational->decimal-string : exact-rational -> (or/c string? #f)
;; Render an exact non-integer like DrRacket's repeating-decimal number snips, using a
;; combining overline (U+0305) on the repeating digits: 1/3 => "0.3̅", 1/6 => "0.16̅".
;; Returns #f when the period is too long to be useful.
(define (rational->decimal-string q)
  (define a (abs q))
  (define int (floor a))
  (define den (denominator a))
  (let loop ([r (numerator (- a int))] [digits '()] [seen (hasheqv)] [i 0])
    (define (finish nonrep rep)
      (string-append (if (negative? q) "-" "")
                     (number->string int) "."
                     (apply string (map digit nonrep))
                     (apply string-append (map (λ (d) (string (digit d) overline)) rep))))
    (cond
      [(zero? r) (finish (reverse digits) '())]
      [(hash-ref seen r #f)
       => (λ (start)
            (define all (reverse digits))
            (finish (take* all start) (drop* all start)))]
      [(> i 80) #f]
      [else
       (define r10 (* r 10))
       (loop (remainder r10 den) (cons (quotient r10 den) digits) (hash-set seen r i) (add1 i))])))

(define (digit d) (integer->char (+ 48 d)))
(define (take* l n) (if (zero? n) '() (cons (car l) (take* (cdr l) (sub1 n)))))
(define (drop* l n) (if (zero? n) l (drop* (cdr l) (sub1 n))))

(define overline (integer->char #x305))

;; ---------------------------------------------------------------------------
;; Images embedded in source code
;;
;; DrRacket files can contain pasted images (saved in its WXME format). The IDE represents
;; each image in the program text as one character from Supplementary Private Use Area-A
;; (U+F0000 .. U+FFFFD), and sends the images' PNG data alongside as
;; {"<code point>": "<base64 png>"}. Reading the text through `open-source-port` turns each
;; such character into the image itself, as DrRacket's editor ports do.

(define (image-char? c)
  (<= #xF0000 (char->integer c) #xFFFFD))

;; decode-source-images : (or/c hash #f) -> (hash/c char? bytes?)
(define (decode-source-images h)
  (for/hash ([(k v) (in-hash (if (hash? h) h (hash)))]
             #:when (and (string? v) (string->number (format "~a" k))))
    (values (integer->char (string->number (format "~a" k)))
            (base64-decode (string->bytes/latin-1 v)))))

(define image-cache (make-hash)) ; png bytes -> image value
(define (png->image-value png)
  (hash-ref! image-cache png
             (λ ()
               (define read-bitmap (dynamic-require 'racket/draw 'read-bitmap))
               (define image-snip% (dynamic-require 'racket/snip 'image-snip%))
               (make-object image-snip% (read-bitmap (open-input-bytes png) 'png)))))

;; image-value->png : any -> (or/c bytes? #f)
(define (image-value->png v)
  (with-handlers ([(λ (e) #t) (λ (e) #f)])
    (define convertible? (dynamic-require 'file/convertible 'convertible?))
    (define convert (dynamic-require 'file/convertible 'convert))
    (and (convertible? v) (let ([b (convert v 'png-bytes)]) (and (bytes? b) b)))))

;; open-source-port : string (hash/c char? bytes?) -> input-port
(define (open-source-port text images)
  (cond
    [(or (hash-empty? images) (not (for/or ([c (in-string text)]) (image-char? c))))
     (open-input-string text)]
    [else
     (define-values (in out) (make-pipe-with-specials))
     (define start 0)
     (for ([c (in-string text)] [k (in-naturals)])
       (when (image-char? c)
         (write-string text out start k)
         (set! start (add1 k))
         (define png (hash-ref images c #f))
         (if png
             ;; The image itself, as DrRacket's editor ports deliver image snips.
             (write-special (png->image-value png) out)
             (write-char c out))))
     (write-string text out start)
     (close-output-port out)
     in]))
