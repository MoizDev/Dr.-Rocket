# DrRacket parity: missing features

What Dr. Rocket still lacks compared with DrRacket 9.3, found by running both against the
same files (October 2026). Ordered by priority.

## 1. DrRacket editor format (WXME) - high priority

DrRacket saves files that contain images, comment boxes or other boxes in its binary
"GRacket editor format" (the file starts with `#reader(lib"read.ss""wxme")WXME0111`).
Pasting images into code is standard in HtDP courses, so students hit this constantly.

Today in Dr. Rocket:
- The file opens as raw WXME data instead of code with inline images.
- Running it fails with `read-syntax: WXME format problem while reading for image-snip image content`
  (the file is read as UTF-8 text, which corrupts the binary image data).
- Saving it would destroy the images.

Needed:
- Read WXME files byte-exact (decode with Racket's `wxme` library in the server, or a Rust
  decoder) into text plus embedded objects (image snips, comment boxes, number snips, etc.).
- Show images inline in the editor as atomic objects; comment boxes as boxed regions.
- Run them by sending the original bytes (or re-encoded WXME) so Racket's reader sees real
  snips.
- Save back to WXME whenever the buffer contains a non-text object; plain text otherwise
  (same rule as DrRacket).
- Paste images from the clipboard into the editor.

Repro: `racket` script that builds a `text%` with an `image-snip%` and calls
`(send t save-file path 'standard)`, then open the file in Dr. Rocket.

## 2. Insert menu - depends on 1

Dr. Rocket only has Insert λ. DrRacket also has:
- Insert Image... (from a file)
- Insert Comment Box
- Insert Fraction... and Insert Large Letters...
- Insert XML Box, Racket Box, Racket Splice Box
- Insert Pict Box

## 3. Language-provided colouring and indentation

DrRacket asks each `#lang` (via `get-info` / `color-lexer`, `drracket:indentation`,
`drracket:paren-matches`, etc.) for its own lexer, indenter and bracket rules. Dr. Rocket uses
its Racket lexer for every file, so for example `#lang scribble/manual` prose is coloured as
code (`Isn't` is split at the quote).

Needed: query the module's language info in the server and use the language's lexer for
non-Racket `#lang`s (incrementally, like the current lexer), falling back to the Racket lexer.

## 4. Smaller missing features

- Print (definitions and interactions).
- Revert to saved (File > Revert).
- Log viewer ("Show Log"): a panel showing logger output with level/topic filtering.
- Custom key bindings (DrRacket's "Add User-defined Keybindings").
- Package manager beyond Install Package: update, remove, browse catalogs, show installed
  packages.
- Spell check of strings and comments.
- Save Interactions / Log Definitions and Interactions; Save Other > Save as Text.
- DrRacket plugins (tools), notably the handin-server client's Submit button that many
  courses use. This needs the course-specific plugin and is a separate project.

## 5. UI nits found along the way

- The language button shows the graduation-cap "teaching language" icon for non-teaching
  `#lang`s such as `#lang scribble/manual`.
- `.scrbl` files get a generic file icon in the tree and tab instead of a Racket/Scribble icon.

## Already verified working

Run and REPL (stdin, images in output), all language levels with teachpacks, tracing and
output styles, stepper, check syntax (arrows, rename, cross-file jump to definition), docs and
blue boxes, debugger (including required modules open in other tabs), macro stepper, test
coverage, profiler, module browser, test results, big-bang animations, racket/gui windows,
Create Executable, Install Package.
