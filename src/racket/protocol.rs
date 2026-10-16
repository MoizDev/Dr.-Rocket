//! Wire types for the JSON-lines protocol spoken with the Racket backend processes.
//! See DESIGN.md ("Wire protocol"). All positions are 0-based character offsets into the
//! visible buffer (the backend has already subtracted any hidden DrRacket header).

use serde::{Deserialize, Serialize};

/// A source location. `line` is 1-based, `col` and `pos` are 0-based (`pos` in chars).
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct Srcloc {
    /// `None` for the unsaved main buffer or for interactions.
    pub path: Option<String>,
    pub line: Option<u32>,
    pub col: Option<u32>,
    pub pos: Option<usize>,
    pub span: Option<usize>,
}

impl Srcloc {
    /// The char range `[pos, pos + span)` if both are known.
    pub fn range(&self) -> Option<(usize, usize)> {
        Some((self.pos?, self.pos? + self.span.unwrap_or(0)))
    }
}

/// A half-open char range `[start, end)`, serialized as a two-element array.
pub type Range = (usize, usize);

/// A `(position, length)` pair, serialized as a two-element array (used by coverage).
pub type Span = (usize, usize);

// ---------------------------------------------------------------------------
// Runner

/// Which language family the runner should use to evaluate the program.
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum RunLanguage {
    /// A module: `#lang ...` or a DrRacket `#reader` header in the source.
    Module,
    /// A top-level language such as R5RS or Pretty Big; `init` is the module to require.
    Toplevel { init: String },
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Instrument {
    /// No instrumentation (fastest).
    None,
    /// DrRacket's "Debugging": errortrace stack traces with precise source locations.
    #[default]
    Errortrace,
    /// Errortrace plus test-coverage reporting.
    Coverage,
    /// Sampling profiler around the program run.
    Profile,
    /// The single-stepping debugger (breakpoints, step into/over/out).
    Debug,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct RunOptions {
    pub instrument: Instrument,
    /// Submodules to run after the main module, in order (DrRacket: `main`, `test`).
    pub submodules: Vec<String>,
    pub memory_limit_mb: Option<u64>,
    /// Initial breakpoints (char positions) for [`Instrument::Debug`].
    pub breakpoints: Vec<usize>,
    /// Column width used to pretty-print results.
    pub print_width: usize,
    /// Trace function calls (DrRacket's teaching-language "Tracing" option).
    pub trace: bool,
    /// With [`Instrument::Debug`]: other open files to debug if the program requires them.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub debug_modules: Vec<DebugModule>,
}

/// Another file to debug (its current editor text, which may be unsaved).
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct DebugModule {
    pub path: String,
    pub source: String,
    /// Length in chars of a hidden DrRacket header at the start of `source`.
    pub offset: usize,
    /// Breakpoints (0-based char positions in the visible buffer).
    pub breakpoints: Vec<usize>,
}

impl Default for RunOptions {
    fn default() -> Self {
        Self {
            instrument: Instrument::Errortrace,
            submodules: vec!["main".into(), "test".into()],
            memory_limit_mb: None,
            breakpoints: Vec::new(),
            print_width: 80,
            trace: false,
            debug_modules: Vec::new(),
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum DebugAction {
    Continue,
    StepInto,
    StepOver,
    StepOut,
    Pause,
}

#[derive(Serialize)]
#[serde(tag = "cmd", rename_all = "kebab-case")]
pub(crate) enum RunnerCommand<'a> {
    Run {
        path: Option<&'a str>,
        source: &'a str,
        offset: usize,
        language: &'a RunLanguage,
        options: &'a RunOptions,
    },
    Eval {
        id: u64,
        source: &'a str,
    },
    Input {
        text: &'a str,
    },
    InputEof,
    Break,
    Debug {
        action: DebugAction,
    },
    Breakpoints {
        positions: &'a [usize],
        /// `None` for the main program, else another debugged file.
        #[serde(skip_serializing_if = "Option::is_none")]
        path: Option<&'a str>,
    },
    Complete {
        id: u64,
        prefix: &'a str,
    },
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Stream {
    Out,
    Err,
    /// Function-call tracing output (teaching languages' Tracing option).
    Trace,
}

/// A PNG image produced by the program (e.g. a `2htdp/image` value).
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct ImageData {
    /// Base64-encoded PNG bytes.
    pub png: String,
    pub width: u32,
    pub height: u32,
}

impl ImageData {
    pub fn decode_png(&self) -> Option<Vec<u8>> {
        use base64::Engine;
        base64::engine::general_purpose::STANDARD.decode(&self.png).ok()
    }
}

/// A piece of a printed value: text, or an inline image.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum Segment {
    Text { text: String },
    Image(ImageData),
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ErrorKind {
    Read,
    Syntax,
    Runtime,
    Break,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct StackFrame {
    /// Procedure name or the source text of the expression, when known.
    pub name: Option<String>,
    #[serde(flatten)]
    pub srcloc: Srcloc,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct ErrorReport {
    pub message: String,
    pub kind: ErrorKind,
    /// Where to highlight; the first entry is the primary location.
    pub srclocs: Vec<Srcloc>,
    pub stack: Vec<StackFrame>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct TestFailure {
    pub message: String,
    pub srcloc: Srcloc,
}

/// check-expect results from the teaching languages' test engine.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct TestReport {
    pub passed: usize,
    pub total: usize,
    /// DrRacket-style summary ("Ran 2 tests. 1 of the 2 tests failed. ...").
    pub text: String,
    pub failures: Vec<TestFailure>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct ProfileNode {
    pub name: String,
    pub srcloc: Option<Srcloc>,
    pub self_ms: f64,
    pub total_ms: f64,
    /// Indices into the report's `nodes`.
    pub callers: Vec<usize>,
    pub callees: Vec<usize>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct ProfileReport {
    pub total_ms: f64,
    pub nodes: Vec<ProfileNode>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct Local {
    pub name: String,
    pub value: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct DebugFrame {
    pub name: String,
    pub srcloc: Srcloc,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct PausedState {
    pub srcloc: Srcloc,
    /// Frame locals first, then module-level variables.
    pub locals: Vec<Local>,
    /// Innermost frame first.
    pub stack: Vec<DebugFrame>,
    /// The value(s) of the expression just evaluated, when paused after it.
    pub result: Option<String>,
}

/// Events from the runner. `Terminated` is synthesized locally when the process ends.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(tag = "ev", rename_all = "kebab-case")]
pub enum RunEvent {
    Started {
        racket_version: String,
        language: String,
    },
    /// Program output. Images written with `display`/`write` arrive with `image` set.
    Output {
        stream: Stream,
        text: String,
        image: Option<ImageData>,
    },
    /// A printed result.
    Value {
        text: String,
        segments: Vec<Segment>,
    },
    Error(ErrorReport),
    Tests(TestReport),
    Coverage {
        covered: Vec<Span>,
        uncovered: Vec<Span>,
    },
    Profile(ProfileReport),
    Paused(PausedState),
    Resumed,
    /// User code is blocked reading from stdin.
    InputWanted,
    Completions {
        id: u64,
        items: Vec<String>,
    },
    /// Ready for the next interaction. `id` is `None` after the definitions ran.
    Ready {
        id: Option<u64>,
    },
    /// The program called `exit`.
    Exit {
        code: i32,
    },
    /// The process ended (or could not be read). Never sent by Racket.
    #[serde(skip)]
    Terminated {
        code: Option<i32>,
        stderr: String,
    },
}

// ---------------------------------------------------------------------------
// Stepper

#[derive(Serialize)]
#[serde(tag = "cmd", rename_all = "kebab-case")]
pub(crate) enum StepperCommand<'a> {
    Step {
        path: Option<&'a str>,
        source: &'a str,
        level: &'a str,
        settings: &'a serde_json::Value,
        teachpacks: &'a [String],
        width: usize,
    },
    More,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct SrcSpan {
    pub pos: usize,
    pub span: usize,
}

/// One top-level expression in a step, pretty-printed, with highlighted subterms.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct StepExpr {
    pub text: String,
    /// Char ranges into `text`.
    pub highlights: Vec<Range>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct Step {
    pub index: usize,
    /// "normal" or "user-application".
    pub kind: String,
    pub before: Vec<StepExpr>,
    pub after: Vec<StepExpr>,
    pub before_src: Option<SrcSpan>,
    pub after_src: Option<SrcSpan>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(tag = "ev", rename_all = "kebab-case")]
pub enum StepperEvent {
    Step(Step),
    /// The step's reduction raised an error.
    StepError {
        index: usize,
        before: Vec<StepExpr>,
        message: String,
        before_src: Option<SrcSpan>,
    },
    Error {
        message: String,
    },
    /// Every 500 steps the stepper waits for [`crate::racket::stepper::StepperSession::more`].
    Paused {
        count: usize,
    },
    Finished {
        count: usize,
    },
    #[serde(skip)]
    Terminated {
        code: Option<i32>,
        stderr: String,
    },
}

// ---------------------------------------------------------------------------
// Server

/// A request to the long-lived language server.
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(tag = "cmd", rename_all = "kebab-case")]
pub enum ServerRequest {
    CheckSyntax {
        path: Option<String>,
        source: String,
        offset: usize,
    },
    /// Signature box for the identifier at `pos`, or for `name` in the buffer's language.
    Bluebox {
        path: Option<String>,
        source: String,
        offset: usize,
        pos: Option<usize>,
        name: Option<String>,
    },
    DocsSearch {
        query: String,
        limit: usize,
    },
    /// Exported identifiers of a language module, e.g. `racket` or `lang/htdp-beginner`.
    Completions {
        language: String,
    },
    MacroSteps {
        path: Option<String>,
        source: String,
        offset: usize,
        width: usize,
        /// "standard" hides library macros; "all" shows everything.
        policy: String,
    },
    ModuleGraph {
        path: String,
        /// Unsaved contents of `path`, or empty to read the file.
        source: String,
        include_libs: bool,
    },
    /// Where `name` is defined in the file `path` (for jump-to-definition across files).
    JumpTarget {
        path: String,
        name: String,
    },
}

impl ServerRequest {
    pub fn kind(&self) -> ServerRequestKind {
        match self {
            ServerRequest::CheckSyntax { .. } => ServerRequestKind::CheckSyntax,
            ServerRequest::Bluebox { .. } => ServerRequestKind::Bluebox,
            ServerRequest::DocsSearch { .. } => ServerRequestKind::DocsSearch,
            ServerRequest::Completions { .. } => ServerRequestKind::Completions,
            ServerRequest::MacroSteps { .. } => ServerRequestKind::MacroSteps,
            ServerRequest::ModuleGraph { .. } => ServerRequestKind::ModuleGraph,
            ServerRequest::JumpTarget { .. } => ServerRequestKind::JumpTarget,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ServerRequestKind {
    CheckSyntax,
    Bluebox,
    DocsSearch,
    Completions,
    MacroSteps,
    ModuleGraph,
    JumpTarget,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ArrowKind {
    /// A binding arrow within the program.
    Lexical,
    /// From a `require` (or `#lang`) to an identifier it supplies.
    Imported,
    /// A tail-call arrow.
    Tail,
}

/// Binding arrow: from the binder (`from`) to a use (`to`).
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct Arrow {
    pub from: Range,
    pub to: Range,
    pub kind: ArrowKind,
    pub actual: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct Tooltip {
    pub start: usize,
    pub end: usize,
    pub text: String,
}

/// Semantic coloring: `style` is a DrRacket check-syntax style such as "lexically-bound",
/// "imported", "set!d", "free-variable", or "unused-identifier".
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct ColorRange {
    pub start: usize,
    pub end: usize,
    pub style: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct DocEntry {
    pub start: usize,
    pub end: usize,
    pub label: String,
    pub path: Option<String>,
    pub anchor: Option<String>,
}

impl DocEntry {
    pub fn url(&self) -> Option<String> {
        Some(doc_url(self.path.as_deref()?, self.anchor.as_deref()))
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct Jump {
    pub start: usize,
    pub end: usize,
    /// The file defining the identifier.
    pub path: Option<String>,
    pub id: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct Definition {
    pub start: usize,
    pub end: usize,
    pub name: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct RequireOpen {
    pub start: usize,
    pub end: usize,
    pub path: Option<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct SyntaxError {
    pub message: String,
    pub srclocs: Vec<Srcloc>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
pub struct CheckSyntaxResult {
    pub arrows: Vec<Arrow>,
    pub tooltips: Vec<Tooltip>,
    pub colors: Vec<ColorRange>,
    pub unused_requires: Vec<Range>,
    pub docs: Vec<DocEntry>,
    pub jumps: Vec<Jump>,
    pub definitions: Vec<Definition>,
    pub require_opens: Vec<RequireOpen>,
    /// Read/expansion errors; empty when the program expanded cleanly.
    pub errors: Vec<SyntaxError>,
}

impl CheckSyntaxResult {
    /// All occurrences bound to the same binder as the identifier covering `pos`
    /// (binder first), for highlighting and rename.
    pub fn occurrences(&self, pos: usize) -> Vec<Range> {
        let inside = |r: &Range| r.0 <= pos && pos < r.1;
        let lexical = || self.arrows.iter().filter(|a| a.kind == ArrowKind::Lexical);
        let binder = lexical()
            .find(|a| inside(&a.from) || inside(&a.to))
            .map(|a| a.from);
        let Some(binder) = binder else { return Vec::new() };
        let mut out = vec![binder];
        for a in lexical().filter(|a| a.from == binder) {
            if !out.contains(&a.to) {
                out.push(a.to);
            }
        }
        out
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct DocLink {
    pub path: String,
    pub anchor: Option<String>,
}

impl DocLink {
    pub fn url(&self) -> String {
        doc_url(&self.path, self.anchor.as_deref())
    }
}

/// DrRacket's "blue box": the signature of a documented binding.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct Bluebox {
    pub lines: Vec<String>,
    /// "procedure", "syntax", ...
    pub kind: String,
    pub doc: Option<DocLink>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct DocsHit {
    pub name: String,
    pub libs: Vec<String>,
    pub kind: String,
    pub path: Option<String>,
    pub anchor: Option<String>,
}

impl DocsHit {
    pub fn url(&self) -> Option<String> {
        Some(doc_url(self.path.as_deref()?, self.anchor.as_deref()))
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct MacroStep {
    /// The reduction kind ("macro", "error", ...).
    #[serde(rename = "type")]
    pub step_type: String,
    /// Human-readable description, e.g. "Macro transformation".
    pub name: String,
    pub before: String,
    pub after: String,
    pub foci_before: Vec<Range>,
    pub foci_after: Vec<Range>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct GraphNode {
    pub path: Option<String>,
    pub label: String,
    /// Installed library module (collection or package) rather than project code.
    pub lib: bool,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
pub struct ModuleGraph {
    pub nodes: Vec<GraphNode>,
    /// `(importer, imported)` indices into `nodes`.
    pub edges: Vec<(usize, usize)>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct JumpTarget {
    /// Position in the target file's visible text (after any DrRacket header).
    pub pos: usize,
    /// Length in chars of the hidden DrRacket header in that file (0 if none).
    pub header_length: usize,
    /// 1-based line in the visible text.
    pub line: usize,
}

/// A decoded server response.
#[derive(Clone, Debug, PartialEq)]
pub enum ServerResponse {
    CheckSyntax(CheckSyntaxResult),
    Bluebox(Option<Bluebox>),
    DocsSearch(Vec<DocsHit>),
    Completions(Vec<String>),
    MacroSteps(Vec<MacroStep>),
    ModuleGraph(ModuleGraph),
    JumpTarget(Option<JumpTarget>),
}

impl ServerResponse {
    pub(crate) fn decode(kind: ServerRequestKind, v: serde_json::Value) -> Result<Self, String> {
        use serde_json::from_value as from;
        let r = match kind {
            ServerRequestKind::CheckSyntax => from(v).map(ServerResponse::CheckSyntax),
            ServerRequestKind::Bluebox => from(v).map(ServerResponse::Bluebox),
            ServerRequestKind::DocsSearch => from(v).map(ServerResponse::DocsSearch),
            ServerRequestKind::Completions => from(v).map(ServerResponse::Completions),
            ServerRequestKind::MacroSteps => from(v).map(ServerResponse::MacroSteps),
            ServerRequestKind::ModuleGraph => from(v).map(ServerResponse::ModuleGraph),
            ServerRequestKind::JumpTarget => from(v).map(ServerResponse::JumpTarget),
        };
        r.map_err(|e| format!("malformed {kind:?} response: {e}"))
    }
}

/// `file://` URL for a documentation page and anchor.
pub fn doc_url(path: &str, anchor: Option<&str>) -> String {
    let mut url = String::from("file://");
    for c in path.chars() {
        match c {
            ' ' => url.push_str("%20"),
            '#' => url.push_str("%23"),
            '%' => url.push_str("%25"),
            '?' => url.push_str("%3F"),
            c => url.push(c),
        }
    }
    if let Some(a) = anchor {
        url.push('#');
        for c in a.chars() {
            match c {
                ' ' => url.push_str("%20"),
                '"' => url.push_str("%22"),
                '%' => url.push_str("%25"),
                '<' => url.push_str("%3C"),
                '>' => url.push_str("%3E"),
                '`' => url.push_str("%60"),
                '#' => url.push_str("%23"),
                c => url.push(c),
            }
        }
    }
    url
}
