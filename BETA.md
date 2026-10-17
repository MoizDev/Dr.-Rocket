# Dr. Rocket beta

Thanks for trying Dr. Rocket, a fast, modern replacement for DrRacket. This is a beta: it
should handle your Racket work, and when it doesn't, we want to hear about it.

## Install

You need **Racket** (any recent version; we test with 9.3). If you have DrRacket, you
already have it. Otherwise get it from [racket-lang.org](https://racket-lang.org).

**macOS:** open `Dr.Rocket-<version>.dmg` and drag Dr. Rocket into Applications. The beta
isn't signed with an Apple certificate yet, so the first time, **right-click the app and
choose Open**, then Open again. (If macOS only offers "Move to Trash", open System Settings
→ Privacy & Security and click **Open Anyway** next to Dr. Rocket.)

**Windows:** unzip `Dr.Rocket-<version>-windows-x64.zip` and run `Dr. Rocket.exe`. Windows
may show "Windows protected your PC" because the beta isn't signed: click **More info** →
**Run anyway**.

**Linux (x64):** extract `Dr.Rocket-<version>-linux-x64.tar.gz` and run `drrocket` inside
it. To add it to your app launcher, copy `drrocket` onto your `PATH` (e.g. `~/.local/bin`),
`drrocket.desktop` to `~/.local/share/applications` and `drrocket.png` to
`~/.local/share/icons`.

Dr. Rocket finds Racket on its own. If it says "Racket not found", set the path in
Settings → Racket.

## What to try

Use it for your real work for a week, the way you'd use DrRacket. In particular:

- Your assignments in Beginning/Intermediate Student (Choose Language: Ctrl/⌘ L)
- **Run** (⌘R / Ctrl+R), the REPL, and check-expect results
- The **Stepper** (⇧⌘E / Ctrl+Shift+E): does it match DrRacket?
- The **Visualizer** (⇧⌘V / Ctrl+Shift+V): run a recursive function, a list or a tree
- Files from DrRacket **with pasted images** (open, run, edit, save, reopen in DrRacket)
- The **Debugger**, check syntax (hover, rename with F2), and the docs (F1)

## Known limitations

- XML boxes, Racket boxes, test-case boxes and large letters from DrRacket aren't editable:
  files with them open read-only (they're never damaged).
- Non-Racket `#lang`s (like Scribble) are coloured as Racket code.
- No custom key bindings or spell checking yet.
- DrRacket plugins (such as a course's handin "Submit" button) don't run in Dr. Rocket.

## Reporting a problem

1. Choose **Report a Problem…** from the ☰ menu (or the command palette, ⇧⌘P /
   Ctrl+Shift+P). It copies your version, system and the latest crash report, if any.
2. Paste it into a message to us and add **what you were doing** and, if you can, the file.

If Dr. Rocket crashes, it saves a report automatically and reminds you on the next launch.
