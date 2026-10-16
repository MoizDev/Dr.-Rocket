//! DrRacket's language levels.
//!
//! The module language (`#lang ...`), the How to Design Programs and DeinProgramm teaching
//! languages, and the legacy R5RS / Pretty Big languages. Teaching-language files carry
//! DrRacket's three-line metadata header; [`load`] strips it (the editor never shows it) and
//! [`save`] writes it back byte-for-byte as DrRacket would, so files round-trip with DrRacket.

use super::protocol::RunLanguage;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Language {
    /// The Racket Language: the program's `#lang` line chooses the dialect.
    Module,
    Beginner,
    BeginnerAbbr,
    Intermediate,
    IntermediateLambda,
    Advanced,
    /// Schreibe Dein Programm! - Anfänger
    SdpBeginner,
    /// Schreibe Dein Programm!
    Sdp,
    /// Schreibe Dein Programm! - fortgeschritten
    SdpAdvanced,
    R5rs,
    PrettyBig,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Family {
    Module,
    Htdp,
    DeinProgramm,
    Legacy,
}

/// A section of the Choose Language dialog.
#[derive(Clone, Copy, Debug)]
pub struct LanguageGroup {
    /// Top-level heading, e.g. "Teaching Languages".
    pub heading: &'static str,
    /// Sub-heading within it, e.g. "How to Design Programs".
    pub title: &'static str,
    pub languages: &'static [Language],
}

pub const GROUPS: &[LanguageGroup] = &[
    LanguageGroup { heading: "The Racket Language", title: "", languages: &[Language::Module] },
    LanguageGroup {
        heading: "Teaching Languages",
        title: "How to Design Programs",
        languages: &[
            Language::Beginner,
            Language::BeginnerAbbr,
            Language::Intermediate,
            Language::IntermediateLambda,
            Language::Advanced,
        ],
    },
    LanguageGroup {
        heading: "Teaching Languages",
        title: "DeinProgramm",
        languages: &[Language::SdpBeginner, Language::Sdp, Language::SdpAdvanced],
    },
    LanguageGroup { heading: "Other Languages", title: "Legacy Languages", languages: &[Language::R5rs, Language::PrettyBig] },
];

impl Language {
    pub const ALL: [Language; 11] = [
        Language::Module,
        Language::Beginner,
        Language::BeginnerAbbr,
        Language::Intermediate,
        Language::IntermediateLambda,
        Language::Advanced,
        Language::SdpBeginner,
        Language::Sdp,
        Language::SdpAdvanced,
        Language::R5rs,
        Language::PrettyBig,
    ];

    /// Stable id, used in settings files.
    pub fn id(self) -> &'static str {
        match self {
            Language::Module => "module",
            Language::Beginner => "beginner",
            Language::BeginnerAbbr => "beginner-abbr",
            Language::Intermediate => "intermediate",
            Language::IntermediateLambda => "intermediate-lambda",
            Language::Advanced => "advanced",
            Language::SdpBeginner => "sdp-beginner",
            Language::Sdp => "sdp",
            Language::SdpAdvanced => "sdp-advanced",
            Language::R5rs => "r5rs",
            Language::PrettyBig => "pretty-big",
        }
    }

    pub fn from_id(id: &str) -> Option<Language> {
        Language::ALL.into_iter().find(|l| l.id() == id)
    }

    /// The name DrRacket shows.
    pub fn name(self) -> &'static str {
        match self {
            Language::Module => "The Racket Language",
            Language::Beginner => "Beginning Student",
            Language::BeginnerAbbr => "Beginning Student with List Abbreviations",
            Language::Intermediate => "Intermediate Student",
            Language::IntermediateLambda => "Intermediate Student with lambda",
            Language::Advanced => "Advanced Student",
            Language::SdpBeginner => "Schreibe Dein Programm! - Anfänger",
            Language::Sdp => "Schreibe Dein Programm!",
            Language::SdpAdvanced => "Schreibe Dein Programm! - fortgeschritten",
            Language::R5rs => "R5RS",
            Language::PrettyBig => "Pretty Big",
        }
    }

    /// Short name for the status bar.
    pub fn short_name(self) -> &'static str {
        match self {
            Language::Module => "Racket",
            Language::Beginner => "BSL",
            Language::BeginnerAbbr => "BSL+",
            Language::Intermediate => "ISL",
            Language::IntermediateLambda => "ISL+",
            Language::Advanced => "ASL",
            Language::SdpBeginner => "SDP Anfänger",
            Language::Sdp => "SDP",
            Language::SdpAdvanced => "SDP fortgeschritten",
            Language::R5rs => "R5RS",
            Language::PrettyBig => "Pretty Big",
        }
    }

    /// DrRacket's one-line summary.
    pub fn summary(self) -> &'static str {
        match self {
            Language::Module => "Start your program with #lang to choose a dialect, for example #lang racket",
            Language::Beginner => "define, cond, structs, constants, and primitives",
            Language::BeginnerAbbr => "Beginner, with list style printing in the REPL",
            Language::Intermediate => "Beginner plus lexical scope",
            Language::IntermediateLambda => "Intermediate plus higher-order functions",
            Language::Advanced => "Intermediate plus lambda and mutation",
            Language::SdpBeginner => "Definitions, conditionals, records, and signatures",
            Language::Sdp => "Anfänger plus higher-order functions and lists",
            Language::SdpAdvanced => "Plus quote, symbols, and mutation",
            Language::R5rs => "Scheme as described by the Revised5 Report",
            Language::PrettyBig => "Racket plus the HtDP teachpacks (legacy)",
        }
    }

    pub fn family(self) -> Family {
        match self {
            Language::Module => Family::Module,
            Language::Beginner
            | Language::BeginnerAbbr
            | Language::Intermediate
            | Language::IntermediateLambda
            | Language::Advanced => Family::Htdp,
            Language::SdpBeginner | Language::Sdp | Language::SdpAdvanced => Family::DeinProgramm,
            Language::R5rs | Language::PrettyBig => Family::Legacy,
        }
    }

    /// Teaching languages store their choice in a DrRacket header.
    pub fn is_teaching(self) -> bool {
        matches!(self.family(), Family::Htdp | Family::DeinProgramm)
    }

    /// The `#reader` spec written in the header.
    pub fn reader_module(self) -> Option<&'static str> {
        Some(match self {
            Language::Beginner => r#"(lib "htdp-beginner-reader.ss" "lang")"#,
            Language::BeginnerAbbr => r#"(lib "htdp-beginner-abbr-reader.ss" "lang")"#,
            Language::Intermediate => r#"(lib "htdp-intermediate-reader.ss" "lang")"#,
            Language::IntermediateLambda => r#"(lib "htdp-intermediate-lambda-reader.ss" "lang")"#,
            Language::Advanced => r#"(lib "htdp-advanced-reader.ss" "lang")"#,
            Language::SdpBeginner => r#"(lib "beginner-reader.rkt" "deinprogramm" "sdp")"#,
            Language::Sdp => r#"(lib "vanilla-reader.rkt" "deinprogramm" "sdp")"#,
            Language::SdpAdvanced => r#"(lib "advanced-reader.rkt" "deinprogramm" "sdp")"#,
            _ => return None,
        })
    }

    /// The module providing the language's bindings (for completions and docs).
    pub fn language_module(self) -> &'static str {
        match self {
            Language::Module => "racket",
            Language::Beginner => "lang/htdp-beginner",
            Language::BeginnerAbbr => "lang/htdp-beginner-abbr",
            Language::Intermediate => "lang/htdp-intermediate",
            Language::IntermediateLambda => "lang/htdp-intermediate-lambda",
            Language::Advanced => "lang/htdp-advanced",
            Language::SdpBeginner => "deinprogramm/sdp/beginner",
            Language::Sdp => "deinprogramm/sdp/vanilla",
            Language::SdpAdvanced => "deinprogramm/sdp/advanced",
            Language::R5rs => "r5rs",
            Language::PrettyBig => "lang/plt-pretty-big",
        }
    }

    /// How the runner evaluates programs in this language.
    pub fn run_language(self) -> RunLanguage {
        match self {
            Language::R5rs => RunLanguage::Toplevel { init: "r5rs".into() },
            Language::PrettyBig => RunLanguage::Toplevel { init: "lang/plt-pretty-big".into() },
            _ => RunLanguage::Module,
        }
    }

    /// Completions language key for a buffer: the language module, or for the module
    /// language the module named by the buffer's `#lang` line.
    pub fn completions_language(self, source: &str) -> String {
        if self != Language::Module {
            return self.language_module().to_string();
        }
        match hash_lang(source) {
            Some(name) => {
                if let Some(level) = teaching_level_for_lang_line(&name) {
                    return level.language_module().to_string();
                }
                // `#lang at-exp racket` and similar meta-languages: use the base language.
                name.split_whitespace().last().unwrap_or("racket").to_string()
            }
            None => "racket".into(),
        }
    }

    /// The stepper level for this language, or `None` when DrRacket's stepper doesn't
    /// support it. For the module language, `#lang htdp/bsl`, `bsl+`, `isl`, `isl+` work.
    pub fn stepper_level(self, source: &str) -> Option<&'static str> {
        match self {
            Language::Beginner => Some("beginner"),
            Language::BeginnerAbbr => Some("beginner-abbr"),
            Language::Intermediate => Some("intermediate"),
            Language::IntermediateLambda => Some("intermediate-lambda"),
            Language::SdpBeginner => Some("sdp-beginner"),
            Language::Sdp => Some("sdp-vanilla"),
            Language::Module => {
                let level = teaching_level_for_lang_line(&hash_lang(source)?)?;
                level.stepper_level("").map(|_| "module")
            }
            // Like DrRacket: the stepper doesn't support mutation-heavy levels.
            Language::Advanced | Language::SdpAdvanced | Language::R5rs | Language::PrettyBig => None,
        }
    }

    /// DrRacket offers its debugger for the module language and the advanced levels.
    pub fn supports_debugger(self) -> bool {
        matches!(self, Language::Module | Language::Advanced | Language::SdpAdvanced | Language::PrettyBig)
    }

    /// DrRacket's default settings for this level.
    pub fn default_settings(self) -> TeachingSettings {
        match self.family() {
            Family::DeinProgramm => TeachingSettings {
                case_sensitive: false,
                printing_style: PrintingStyle::Write,
                fraction_style: FractionStyle::RepeatingDecimal,
                show_sharing: false,
                insert_newlines: true,
                annotations: "none".into(),
                tracing: false,
                teachpacks: Vec::new(),
                true_false_empty_as_ids: false,
                writing_style: if self == Language::SdpAdvanced { WritingStyle::Datum } else { WritingStyle::Explicit },
            },
            _ => TeachingSettings {
                case_sensitive: true,
                printing_style: PrintingStyle::Constructor,
                fraction_style: FractionStyle::RepeatingDecimal,
                // Only Advanced Student shows sharing by default.
                show_sharing: self == Language::Advanced,
                insert_newlines: true,
                annotations: "none".into(),
                tracing: false,
                teachpacks: Vec::new(),
                true_false_empty_as_ids: false,
                writing_style: WritingStyle::Explicit,
            },
        }
    }

    /// Output styles DrRacket offers in this level's settings panel.
    pub fn printing_styles(self) -> &'static [PrintingStyle] {
        match self.family() {
            Family::DeinProgramm => &[PrintingStyle::Constructor, PrintingStyle::Quasiquote, PrintingStyle::Write],
            Family::Htdp if self == Language::Beginner => {
                // Beginning Student has no quote, so no quasiquote output.
                &[PrintingStyle::Constructor, PrintingStyle::Write, PrintingStyle::Print]
            }
            _ => &[PrintingStyle::Constructor, PrintingStyle::Quasiquote, PrintingStyle::Write, PrintingStyle::Print],
        }
    }

    /// The level used for `#lang htdp/...`-style files in the module language.
    pub fn from_lang_line(name: &str) -> Option<Language> {
        teaching_level_for_lang_line(name)
    }
}

