# Third-party notices

Dr. Rocket is licensed under either of the [MIT license](LICENSE-MIT) or the
[Apache License 2.0](LICENSE-APACHE), at your option. It includes or builds on the work below.
The full license texts are in [licenses/](licenses).

## Bundled with the app

- **File icons**: [Material Icon Theme](https://github.com/material-extensions/vscode-material-icon-theme)
  by Material Extensions, MIT License ([licenses/MaterialIconTheme-MIT.txt](licenses/MaterialIconTheme-MIT.txt)).
- **UI glyphs**: [Material Symbols](https://github.com/google/material-design-icons) by Google,
  Apache License 2.0 ([LICENSE-APACHE](LICENSE-APACHE)), subsetted by `tools/subset_icons.py`.
- **Fonts**: [Fira Code](https://github.com/tonsky/FiraCode) by The Fira Code Project Authors and
  [Inter](https://github.com/rsms/inter) by The Inter Project Authors, both under the SIL Open
  Font License 1.1 ([licenses/FiraCode-OFL.txt](licenses/FiraCode-OFL.txt),
  [licenses/Inter-OFL.txt](licenses/Inter-OFL.txt)).

## Color themes

The built-in themes are Dr. Rocket's own, written for its theme format, but their colors come
from these palettes:

- **One Dark and One Light**: Atom's themes, Copyright (c) 2011-2022 GitHub Inc., MIT License
  ([licenses/Atom-OneDark-MIT.txt](licenses/Atom-OneDark-MIT.txt)).
- **Catppuccin**: Copyright (c) 2021 Catppuccin, MIT License
  ([licenses/Catppuccin-MIT.txt](licenses/Catppuccin-MIT.txt)).
- **Tokyo Night**: Copyright (c) 2018-present Enkia, MIT License
  ([licenses/TokyoNight-MIT.txt](licenses/TokyoNight-MIT.txt)).
- **GitHub**: Primer's GitHub theme, Copyright (c) 2020 Primer, MIT License
  ([licenses/GitHubTheme-MIT.txt](licenses/GitHubTheme-MIT.txt)).
- **Gruvbox**: by Pavel Pertsev ([morhetz/gruvbox](https://github.com/morhetz/gruvbox)), MIT/X11
  License.

## Racket and DrRacket

Dr. Rocket is not affiliated with the Racket project. It runs the Racket you install from
[racket-lang.org](https://racket-lang.org) and uses Racket's own libraries (the stepper, check
syntax, the debugger and others) through that installation. Its indentation is a port of
DrRacket's (`src/editor/sexp.rs`). Racket and DrRacket are distributed under the MIT license or
the Apache License 2.0, at your option.

## Rust libraries

The app is built with the open-source Rust crates listed in `Cargo.toml` and `Cargo.lock`, each
under its own license: almost all MIT or Apache-2.0, plus two unmodified MPL-2.0 crates
(`nucleo-matcher`, `option-ext`). egui's built-in fallback fonts (Ubuntu, Hack, Noto Emoji and
Emoji Icon Font) come with the `epaint_default_fonts` crate under their own licenses.
