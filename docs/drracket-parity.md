# DrRacket parity

How Dr. Rocket compares with DrRacket 9.3, checked by running both against the same files
(October 2026).

## Covered

- **Running programs:** Definitions + Interactions, Run / Break / Kill, stdin, images in
  output, big-bang animations and racket/gui windows, memory limit, Create Executable.
- **Languages:** every language level (How to Design Programs, DeinProgramm, R5RS, Pretty
  Big, `#lang`), with output style, fraction style, sharing, tracing and teachpacks.
- **Stepper:** step for step identical to DrRacket's (including cons chains shown as lists
  and the closing "finished" step), with images drawn in the steps. Like DrRacket, Advanced
  Student, the advanced DeinProgramm level, R5RS and Pretty Big have no stepper.
- **Check syntax:** arrows, unused bindings and requires, tooltips, rename, jump to
  definition across files, semantic colouring.
- **Debugger:** breakpoints, step into/over/out, locals, call stack, and required modules
  that are open in other tabs.
- **Tests, test coverage, profiler, macro stepper, module browser, docs and blue boxes.**
- **DrRacket's editor format (WXME):** files with pasted images open with the images shown
  inline, run with real images, and save back in DrRacket's format. Comment boxes open as
  `#| … |#` comments and fractions as their value. Insert Image…, pasting images from the
  clipboard, and copying images between files all work.
- **File and Racket menus:** Revert to Saved, Save Interactions As…, Print… (coloured
  HTML through the browser's print dialog), Comment Out with a Box / with `#;`, Show Log
  (the program's logged warnings and errors), Install / Update / Remove Package and Show
  Installed Packages.

Beyond DrRacket: the Visualizer (an animated replay of a run, with trees and lists drawn as
diagrams), project tabs, integrated terminals, Zed themes, and a modern editor.

## Not covered (deliberately, for now)

- **Rare DrRacket boxes:** XML boxes, Racket and splice boxes, test-case boxes, pict boxes
  and Insert Large Letters. A file containing them opens **read-only** with a message, so it
  is never damaged; edit those files in DrRacket.
- **Language-provided colouring and indentation** for non-Racket `#lang`s (DrRacket asks the
  language for its lexer and indenter via `get-info`). Dr. Rocket uses its Racket lexer for
  every file, so `#lang scribble/manual` prose is coloured as code.
- **Custom key bindings** and **spell checking**.
- **DrRacket plugins (tools)**, including course-specific handin-server Submit buttons.
- **Log levels below warning:** Racket's own libraries log constantly at `info`, so only
  warnings and errors are shown.

## Platforms and distribution

- macOS: `tools/bundle.sh` builds the app; `--universal` for Apple Silicon and Intel,
  `--dmg` for a disk image. Distribution signing and notarization run automatically when
  `DEVELOPER_ID` and `NOTARY_PROFILE` are set (an Apple Developer account is needed).
- Windows and Linux: the code builds there (checked in CI, which runs the full test suite
  with Racket on macOS, Windows and Linux), but day-to-day use has been on macOS.
