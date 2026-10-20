# Dr. Rocket - design notes

Dr. Rocket is a native Racket IDE written in Rust (egui/eframe 0.37, wgpu). It replaces DrRacket:
definitions + interactions, every DrRacket language level, the algebraic stepper, check syntax
(binding arrows, rename, jump to definition), the debugger, macro stepper, profiler, coverage,
test results, docs - inside a modern editor shell (file tree with Material icons, tabs, command
palette, quick open, recent projects, themes from Zed, integrated terminals).

## Process model

All Racket-semantic work happens in Racket processes; the Rust side never guesses semantics.

| Process | Lifetime | Script | Purpose |
|---|---|---|---|
| runner | one per Run (killed on next Run / Kill) | `racket/runner.rkt` | runs the program, then serves the REPL in the module's namespace; errortrace, coverage, profiler, debugger |
| stepper | one per stepper session | `racket/stepper.rkt` | DrRacket's algebraic stepper model (`stepper/private/model`) run headlessly |
| server | one per app, long-lived | `racket/server.rkt` | check syntax, docs/blueboxes, completions, macro stepper, module graph |

The `.rkt` scripts are embedded in the binary (`include_str!`), written to
`<cache>/dr-rocket/racket-<hash>/` on startup and compiled with `raco make` in the background.

The Racket executable is found via settings `racket_path`, `$PATH`, `/opt/homebrew/bin`,
`/usr/local/bin`, `/Applications/Racket*/bin`.

## Wire protocol

Newline-delimited JSON over the child's stdin/stdout. The runner reserves its real stdout for
the protocol: user code's `current-output-port`/`current-error-port` are custom ports that forward
text as `output` events. stderr of the child process is logged (diagnostics only).

**Positions are 0-based character offsets into the source text as sent** (Racket's 1-based
`syntax-position` minus one). For teaching languages the IDE sends the DrRacket metadata header
followed by the buffer text, and passes `"offset": <header chars>` so that the backend subtracts it
and all reported positions are relative to the visible buffer. Positions from other files carry
their `path`; positions without a known file use `"path": null`.

A srcloc object: `{"path": string|null, "line": n|null, "col": n|null, "pos": n|null, "span": n|null}`
(`line` 1-based, `col` 0-based, `pos` 0-based char offset).

### runner

IDE -> runner:
```
{"cmd":"run","path":"/abs/f.rkt"|null,"source":"...","offset":0,
 "language":{"kind":"module"} | {"kind":"toplevel","init":"r5rs"|"lang/plt-pretty-big"|...},
 "options":{"instrument":"none"|"errortrace"|"coverage"|"profile"|"debug",
            "submodules":["main","test"], "memory_limit_mb":256|null,
            "breakpoints":[pos,...], "print_width":80, "trace":false,
            "debug_modules":[{"path":"/abs/helper.rkt","source":"...","offset":0,
                              "breakpoints":[pos,...]}]}}
; debug_modules: other open files. When the program requires one, the runner loads it from
; the editor text with debugger annotations (DrRacket debugs required modules open in tabs).
{"cmd":"eval","id":7,"source":"(+ 1 2)"}
{"cmd":"input","text":"hello\n"}          {"cmd":"input-eof"}
{"cmd":"break"}
{"cmd":"debug","action":"continue"|"step-into"|"step-over"|"step-out"|"pause"}
{"cmd":"breakpoints","positions":[pos,...],"path":"/abs/helper.rkt"|absent}
{"cmd":"complete","id":3,"prefix":"str"}
```
runner -> IDE:
```
{"ev":"started","racket_version":"9.3","language":"..."}
{"ev":"output","stream":"out"|"err","text":"...","image":{"png":..,"width":w,"height":h}|absent}
                                 ; output is coalesced (~25 ms / 64 KB batches)
{"ev":"value","text":"...","segments":[{"text":"..."}|{"png":"<base64>","width":w,"height":h}]}
{"ev":"error","message":"...","kind":"read"|"syntax"|"runtime"|"break",
 "srclocs":[srcloc...],"stack":[{"name":"f"|null, ...srcloc}]}
{"ev":"tests","passed":n,"total":n,"text":"Ran 3 tests. 2 passed.",
 "failures":[{"message":"...","srcloc":srcloc}]}
{"ev":"coverage","covered":[[pos,span]...],"uncovered":[[pos,span]...]}
{"ev":"profile","total_ms":n,"nodes":[{"name":..,"srcloc":..,"self_ms":..,"total_ms":..,
 "callers":[idx...],"callees":[idx...]}]}
{"ev":"paused","srcloc":srcloc,"locals":[{"name":"x","value":"3"}],
 "stack":[{"name":..,"srcloc":..}],"result":"..."|null}
{"ev":"resumed"}
{"ev":"input-wanted"}
{"ev":"completions","id":3,"items":["string-append",...]}
{"ev":"ready","id":7|null}     ; definitions finished (id null) or an eval finished
{"ev":"exit","code":0}          ; the program called `exit` (or ran out of memory: code 1)
```
Teaching-language results print exactly like DrRacket (constructor style, `#true`, repeating
decimals as `0.3̅` using U+0305). Teaching-language errors carry DrRacket's rewritten messages
("f: expects only 1 argument, but found 2"); messages never carry a source-location prefix.
Stack frames from Racketeer itself and from installed libraries are omitted. Coverage gives a
character the coverage of its innermost instrumented expression (forms such as `define` count as
covered when anything inside them ran).

