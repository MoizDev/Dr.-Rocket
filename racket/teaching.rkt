#lang racket/base
;; The teaching languages' levels and their Choose Language output settings, shared by the
;; runner and the stepper.

(provide (struct-out level-info)
         (struct-out sdp-level-info)
         levels
         lang-line-level
         htdp-runtime-settings-args
         sdp-runtime-settings-args)

;; Per level: the language module, reader and printing configuration, and how the stepper
;; shows lambdas. `family` is 'htdp or a DeinProgramm level symbol.
(struct level-info (module accept-quasiquote? abbreviate? function-output? output-function?
                     lambdas? sharing? [family #:auto])
  #:auto-value 'htdp)
(struct sdp-level-info level-info (sdp-level writing-style))

(define levels
  (hash "beginner"            (level-info '(lib "htdp-beginner.ss" "lang") #f #f #f #t #f #f)
        "beginner-abbr"       (level-info '(lib "htdp-beginner-abbr.ss" "lang") #t #t #f #t #f #f)
        "intermediate"        (level-info '(lib "htdp-intermediate.ss" "lang") #t #t #t #t #f #f)
        "intermediate-lambda" (level-info '(lib "htdp-intermediate-lambda.ss" "lang") #t #t #t #f #t #f)
        "advanced"            (level-info '(lib "htdp-advanced.ss" "lang") #t #t #t #f #t #t)
        "sdp-beginner"        (sdp-level-info '(lib "deinprogramm/sdp/beginner.rkt") #t #t #f #f #f #f 'beginner "explicit")
        "sdp-vanilla"         (sdp-level-info '(lib "deinprogramm/sdp/vanilla.rkt") #t #t #f #f #f #f 'vanilla "explicit")))

;; `#lang htdp/bsl` and friends map onto the same levels.
(define lang-line-levels
  '(("htdp/bsl" . "beginner") ("htdp/bsl+" . "beginner-abbr") ("htdp/isl" . "intermediate")
    ("htdp/isl+" . "intermediate-lambda") ("htdp/asl" . "advanced")))

(define (lang-line-level source)
  (define m (regexp-match #px"^\\s*(?:;[^\n]*\n\\s*)*#lang\\s+(\\S+)" source))
  (and m (let ([p (assoc (cadr m) lang-line-levels)]) (and p (cdr p)))))

(define (setting settings key default)
  (define v (hash-ref settings key default))
  (if (eq? v 'null) default v))

;; The fields of htdp/bsl/runtime's `sl-runtime-settings` for a level and the Choose
;; Language settings (`TeachingSettings::stepper_json`).
(define (htdp-runtime-settings-args info settings)
  (list (string->symbol (setting settings 'printing_style "constructor"))
        (string->symbol (setting settings 'fraction_style "repeating-decimal"))
        (setting settings 'show_sharing (level-info-sharing? info))
        (setting settings 'insert_newlines #t)
        #f ; tracing: the runner traces calls itself
        (setting settings 'true_false_empty_as_ids #f)
        (level-info-abbreviate? info)
        (level-info-function-output? info)
        (level-info-output-function? info)))

;; The fields of DeinProgramm's `sdp-runtime-settings`.
(define (sdp-runtime-settings-args settings default-writing-style)
  (list (string->symbol (setting settings 'printing_style "write"))
        (string->symbol (setting settings 'writing_style default-writing-style))
        (string->symbol (setting settings 'fraction_style "repeating-decimal"))
        (setting settings 'show_sharing #f)
        (setting settings 'insert_newlines #t)
        #f))