fn teaching_level_for_lang_line(name: &str) -> Option<Language> {
    Some(match name.trim() {
        "htdp/bsl" => Language::Beginner,
        "htdp/bsl+" => Language::BeginnerAbbr,
        "htdp/isl" => Language::Intermediate,
        "htdp/isl+" => Language::IntermediateLambda,
        "htdp/asl" => Language::Advanced,
        "deinprogramm/sdp/beginner" => Language::SdpBeginner,
        "deinprogramm/sdp" | "deinprogramm/sdp/vanilla" => Language::Sdp,
        "deinprogramm/sdp/advanced" => Language::SdpAdvanced,
        _ => return None,
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PrintingStyle {
    Constructor,
    Quasiquote,
    Write,
    TradWrite,
    Print,
}

impl PrintingStyle {
    pub fn symbol(self) -> &'static str {
        match self {
            PrintingStyle::Constructor => "constructor",
            PrintingStyle::Quasiquote => "quasiquote",
            PrintingStyle::Write => "write",
            PrintingStyle::TradWrite => "trad-write",
            PrintingStyle::Print => "print",
        }
    }

    pub fn from_symbol(s: &str) -> Option<Self> {
        [Self::Constructor, Self::Quasiquote, Self::Write, Self::TradWrite, Self::Print]
            .into_iter()
            .find(|p| p.symbol() == s)
    }

    pub fn label(self) -> &'static str {
        match self {
            PrintingStyle::Constructor => "Constructor",
            PrintingStyle::Quasiquote => "Quasiquote",
            PrintingStyle::Write => "write",
            PrintingStyle::TradWrite => "traditional write",
            PrintingStyle::Print => "print",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FractionStyle {
    MixedFraction,
    MixedFractionE,
    RepeatingDecimal,
    RepeatingDecimalE,
}

impl FractionStyle {
    pub fn symbol(self) -> &'static str {
        match self {
            FractionStyle::MixedFraction => "mixed-fraction",
            FractionStyle::MixedFractionE => "mixed-fraction-e",
            FractionStyle::RepeatingDecimal => "repeating-decimal",
            FractionStyle::RepeatingDecimalE => "repeating-decimal-e",
        }
    }

    pub fn from_symbol(s: &str) -> Option<Self> {
        [Self::MixedFraction, Self::MixedFractionE, Self::RepeatingDecimal, Self::RepeatingDecimalE]
            .into_iter()
            .find(|f| f.symbol() == s)
    }

    pub fn label(self) -> &'static str {
        match self {
            FractionStyle::MixedFraction => "Mixed fractions",
            FractionStyle::MixedFractionE => "Mixed fractions with #e prefix",
            FractionStyle::RepeatingDecimal => "Repeating decimals",
            FractionStyle::RepeatingDecimalE => "Repeating decimals with #e prefix",
        }
    }
}

/// DeinProgramm's "Schreibweise": how compound values are written.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum WritingStyle {
    Explicit,
    Datum,
}

impl WritingStyle {
    pub fn symbol(self) -> &'static str {
        match self {
            WritingStyle::Explicit => "explicit",
            WritingStyle::Datum => "datum",
        }
    }
}

