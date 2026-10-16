#lang racket/base
;; Renders the Dr. Rocket app icon (a λ on a violet-to-blue squircle) at every macOS icon size.
;; Usage: racket tools/make_icon.rkt <out-dir>
(require racket/draw racket/class racket/math)

(define out-dir (vector-ref (current-command-line-arguments) 0))

(define (render size path)
  (define bm (make-bitmap size size #:backing-scale 1))
  (define dc (new bitmap-dc% [bitmap bm]))
  (send dc set-smoothing 'aligned)
  ;; macOS icon grid: the shape occupies ~80% of the canvas, centered.
  (define inset (* size 0.1))
  (define w (- size (* 2 inset)))
  (define radius (* w 0.225))
  (define grad (new linear-gradient%
                    [x0 inset] [y0 inset] [x1 (+ inset w)] [y1 (+ inset w)]
                    [stops (list (list 0 (make-color #xC0 #x6B #xE8))
                                 (list 1 (make-color #x4F #x8F #xF2)))]))
  ;; Soft drop shadow.
  (send dc set-pen (make-pen #:style 'transparent))
  (define unit (/ size 512))
  (for ([k (in-range 12)])
    (define grow (* (- 12 k) 1.6 unit))
    (send dc set-brush (make-brush #:color (make-color 0 0 0 0.018)))
    (send dc draw-rounded-rectangle (- inset grow) (+ inset (* 6 unit) (- grow))
          (+ w (* 2 grow)) (+ w (* 2 grow)) (+ radius grow)))
  (send dc set-brush (new brush% [gradient grad]))
  (send dc draw-rounded-rectangle inset inset w w radius)
  ;; Top highlight.
  (define hl (new linear-gradient% [x0 0] [y0 inset] [x1 0] [y1 (+ inset (* w 0.42))]
                  [stops (list (list 0 (make-color 255 255 255 0.22)) (list 1 (make-color 255 255 255 0.0)))]))
  (send dc set-brush (new brush% [gradient hl]))
  (send dc draw-rounded-rectangle inset inset w (* w 0.55) radius)
  ;; The λ.
  (send dc set-font (make-font #:size (* w 0.64) #:face "Inter" #:weight 'semibold #:size-in-pixels? #t))
  (send dc set-text-foreground (make-color 255 255 255))
  (define-values (tw th _d _a) (send dc get-text-extent "λ"))
  (send dc draw-text "λ" (+ inset (/ (- w tw) 2)) (+ inset (/ (- w th) 2) (* w -0.02)))
  (send bm save-file path 'png))

(for ([s '(16 32 64 128 256 512 1024)])
  (render s (build-path out-dir (format "icon_~a.png" s))))