### stepper

```
IDE -> {"cmd":"step","path":..,"source":"<buffer text without header>","level":"beginner"|
        "beginner-abbr"|"intermediate"|"intermediate-lambda"|"advanced"|"sdp-beginner"|
        "sdp-vanilla"|"module",          ; "module": a `#lang htdp/bsl|bsl+|isl|isl+` buffer
        "settings":{"printing_style","fraction_style","show_sharing","insert_newlines",
                    "case_sensitive","true_false_empty_as_ids","writing_style","include_tests",
                    "functional_focus"},
        "teachpacks":["(lib \"image.rkt\" \"teachpack\" \"2htdp\")",...],"width":72}
IDE -> {"cmd":"more"}                       ; after a "paused" event
<- {"ev":"step","index":i,"kind":"normal"|"user-application"|...,
    "before":[{"text":"...","highlights":[[s,e],...]}],
    "after":[{"text":"...","highlights":[[s,e],...]}],
    "before_src":{"pos":p,"span":s}|null,"after_src":...|null}
<- {"ev":"step-error","index":i,"before":[...],"message":"...","before_src":...}
<- {"ev":"error","message":"..."}
<- {"ev":"paused","count":n}                ; every 500 steps, waits for "more"
<- {"ev":"finished","count":n}
```
Highlight ranges are 0-based char offsets into that expression's `text`.
`include_tests` defaults to true; false disables test execution for this stepper session.
The option is saved in Settings → Racket and takes effect when stepping starts/restarts.
`functional_focus` defaults to false. When enabled, only the active highlighted subexpressions
are rendered, without the accumulated surrounding recursive expression. Every reduction is
still evaluated and recorded; source locations and image highlights remain attached to it.
The UI also persists `stepper_after_only` (default false). It switches the existing trace
between before/after and after-only without rerunning code; after highlights and inline
images are retained. Failed steps always show the expression that failed.

### server

Requests carry an `id`; responses echo it. Requests run concurrently; a newer `check-syntax` for
the same path cancels the older one.
```
{"id":1,"cmd":"check-syntax","path":..,"source":..,"offset":n}
 -> {"id":1,"ok":true,"result":{
      "arrows":[{"from":[s,e],"to":[s,e],"kind":"lexical"|"imported"|"tail","actual":bool}],
      "tooltips":[{"start":s,"end":e,"text":"..."}],
      "colors":[{"start":s,"end":e,"style":"lexically-bound"|"imported"|"set!d"|"free-variable"|"unused-identifier"|...}],
      "unused_requires":[[s,e]],
      "docs":[{"start":s,"end":e,"label":"map","path":"/.../doc/reference/pairs.html","anchor":"..."}],
      "jumps":[{"start":s,"end":e,"path":"/abs/other.rkt","id":"foo"}],
      "definitions":[{"start":s,"end":e,"name":"foo"}],
      "require_opens":[{"start":s,"end":e,"path":"/abs/required.rkt"}],
      "errors":[{"message":"...","srclocs":[srcloc...]}]}}
     ; read/expansion errors are results (ok:true, errors non-empty), not failures
{"id":2,"cmd":"bluebox","path":..,"source":..,"offset":n,"pos":p|null,"name":"map"|null}
 -> {"id":2,"ok":true,"result":{"lines":["(map proc lst ...+) -> list?", ...],"kind":"procedure",
                                "doc":{"path":..,"anchor":..}}|null}
{"id":3,"cmd":"docs-search","query":"map","limit":50}
 -> {"id":3,"ok":true,"result":[{"name":"map","libs":["racket/base"],"path":..,"anchor":..,"kind":"procedure"}]}
{"id":4,"cmd":"completions","language":"racket"|"lang/htdp-beginner"|<module path>}
 -> {"id":4,"ok":true,"result":["define","map",...]}
{"id":5,"cmd":"macro-steps","path":..,"source":..,"offset":n,"width":80,"policy":"standard"|"all"}
 -> {"id":5,"ok":true,"result":[{"type":"macro","name":"Macro transformation","before":"...",
     "after":"...","foci_before":[[s,e]],"foci_after":[[s,e]]}]}  ; foci index into before/after
{"id":6,"cmd":"module-graph","path":..,"source":""|"<unsaved text>","include_libs":false}
 -> {"id":6,"ok":true,"result":{"nodes":[{"path":..,"label":"main.rkt","lib":false}],"edges":[[i,j]...]}}
{"id":7,"cmd":"jump-target","path":"/abs/other.rkt","name":"foo"}
 -> {"id":7,"ok":true,"result":{"pos":p,"line":n,"header_length":n}|null}
     ; pos/line are relative to that file's visible text (after any DrRacket header)
{"cmd":"cancel","id":1}            ; that request then answers {"id":1,"ok":false,"error":"cancelled"}
errors: {"id":1,"ok":false,"error":"..."}
```

## Language levels

`src/racket/lang.rs` models every DrRacket language:

- **The Racket Language** (module language; `#lang` line in the buffer).
- **Teaching languages** (How to Design Programs): Beginning Student, Beginning Student with List
  Abbreviations, Intermediate Student, Intermediate Student with lambda, Advanced Student.
  Settings: case sensitivity, output style (constructor / quasiquote / write / print), fraction
  style (mixed / repeating decimal / improper), show sharing, insert newlines, tracing,
  teachpacks, true/false/empty as ids, enable tests.
- **DeinProgramm**: Schreibe Dein Programm! - Anfänger, Schreibe Dein Programm!,
  Schreibe Dein Programm! - fortgeschritten (the DMdA family is not part of current Racket).
- **Legacy**: R5RS, Pretty Big.

Files saved in a teaching language carry DrRacket's three-line metadata header
(`;; The first three lines ...` + `#reader(lib "htdp-beginner-reader.ss" "lang")((modname ..) ...)`),
byte-compatible with DrRacket. The editor hides the header; it is regenerated on save and parsed
on load, so files round-trip with DrRacket.

## Rust module map

```
src/main.rs            entry, window, fonts
src/app.rs             App: workspace state, layout, command dispatch
src/config.rs          settings (seeded from Zed), recents, paths
src/theme.rs           Zed theme loader
src/icons.rs           Material file-icon theme + UI glyph font
src/editor/            rope buffer, lexer, indenter, s-expression ops, editor widget
src/racket/            racket discovery, protocol types, runner/stepper/server clients, lang.rs
src/terminal/          terminal emulator widget (alacritty_terminal + portable PTY)
src/ui/                panels: file tree, repl, stepper, palette, welcome, dialogs, status bar
```