/// The settings DrRacket stores in a teaching-language header.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct TeachingSettings {
    pub case_sensitive: bool,
    pub printing_style: PrintingStyle,
    pub fraction_style: FractionStyle,
    pub show_sharing: bool,
    pub insert_newlines: bool,
    /// DrRacket's annotation setting symbol: "none", "debug", "debug/profile", "test-coverage".
    pub annotations: String,
    pub tracing: bool,
    /// Require specs as s-expression text, e.g. `(lib "image.rkt" "teachpack" "2htdp")`.
    pub teachpacks: Vec<String>,
    /// HtDP only: print `true`/`false`/`empty` instead of `#true`/`#false`/`'()`.
    pub true_false_empty_as_ids: bool,
    /// DeinProgramm only.
    pub writing_style: WritingStyle,
}

impl Default for TeachingSettings {
    fn default() -> Self {
        Language::Beginner.default_settings()
    }
}

/// Teachpacks DrRacket offers, as (label, require spec).
pub const TEACHPACKS: &[(&str, &str)] = &[
    ("2htdp/image", r#"(lib "image.rkt" "teachpack" "2htdp")"#),
    ("2htdp/universe", r#"(lib "universe.rkt" "teachpack" "2htdp")"#),
    ("2htdp/batch-io", r#"(lib "batch-io.rkt" "teachpack" "2htdp")"#),
    ("2htdp/abstraction", r#"(lib "abstraction.rkt" "teachpack" "2htdp")"#),
    ("2htdp/itunes", r#"(lib "itunes.rkt" "teachpack" "2htdp")"#),
    ("2htdp/planetcute", r#"(lib "planetcute.rkt" "teachpack" "2htdp")"#),
    ("2htdp/web-io", r#"(lib "web-io.rkt" "teachpack" "2htdp")"#),
    ("htdp/dir", r#"(lib "dir.rkt" "teachpack" "htdp")"#),
    ("htdp/draw", r#"(lib "draw.rkt" "teachpack" "htdp")"#),
    ("htdp/guess", r#"(lib "guess.rkt" "teachpack" "htdp")"#),
    ("htdp/hangman", r#"(lib "hangman.rkt" "teachpack" "htdp")"#),
    ("htdp/matrix", r#"(lib "matrix.rkt" "teachpack" "htdp")"#),
];

impl TeachingSettings {
    /// The settings object the stepper process expects.
    pub fn stepper_json(&self) -> serde_json::Value {
        serde_json::json!({
            "case_sensitive": self.case_sensitive,
            "printing_style": self.printing_style.symbol(),
            "fraction_style": self.fraction_style.symbol(),
            "show_sharing": self.show_sharing,
            "insert_newlines": self.insert_newlines,
            "true_false_empty_as_ids": self.true_false_empty_as_ids,
            "writing_style": self.writing_style.symbol(),
        })
    }

    fn teachpack_list(&self) -> String {
        format!("({})", self.teachpacks.join(" "))
    }
}

// ---------------------------------------------------------------------------
// Headers

const HTDP_PREFIX: [&str; 2] = [
    ";; The first three lines of this file were inserted by DrRacket. They record metadata",
    ";; about the language level of this file in a form that our tools can easily process.",
];

const DEINPROGRAMM_PREFIX: [&str; 2] = [
    ";; Die ersten drei Zeilen dieser Datei wurden von DrRacket eingefügt. Sie enthalten Metadaten",
    ";; über die Sprachebene dieser Datei in einer Form, die DrRacket verarbeiten kann.",
];

fn rbool(b: bool) -> &'static str {
    if b { "#t" } else { "#f" }
}

/// The three-line DrRacket header for a teaching-language file, or `None` for languages
/// that don't use one.
pub fn header(language: Language, modname: &str, settings: &TeachingSettings) -> Option<String> {
    let reader = language.reader_module()?;
    let s = settings;
    let tps = s.teachpack_list();
    let modname = write_symbol(modname);
    Some(match language.family() {
        Family::Htdp => format!(
            "{}\n{}\n#reader{reader}((modname {modname}) (read-case-sensitive {cs}) (teachpacks {tps}) (htdp-settings #({cs} {ps} {fs} {sh} {nl} {an} {tr} {tps} {tf})))\n",
            HTDP_PREFIX[0],
            HTDP_PREFIX[1],
            cs = rbool(s.case_sensitive),
            ps = s.printing_style.symbol(),
            fs = s.fraction_style.symbol(),
            sh = rbool(s.show_sharing),
            nl = rbool(s.insert_newlines),
            an = s.annotations,
            tr = rbool(s.tracing),
            tf = rbool(s.true_false_empty_as_ids),
        ),
        Family::DeinProgramm => format!(
            "{}\n{}\n#reader{reader}((modname {modname}) (read-case-sensitive {cs}) (teachpacks {tps}) (deinprogramm-settings #({cs} {ps} {fs} {sh} {nl} {an} {ws} {tr} {tps})))\n",
            DEINPROGRAMM_PREFIX[0],
            DEINPROGRAMM_PREFIX[1],
            cs = rbool(s.case_sensitive),
            ps = s.printing_style.symbol(),
            fs = s.fraction_style.symbol(),
            sh = rbool(s.show_sharing),
            nl = rbool(s.insert_newlines),
            an = s.annotations,
            ws = s.writing_style.symbol(),
            tr = rbool(s.tracing),
        ),
        _ => return None,
    })
}

/// The module name DrRacket uses for a file: its name without extension.
pub fn modname_for(path: Option<&Path>) -> String {
    path.and_then(|p| p.file_stem())
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "unsaved".into())
}

/// A parsed DrRacket header.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Header {
    pub language: Language,
    pub settings: TeachingSettings,
    pub modname: Option<String>,
    /// Length of the header in chars (through the third line's newline).
    pub len_chars: usize,
    /// Length of the header in bytes.
    pub len_bytes: usize,
}

/// Parse DrRacket's metadata header at the start of `text`, if present.
pub fn parse_header(text: &str) -> Option<Header> {
    let mut lines = text.split_inclusive('\n');
    let l1 = lines.next()?;
    let l2 = lines.next()?;
    let l3 = lines.next()?;
    if !(l1.starts_with(";;") && l2.starts_with(";;") && l3.starts_with("#reader")) {
        return None;
    }
    let len_bytes = l1.len() + l2.len() + l3.len();
    let len_chars = text[..len_bytes].chars().count();
    let rest = l3.strip_prefix("#reader")?;
    let (reader, used) = sexp::parse(rest)?;
    let (table, _) = sexp::parse(&rest[used..])?;
    let language = language_for_reader(&reader)?;
    let mut settings = language.default_settings();
    let mut modname = None;
    let entries = table.as_list()?;
    let lookup = |key: &str| {
        entries.iter().find_map(|e| {
            let l = e.as_list()?;
            (l.first()?.as_sym()? == key).then(|| l.get(1)).flatten()
        })
    };
    if let Some(m) = lookup("modname").and_then(|v| v.as_sym()) {
        modname = Some(m.to_string());
    }
    if let Some(b) = lookup("read-case-sensitive").and_then(|v| v.as_bool()) {
        settings.case_sensitive = b;
    }
    if let Some(tps) = lookup("teachpacks").and_then(|v| v.as_list()) {
        settings.teachpacks = tps.iter().map(|t| t.to_string()).collect();
    }
    let key = if language.family() == Family::DeinProgramm { "deinprogramm-settings" } else { "htdp-settings" };
    if let Some(v) = lookup(key).and_then(|v| v.as_vector()) {
        apply_settings_vector(language, v, &mut settings);
    }
    Some(Header { language, settings, modname, len_chars, len_bytes })
}

fn apply_settings_vector(language: Language, v: &[sexp::Sexp], s: &mut TeachingSettings) {
    let b = |i: usize| v.get(i).and_then(|x| x.as_bool());
    let sym = |i: usize| v.get(i).and_then(|x| x.as_sym());
    if let Some(x) = b(0) {
        s.case_sensitive = x;
    }
    if let Some(x) = sym(1).and_then(PrintingStyle::from_symbol) {
        s.printing_style = x;
    }
    if let Some(x) = sym(2).and_then(FractionStyle::from_symbol) {
        s.fraction_style = x;
    }
    if let Some(x) = b(3) {
        s.show_sharing = x;
    }
    if let Some(x) = b(4) {
        s.insert_newlines = x;
    }
    if let Some(x) = sym(5) {
        s.annotations = x.to_string();
    }
    match language.family() {
        Family::DeinProgramm => {
            match sym(6) {
                Some("datum") => s.writing_style = WritingStyle::Datum,
                Some("explicit") => s.writing_style = WritingStyle::Explicit,
                _ => {}
            }
            if let Some(x) = b(7) {
                s.tracing = x;
            }
        }
        _ => {
            if let Some(x) = b(6) {
                s.tracing = x;
            }
            if let Some(x) = b(8) {
                s.true_false_empty_as_ids = x;
            }
        }
    }
}

fn language_for_reader(reader: &sexp::Sexp) -> Option<Language> {
    let parts = reader.as_list()?;
    if parts.first()?.as_sym()? != "lib" {
        return None;
    }
    let strs: Vec<&str> = parts[1..].iter().filter_map(|p| p.as_str()).collect();
    let file = strs.first()?.trim_end_matches(".ss").trim_end_matches(".rkt");
    let collection = strs[1..].join("/");
    Some(match (collection.as_str(), file) {
        ("lang", "htdp-beginner-reader") => Language::Beginner,
        ("lang", "htdp-beginner-abbr-reader") => Language::BeginnerAbbr,
        ("lang", "htdp-intermediate-reader") => Language::Intermediate,
        ("lang", "htdp-intermediate-lambda-reader") => Language::IntermediateLambda,
        ("lang", "htdp-advanced-reader") => Language::Advanced,
        ("deinprogramm/sdp", "beginner-reader") => Language::SdpBeginner,
        ("deinprogramm/sdp", "vanilla-reader") => Language::Sdp,
        ("deinprogramm/sdp", "advanced-reader") => Language::SdpAdvanced,
        _ => return None,
    })
}

/// A file as the editor sees it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LoadedFile {
    pub language: Language,
    pub settings: TeachingSettings,
    /// The text without any DrRacket header.
    pub body: String,
    /// The header that was stripped (empty if none).
    pub header: String,
}

/// Split a file into its language and visible text. Files with a DrRacket header are
/// teaching-language files; everything else is in the module language.
pub fn load(text: &str) -> LoadedFile {
    match parse_header(text) {
        Some(h) => LoadedFile {
            language: h.language,
            settings: h.settings,
            body: text[h.len_bytes..].to_string(),
            header: text[..h.len_bytes].to_string(),
        },
        None => LoadedFile {
            language: Language::Module,
            settings: TeachingSettings::default(),
            body: text.to_string(),
            header: String::new(),
        },
    }
}

/// The file contents to write: the DrRacket header (teaching languages) followed by the body.
pub fn save(language: Language, settings: &TeachingSettings, path: Option<&Path>, body: &str) -> String {
    match header(language, &modname_for(path), settings) {
        Some(h) => h + body,
        None => body.to_string(),
    }
}

/// The program text to send to the runner and its hidden-prefix length in chars.
pub fn program_text(language: Language, settings: &TeachingSettings, path: Option<&Path>, body: &str) -> (String, usize) {
    match header(language, &modname_for(path), settings) {
        Some(h) => {
            let n = h.chars().count();
            (h + body, n)
        }
        None => (body.to_string(), 0),
    }
}

/// The language named by a `#lang` (or `#!`) line, skipping leading comments.
pub fn hash_lang(text: &str) -> Option<String> {
    let mut s = text;
    loop {
        s = s.trim_start();
        if let Some(rest) = s.strip_prefix(';') {
            s = rest.split_once('\n').map(|(_, r)| r).unwrap_or("");
        } else if s.starts_with("#|") {
            s = skip_block_comment(s)?;
        } else {
            break;
        }
    }
    let rest = if let Some(r) = s.strip_prefix("#lang") {
        if !r.starts_with([' ', '\t']) {
            return None;
        }
        r
    } else {
        s.strip_prefix("#!")?
    };
    let line = rest.split('\n').next().unwrap_or("").trim();
    (!line.is_empty()).then(|| line.to_string())
}

fn skip_block_comment(s: &str) -> Option<&str> {
    let mut depth = 0usize;
    let mut i = 0;
    let b = s.as_bytes();
    while i + 1 < b.len() {
        if b[i] == b'#' && b[i + 1] == b'|' {
            depth += 1;
            i += 2;
        } else if b[i] == b'|' && b[i + 1] == b'#' {
            depth -= 1;
            i += 2;
            if depth == 0 {
                return Some(&s[i..]);
            }
        } else {
            i += 1;
        }
    }
    None
}

/// Write a symbol the way Racket's `~s` does for ordinary names.
fn write_symbol(name: &str) -> String {
    let plain = !name.is_empty()
        && !name.starts_with('#')
        && name.parse::<f64>().is_err()
        && name != "."
        && name.chars().all(|c| !c.is_whitespace() && !"()[]{}\",'`;|\\".contains(c));
    if plain {
        name.to_string()
    } else if !name.contains('|') && !name.contains('\\') {
        format!("|{name}|")
    } else {
        name.chars()
            .flat_map(|c| {
                let esc = c.is_whitespace() || "()[]{}\",'`;|\\#".contains(c);
                esc.then_some('\\').into_iter().chain(std::iter::once(c))
            })
            .collect()
    }
}

/// A tiny s-expression reader, enough for DrRacket's header metadata.
mod sexp {
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub enum Sexp {
        List(Vec<Sexp>),
        Vector(Vec<Sexp>),
        Str(String),
        Sym(String),
        Bool(bool),
    }

    impl Sexp {
        pub fn as_list(&self) -> Option<&[Sexp]> {
            match self {
                Sexp::List(l) => Some(l),
                _ => None,
            }
        }
        pub fn as_vector(&self) -> Option<&[Sexp]> {
            match self {
                Sexp::Vector(v) => Some(v),
                _ => None,
            }
        }
        pub fn as_sym(&self) -> Option<&str> {
            match self {
                Sexp::Sym(s) => Some(s),
                _ => None,
            }
        }
        pub fn as_str(&self) -> Option<&str> {
            match self {
                Sexp::Str(s) => Some(s),
                _ => None,
            }
        }
        pub fn as_bool(&self) -> Option<bool> {
            match self {
                Sexp::Bool(b) => Some(*b),
                _ => None,
            }
        }
    }

    impl std::fmt::Display for Sexp {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            let seq = |f: &mut std::fmt::Formatter<'_>, items: &[Sexp]| -> std::fmt::Result {
                for (i, x) in items.iter().enumerate() {
                    if i > 0 {
                        f.write_str(" ")?;
                    }
                    write!(f, "{x}")?;
                }
                Ok(())
            };
            match self {
                Sexp::List(l) => {
                    f.write_str("(")?;
                    seq(f, l)?;
                    f.write_str(")")
                }
                Sexp::Vector(v) => {
                    f.write_str("#(")?;
                    seq(f, v)?;
                    f.write_str(")")
                }
                Sexp::Str(s) => {
                    f.write_str("\"")?;
                    for c in s.chars() {
                        match c {
                            '"' => f.write_str("\\\"")?,
                            '\\' => f.write_str("\\\\")?,
                            '\n' => f.write_str("\\n")?,
                            c => write!(f, "{c}")?,
                        }
                    }
                    f.write_str("\"")
                }
                Sexp::Sym(s) => f.write_str(&super::write_symbol(s)),
                Sexp::Bool(b) => f.write_str(if *b { "#t" } else { "#f" }),
            }
        }
    }

    /// Parse one datum from the start of `s`; returns it and the bytes consumed.
    pub fn parse(s: &str) -> Option<(Sexp, usize)> {
        let mut p = Parser { s, i: 0 };
        let v = p.datum()?;
        Some((v, p.i))
    }

    struct Parser<'a> {
        s: &'a str,
        i: usize,
    }

    impl Parser<'_> {
        fn peek(&self) -> Option<char> {
            self.s[self.i..].chars().next()
        }
        fn bump(&mut self) -> Option<char> {
            let c = self.peek()?;
            self.i += c.len_utf8();
            Some(c)
        }
        fn skip_ws(&mut self) {
            while self.peek().is_some_and(char::is_whitespace) {
                self.bump();
            }
        }
        fn datum(&mut self) -> Option<Sexp> {
            self.skip_ws();
            match self.peek()? {
                '(' | '[' => {
                    let close = if self.bump()? == '(' { ')' } else { ']' };
                    Some(Sexp::List(self.items(close)?))
                }
                '#' if self.s[self.i..].starts_with("#(") => {
                    self.i += 2;
                    Some(Sexp::Vector(self.items(')')?))
                }
                '"' => {
                    self.bump();
                    let mut out = String::new();
                    loop {
                        match self.bump()? {
                            '"' => return Some(Sexp::Str(out)),
                            '\\' => match self.bump()? {
                                'n' => out.push('\n'),
                                't' => out.push('\t'),
                                c => out.push(c),
                            },
                            c => out.push(c),
                        }
                    }
                }
                '|' => {
                    self.bump();
                    let mut out = String::new();
                    loop {
                        match self.bump()? {
                            '|' => return Some(Sexp::Sym(out)),
                            c => out.push(c),
                        }
                    }
                }
                _ => {
                    let start = self.i;
                    let mut out = String::new();
                    while let Some(c) = self.peek() {
                        if c.is_whitespace() || "()[]\"".contains(c) {
                            break;
                        }
                        self.bump();
                        if c == '\\' {
                            out.push(self.bump()?);
                        } else {
                            out.push(c);
                        }
                    }
                    if self.i == start {
                        return None;
                    }
                    Some(match out.as_str() {
                        "#t" | "#true" => Sexp::Bool(true),
                        "#f" | "#false" => Sexp::Bool(false),
                        _ => Sexp::Sym(out),
                    })
                }
            }
        }
        fn items(&mut self, close: char) -> Option<Vec<Sexp>> {
            let mut items = Vec::new();
            loop {
                self.skip_ws();
                if self.peek()? == close {
                    self.bump();
                    return Some(items);
                }
                items.push(self.datum()?);
            }
        }
    }
}
