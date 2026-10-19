#lang racket/base
;; Reindent files in place with DrRacket's own indenter (the framework's racket:text%).
;; Regenerates the expected output for the indentation tests:
;;   racket tools/drracket-indent.rkt tests/indent/*.rkt
(require racket/class racket/gui/base framework racket/file)
(for ([path (current-command-line-arguments)])
  (define t (new racket:text%))
  (send t insert (file->string path))
  (send t tabify-all)
  (display-to-file (send t get-text) path #:exists 'replace))
