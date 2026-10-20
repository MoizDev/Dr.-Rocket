//! End-to-end tests against a real Racket installation. They are skipped (pass trivially)
//! when Racket can't be found.

use super::lang::{self, Language};
use super::protocol::*;
use super::runner::{RunConfig, RunSession};
use super::*;
use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

const TIMEOUT: Duration = Duration::from_secs(60);

fn backend() -> Option<&'static Backend> {
    static BACKEND: OnceLock<Option<Backend>> = OnceLock::new();
    BACKEND
        .get_or_init(|| {
            let install = discover(std::env::var("RACKETEER_RACKET").ok().as_deref())?;
            let dir = std::env::temp_dir().join(format!("racketeer-tests-{:016x}", super::scripts_hash(&install)));
            let b = Backend::start_in(install, dir, None).expect("install scripts");
            assert!(b.wait_compiled(Duration::from_secs(300)), "raco make timed out");
            assert_eq!(b.status(), BackendStatus::Ready, "backend failed to compile");
            Some(b)
        })
        .as_ref()
}

macro_rules! need_racket {
    () => {
        match backend() {
            Some(b) => b,
            None => {
                eprintln!("racket not found; skipping");
                return;
            }
        }
    };
}

fn scratch_dir() -> PathBuf {
    let d = std::env::temp_dir().join("racketeer-test-files");
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// Receive run events until `done` matches one (inclusive). Panics on timeout.
fn run_until(session: &RunSession, mut done: impl FnMut(&RunEvent) -> bool) -> Vec<RunEvent> {
    let deadline = Instant::now() + TIMEOUT;
    let mut seen = Vec::new();
    loop {
        let left = deadline.checked_duration_since(Instant::now()).unwrap_or_else(|| panic!("timed out; got {seen:#?}"));
        match session.events().recv_timeout(left) {
            Ok(ev) => {
                let stop = done(&ev);
                if let RunEvent::Terminated { stderr, .. } = &ev
                    && !stop
                {
                    panic!("runner terminated: {stderr}\nevents: {seen:#?}");
                }
                seen.push(ev);
                if stop {
                    return seen;
                }
            }
            Err(_) => panic!("timed out; got {seen:#?}"),
        }
    }
}

fn ready(id: Option<u64>) -> impl FnMut(&RunEvent) -> bool {
    move |e| matches!(e, RunEvent::Ready { id: got } if *got == id)
}

fn values(events: &[RunEvent]) -> Vec<String> {
    events
        .iter()
        .filter_map(|e| match e {
            RunEvent::Value { text, .. } => Some(text.clone()),
            _ => None,
        })
        .collect()
}

fn module_config(path: Option<PathBuf>, source: &str, instrument: Instrument) -> RunConfig {
    RunConfig {
        path,
        source: source.to_string(),
        offset: 0,
        language: RunLanguage::Module,
        options: RunOptions { instrument, ..RunOptions::default() },
    }
}

#[test]
fn runs_racket_program_with_output_values_and_error_location() {
    let b = need_racket!();
    let path = scratch_dir().join("hello.rkt");
    let src = "#lang racket\n(displayln \"hello\")\n(list 1 2 \"s\")\n(car 5)\n";
    let mut s = RunSession::start(b, &module_config(Some(path.clone()), src, Instrument::Errortrace)).unwrap();
    let events = run_until(&s, ready(None));
    assert!(events.iter().any(|e| matches!(e, RunEvent::Output { stream: Stream::Out, text, .. } if text == "hello\n")));
    assert_eq!(values(&events), vec!["'(1 2 \"s\")"]);
    let err = events.iter().find_map(|e| if let RunEvent::Error(r) = e { Some(r) } else { None }).expect("error");
    assert!(err.message.starts_with("car: contract violation"), "{}", err.message);
    assert_eq!(err.kind, ErrorKind::Runtime);
    let loc = &err.srclocs[0];
    assert_eq!(loc.path.as_deref(), Some(path.to_str().unwrap()));
    assert_eq!(loc.line, Some(4));
    assert_eq!(&src[loc.pos.unwrap()..loc.pos.unwrap() + loc.span.unwrap()], "(car 5)");

    // REPL after the run, in the module's namespace.
    let id = s.eval("(+ 1 2) (string-append \"a\" \"b\")");
    let events = run_until(&s, ready(Some(id)));
    assert_eq!(values(&events), vec!["3", "\"ab\""]);
}

#[test]
fn teaching_language_prints_tests_and_errors_like_drracket() {
    let b = need_racket!();
    let path = scratch_dir().join("bsl-test.rkt");
    let body = "(define (f x) (* x 2))\n(check-expect (f 2) 4)\n(check-expect (f 2) 5)\n(list 1 2)\n(/ 1 3)\ntrue\n";
    let settings = Language::Beginner.default_settings();
    let (source, offset) = lang::program_text(Language::Beginner, &settings, Some(&path), body);
    let config = RunConfig {
        path: Some(path),
        source,
        offset,
        language: Language::Beginner.run_language(),
        options: RunOptions::default(),
    };
    let mut s = RunSession::start(b, &config).unwrap();
    let events = run_until(&s, ready(None));
    assert_eq!(values(&events), vec!["(cons 1 (cons 2 '()))", "0.3\u{305}", "#true"]);
    let tests = events.iter().find_map(|e| if let RunEvent::Tests(t) = e { Some(t) } else { None }).expect("tests");
    assert_eq!((tests.passed, tests.total), (1, 2));
    assert_eq!(tests.failures.len(), 1);
    let failure = &tests.failures[0];
    assert!(failure.message.contains("Actual value 4 differs from 5"), "{}", failure.message);
    assert_eq!(failure.srcloc.line, Some(3), "lines are relative to the visible buffer");
    let pos = failure.srcloc.pos.unwrap();
    assert!(body[pos..].starts_with("(check-expect (f 2) 5)"));

    let id = s.eval("(f 1 2)");
    let events = run_until(&s, ready(Some(id)));
    let err = events.iter().find_map(|e| if let RunEvent::Error(r) = e { Some(r) } else { None }).expect("error");
    assert_eq!(err.message, "f: expects only 1 argument, but found 2");

    let id = s.eval("(check-expect (f 1) 3)");
    let events = run_until(&s, ready(Some(id)));
    let tests = events.iter().find_map(|e| if let RunEvent::Tests(t) = e { Some(t) } else { None }).expect("tests rerun");
    assert_eq!(tests.total, 3);
}

#[test]
fn image_values_arrive_as_png_segments() {
    let b = need_racket!();
    let src = "#lang racket\n(require 2htdp/image)\n(circle 5 \"solid\" \"red\")\n(list 1 (square 3 \"solid\" \"blue\"))\n";
    let s = RunSession::start(b, &module_config(None, src, Instrument::None)).unwrap();
    let events = run_until(&s, ready(None));
    let segs: Vec<&Vec<Segment>> = events
        .iter()
        .filter_map(|e| if let RunEvent::Value { segments, .. } = e { Some(segments) } else { None })
        .collect();
    assert_eq!(segs.len(), 2);
    match &segs[0][..] {
        [Segment::Image(img)] => {
            assert_eq!((img.width, img.height), (10, 10));
            assert_eq!(&img.decode_png().unwrap()[1..4], b"PNG");
        }
        other => panic!("expected one image, got {other:?}"),
    }
    assert!(segs[1].iter().any(|s| matches!(s, Segment::Image(i) if i.width == 3)));
    assert!(segs[1].iter().any(|s| matches!(s, Segment::Text { text } if text.starts_with("(list 1 "))));
}

#[test]
fn input_is_requested_and_delivered() {
    let b = need_racket!();
    let mut s = RunSession::start(b, &module_config(None, "#lang racket\n", Instrument::None)).unwrap();
    run_until(&s, ready(None));
    let id = s.eval("(read-line)");
    run_until(&s, |e| matches!(e, RunEvent::InputWanted));
    s.send_input("typed line\n");
    let events = run_until(&s, ready(Some(id)));
    assert_eq!(values(&events), vec!["\"typed line\""]);
}

#[test]
fn break_interrupts_a_loop() {
    let b = need_racket!();
    let mut s = RunSession::start(b, &module_config(None, "#lang racket\n(define (spin) (spin))\n", Instrument::None)).unwrap();
    run_until(&s, ready(None));
    let id = s.eval("(spin)");
    std::thread::sleep(Duration::from_millis(300));
    s.interrupt();
    let events = run_until(&s, ready(Some(id)));
    assert!(events.iter().any(|e| matches!(e, RunEvent::Error(r) if r.kind == ErrorKind::Break)));
    let id = s.eval("(+ 2 2)");
    assert_eq!(values(&run_until(&s, ready(Some(id)))), vec!["4"]);
}

#[test]
fn coverage_reports_unexecuted_code() {
    let b = need_racket!();
    let src = "#lang racket\n(define (used x) (+ x 1))\n(define (unused x) (* x 2))\n(used 1)\n";
    let s = RunSession::start(b, &module_config(None, src, Instrument::Coverage)).unwrap();
    let events = run_until(&s, ready(None));
    let uncovered = events
        .iter()
        .find_map(|e| if let RunEvent::Coverage { uncovered, .. } = e { Some(uncovered.clone()) } else { None })
        .expect("coverage");
    let texts: Vec<&str> = uncovered.iter().map(|(p, n)| &src[*p..p + n]).collect();
    assert_eq!(texts, vec!["(* x 2)"]);
}

#[test]
fn debugger_pauses_at_breakpoints_with_locals() {
    let b = need_racket!();
    let src = "#lang racket\n(define (f x)\n  (+ x 1))\n(f 5)\n";
    let bp = src.find("(+ x 1)").unwrap();
    let mut config = module_config(None, src, Instrument::Debug);
    config.options.breakpoints = vec![bp];
    let mut s = RunSession::start(b, &config).unwrap();
    let events = run_until(&s, |e| matches!(e, RunEvent::Paused(_)));
    let RunEvent::Paused(p) = events.last().unwrap() else { unreachable!() };
    assert_eq!(p.srcloc.pos, Some(bp));
    assert!(p.locals.iter().any(|l| l.name == "x" && l.value == "5"), "{:?}", p.locals);
    assert_eq!(p.stack[0].name, "(+ x 1)");
    s.debug(DebugAction::Continue);
    let events = run_until(&s, ready(None));
    assert_eq!(values(&events), vec!["6"]);
}

#[test]
fn debugger_shows_only_the_users_variables_and_frames_in_teaching_languages() {
    let b = need_racket!();
    let path = scratch_dir().join("bsl-debug.rkt");
    let body = "(define (pow b e acc)\n  (cond [(zero? e) acc]\n        [else (pow b (sub1 e) (* acc b))]))\n(check-expect (pow 2 3 1) 8)\n(pow 3 2 1)\n";
    let bp = body.find("(pow b (sub1").unwrap();
    let settings = Language::Beginner.default_settings();
    let (source, offset) = lang::program_text(Language::Beginner, &settings, Some(&path), body);
    let config = RunConfig {
        path: Some(path),
        source,
        offset,
        language: Language::Beginner.run_language(),
        options: RunOptions { instrument: Instrument::Debug, breakpoints: vec![bp], ..RunOptions::default() },
    };
    let s = RunSession::start(b, &config).unwrap();
    let events = run_until(&s, |e| matches!(e, RunEvent::Paused(_)));
    let RunEvent::Paused(p) = events.last().unwrap() else { unreachable!() };
    let names: Vec<&str> = p.locals.iter().map(|l| l.name.as_str()).collect();
    // The language's own definitions (`repl?`, the check-expect's `test123`) stay hidden.
    assert_eq!(names, vec!["b", "e", "acc", "pow"], "{:?}", p.locals);
    // So does the frame of its printing wrapper around `(pow 3 2 1)`.
    let frames: Vec<&str> = p.stack.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(frames, vec!["(pow b (sub1 e) (* acc b))"]);
}

#[test]
fn profiler_reports_user_functions() {
    let b = need_racket!();
    let src = "#lang racket\n(define (fib n) (if (< n 2) n (+ (fib (- n 1)) (fib (- n 2)))))\n(void (fib 27))\n";
    let s = RunSession::start(b, &module_config(None, src, Instrument::Profile)).unwrap();
    let events = run_until(&s, ready(None));
    let report = events.iter().find_map(|e| if let RunEvent::Profile(r) = e { Some(r) } else { None }).expect("profile");
    assert!(report.nodes.iter().any(|n| n.name == "fib"), "{:?}", report.nodes.iter().map(|n| &n.name).collect::<Vec<_>>());
}

#[test]
fn r5rs_toplevel_language() {
    let b = need_racket!();
    let config = RunConfig {
        path: None,
        source: "(define x 5)\n(* x 2)\n".into(),
        offset: 0,
        language: Language::R5rs.run_language(),
        options: RunOptions { instrument: Instrument::None, ..RunOptions::default() },
    };
    let s = RunSession::start(b, &config).unwrap();
    assert_eq!(values(&run_until(&s, ready(None))), vec!["10"]);
}

fn step_until(session: &StepperSession) -> Vec<StepperEvent> {
    let deadline = Instant::now() + TIMEOUT;
    let mut seen = Vec::new();
    loop {
        let left = deadline.checked_duration_since(Instant::now()).unwrap_or_else(|| panic!("timed out; got {seen:#?}"));
        let ev = session.events().recv_timeout(left).unwrap_or_else(|_| panic!("timed out; got {seen:#?}"));
        let done = matches!(ev, StepperEvent::Finished { .. } | StepperEvent::Terminated { .. });
        seen.push(ev);
        if done {
            return seen;
        }
    }
}

#[test]
fn stepper_produces_highlighted_reductions() {
    let b = need_racket!();
    let source = "(define (f x) (* x 2))\n(f (+ 1 2))\n".to_string();
    let settings = Language::Beginner.default_settings();
    let request = StepRequest {
        path: None,
        source: source.clone(),
        level: Language::Beginner.stepper_level(&source).unwrap().into(),
        settings: settings.stepper_json(),
        teachpacks: vec![],
        width: 60,
        images: Default::default(),
    };
    let s = StepperSession::start(b, &request).unwrap();
    let events = step_until(&s);
    let steps: Vec<&Step> = events.iter().filter_map(|e| if let StepperEvent::Step(s) = e { Some(s) } else { None }).collect();
    assert_eq!(steps.len(), 3, "{events:#?}");
    let first = steps[0];
    let before = first.before.last().unwrap();
    assert_eq!(before.text, "(f (+ 1 2))");
    let (a, z) = before.highlights[0];
    assert_eq!(before.text.chars().skip(a).take(z - a).collect::<String>(), "(+ 1 2)");
    assert_eq!(first.after.last().unwrap().text, "(f 3)");
    let src = first.before_src.unwrap();
    assert_eq!(&source[src.pos..src.pos + src.span], "(+ 1 2)");
    assert_eq!(steps[1].kind, "user-application");
    assert_eq!(steps[2].after.last().unwrap().text, "6");
    assert!(matches!(events.last(), Some(StepperEvent::Finished { count: 3 })));
}

#[test]
fn stepper_handles_hash_lang_teaching_buffers() {
    let b = need_racket!();
    let source = "#lang htdp/bsl\n(+ 1 (* 2 3))\n".to_string();
    let level = Language::Module.stepper_level(&source).expect("htdp/bsl is steppable");
    let request = StepRequest {
        path: None,
        source,
        level: level.into(),
        settings: Language::Beginner.default_settings().stepper_json(),
        teachpacks: vec![],
        width: 60,
        images: Default::default(),
    };
    let events = step_until(&StepperSession::start(b, &request).unwrap());
    let texts: Vec<String> = events
        .iter()
        .filter_map(|e| if let StepperEvent::Step(s) = e { Some(s.after.last().unwrap().text.clone()) } else { None })
        .collect();
    assert_eq!(texts, vec!["(+ 1 6)", "7"]);
}

/// Matches DrRacket 9.3's stepper step for step (checked against its stepper window): cons
/// chains are shown being built up as `list` values, even in Beginning Student.
#[test]
fn stepper_matches_drracket_on_list_construction() {
    let b = need_racket!();
    let source = "(define (is-list? lst)\n  (cond\n    [(empty? lst) true]\n    [(cons? lst) true]\n    [else false]))\n\n(is-list? (cons 'a (cons 'b empty)))\n".to_string();
    let request = StepRequest {
        path: None,
        source: source.clone(),
        level: Language::Beginner.stepper_level(&source).unwrap().into(),
        settings: Language::Beginner.default_settings().stepper_json(),
        teachpacks: vec![],
        width: 200,
        images: Default::default(),
    };
    let events = step_until(&StepperSession::start(b, &request).unwrap());
    let after: Vec<String> = events
        .iter()
        .filter_map(|e| if let StepperEvent::Step(s) = e { Some(s.after.last().unwrap().text.split_whitespace().collect::<Vec<_>>().join(" ")) } else { None })
        .collect();
    assert_eq!(
        after,
        vec![
            "(is-list? (cons 'a (cons 'b '())))",
            "(is-list? (cons 'a (list 'b)))",
            "(is-list? (list 'a 'b))",
            "(cond ((empty? (list 'a 'b)) #true) ((cons? (list 'a 'b)) #true) (else #false))",
            "(cond (#false #true) ((cons? (list 'a 'b)) #true) (else #false))",
            "(cond ((cons? (list 'a 'b)) #true) (else #false))",
            "(cond (#true #true) (else #false))",
            "#true",
        ]
    );
}

fn server_result(server: &mut Server, req: ServerRequest) -> ServerResponse {
    let id = server.request(req);
    server.wait(id, TIMEOUT).expect("server timed out").expect("server error")
}

#[test]
fn server_check_syntax_bluebox_docs_and_completions() {
    let b = need_racket!();
    let mut server = Server::start(b).unwrap();
    let path = scratch_dir().join("cs.rkt");
    let src = "#lang racket\n(define (f x) (map add1 x))\n(define unused 3)\n(f (list 1 2))\n";
    let ServerResponse::CheckSyntax(cs) = server_result(
        &mut server,
        ServerRequest::CheckSyntax { path: Some(path.to_string_lossy().into()), source: src.into(), offset: 0, images: Default::default() },
    ) else {
        panic!()
    };
    assert!(cs.errors.is_empty(), "{:?}", cs.errors);
    let x_binder = src.find("x)").unwrap();
    let x_use = src.find("x))").unwrap();
    assert!(cs.arrows.iter().any(|a| a.kind == ArrowKind::Lexical && a.from == (x_binder, x_binder + 1) && a.to == (x_use, x_use + 1)));
    assert_eq!(cs.occurrences(x_use), vec![(x_binder, x_binder + 1), (x_use, x_use + 1)]);
    let map_pos = src.find("map").unwrap();
    let doc = cs.docs.iter().find(|d| d.start == map_pos).expect("docs for map");
    assert!(doc.url().unwrap().contains("pairs.html#"), "{doc:?}");
    let unused = src.find("unused").unwrap();
    assert!(cs.colors.iter().any(|c| c.start == unused && c.style == "unused-identifier"));
    assert!(cs.tooltips.iter().any(|t| t.start == map_pos && t.text.contains("imported from racket")));
    // Semantic coloring: a call to `f` (defined here) is lexically bound; `map` is imported.
    let f_call = src.rfind("(f ").unwrap() + 1;
    assert!(cs.colors.iter().any(|c| c.start == f_call && c.style == "lexically-bound"), "{:?}", cs.colors);
    assert!(cs.colors.iter().any(|c| c.start == map_pos && c.style == "imported"), "{:?}", cs.colors);

    let ServerResponse::Bluebox(Some(bb)) = server_result(
        &mut server,
        ServerRequest::Bluebox {
            path: Some(path.to_string_lossy().into()),
            source: src.into(),
            offset: 0,
            pos: Some(map_pos + 1),
            name: None,
        },
    ) else {
        panic!("no bluebox")
    };
    assert!(bb.lines[0].starts_with("(map proc lst"), "{bb:?}");

    let ServerResponse::DocsSearch(hits) = server_result(&mut server, ServerRequest::DocsSearch { query: "string-append".into(), limit: 5 })
    else {
        panic!()
    };
    assert_eq!(hits[0].name, "string-append");
    assert!(hits[0].libs.iter().any(|l| l == "racket/base"));

    let ServerResponse::Completions(words) =
        server_result(&mut server, ServerRequest::Completions { language: Language::Beginner.language_module().into() })
    else {
        panic!()
    };
    assert!(words.iter().any(|w| w == "check-expect"));
    assert!(!words.iter().any(|w| w == "set!"), "BSL has no set!");
}

#[test]
fn server_reports_syntax_errors_and_macro_steps() {
    let b = need_racket!();
    let mut server = Server::start(b).unwrap();
    let src = "#lang racket\n(define (f x) (+ x y))\n";
    let ServerResponse::CheckSyntax(cs) =
        server_result(&mut server, ServerRequest::CheckSyntax { path: None, source: src.into(), offset: 0, images: Default::default() })
    else {
        panic!()
    };
    assert_eq!(cs.errors.len(), 1);
    assert_eq!(cs.errors[0].message, "y: unbound identifier");
    assert_eq!(cs.errors[0].srclocs[0].pos, src.find("y)"));

    let src = "#lang racket\n(define-syntax-rule (swap! a b) (let ([t a]) (set! a b) (set! b t)))\n(define x 1) (define y 2)\n(swap! x y)\n";
    let ServerResponse::MacroSteps(steps) = server_result(
        &mut server,
        ServerRequest::MacroSteps { path: None, source: src.into(), offset: 0, width: 80, policy: "standard".into(), images: Default::default() },
    ) else {
        panic!()
    };
    let swap = steps.iter().find(|s| s.step_type == "macro").expect("a macro step");
    let (a, z) = swap.foci_before[0];
    assert_eq!(swap.before.chars().skip(a).take(z - a).collect::<String>(), "(swap! x y)");
    assert!(swap.after.contains("(let ((t x)) (set! x y) (set! y t))"));
}

#[test]
fn server_module_graph_and_jump_target() {
    let b = need_racket!();
    let dir = scratch_dir().join("graph");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("util.rkt"), "#lang racket/base\n(provide helper)\n\n(define (helper x)\n  (* x 2))\n").unwrap();
    std::fs::write(dir.join("main.rkt"), "#lang racket\n(require \"util.rkt\")\n(helper 3)\n").unwrap();
    let mut server = Server::start(b).unwrap();
    let ServerResponse::ModuleGraph(g) = server_result(
        &mut server,
        ServerRequest::ModuleGraph { path: dir.join("main.rkt").to_string_lossy().into(), source: String::new(), include_libs: false },
    ) else {
        panic!()
    };
    let labels: Vec<&str> = g.nodes.iter().map(|n| n.label.as_str()).collect();
    assert_eq!(labels, vec!["main.rkt", "util.rkt"]);
    assert_eq!(g.edges, vec![(0, 1)]);

    let ServerResponse::JumpTarget(Some(t)) = server_result(
        &mut server,
        ServerRequest::JumpTarget { path: dir.join("util.rkt").to_string_lossy().into(), name: "helper".into() },
    ) else {
        panic!()
    };
    assert_eq!(t.line, 4);
}

#[test]
fn headers_round_trip_for_every_teaching_language() {
    for language in Language::ALL {
        let mut settings = language.default_settings();
        let header = lang::header(language, "my-program", &settings);
        assert_eq!(header.is_some(), language.is_teaching(), "{language:?}");
        let Some(header) = header else { continue };
        let parsed = lang::parse_header(&format!("{header}(+ 1 2)\n")).expect("parses");
        assert_eq!(parsed.language, language);
        assert_eq!(parsed.settings, settings);
        assert_eq!(parsed.modname.as_deref(), Some("my-program"));
        assert_eq!(parsed.len_chars, header.chars().count());

        settings.teachpacks = vec![lang::TEACHPACKS[0].1.to_string()];
        settings.printing_style = lang::PrintingStyle::Write;
        let file = lang::save(language, &settings, Some(std::path::Path::new("/x/a b.rkt")), "(define x 1)\n");
        let loaded = lang::load(&file);
        assert_eq!(loaded.language, language);
        assert_eq!(loaded.settings, settings);
        assert_eq!(loaded.body, "(define x 1)\n");
        assert_eq!(loaded.header + &loaded.body, file);
    }
}

#[test]
fn headers_match_racket_printer_byte_for_byte() {
    let Some(install) = discover(std::env::var("RACKETEER_RACKET").ok().as_deref()) else { return };
    let mut settings = Language::Intermediate.default_settings();
    settings.teachpacks = vec![lang::TEACHPACKS[0].1.to_string(), lang::TEACHPACKS[1].1.to_string()];
    let ours = lang::header(Language::Intermediate, "two words", &settings).unwrap();
    let expr = r##"(printf "#reader~s~s\n" '(lib "htdp-intermediate-reader.ss" "lang") '((modname |two words|) (read-case-sensitive #t) (teachpacks ((lib "image.rkt" "teachpack" "2htdp") (lib "universe.rkt" "teachpack" "2htdp"))) (htdp-settings #(#t constructor repeating-decimal #f #t none #f ((lib "image.rkt" "teachpack" "2htdp") (lib "universe.rkt" "teachpack" "2htdp")) #f))))"##;
    let out = std::process::Command::new(&install.racket).args(["-l", "racket/base", "-e", expr]).output().unwrap();
    let racket_line = String::from_utf8(out.stdout).unwrap();
    assert_eq!(ours.lines().nth(2).unwrap(), racket_line.trim_end());

    let sdp = lang::header(Language::SdpAdvanced, "prog", &Language::SdpAdvanced.default_settings()).unwrap();
    let expr = r##"(printf "#reader~s~s\n" '(lib "advanced-reader.rkt" "deinprogramm" "sdp") '((modname prog) (read-case-sensitive #f) (teachpacks ()) (deinprogramm-settings #(#f write repeating-decimal #f #t none datum #f ()))))"##;
    let out = std::process::Command::new(&install.racket).args(["-l", "racket/base", "-e", expr]).output().unwrap();
    assert_eq!(sdp.lines().nth(2).unwrap(), String::from_utf8(out.stdout).unwrap().trim_end());
}

#[test]
fn parses_drracket_written_file() {
    // Exactly what DrRacket writes for a BSL file with the old true/false setting on.
    let file = ";; The first three lines of this file were inserted by DrRacket. They record metadata\n;; about the language level of this file in a form that our tools can easily process.\n#reader(lib \"htdp-beginner-reader.ss\" \"lang\")((modname bsl) (read-case-sensitive #t) (teachpacks ()) (htdp-settings #(#t constructor repeating-decimal #f #t none #f () #t)))\n(define x 1)\n";
    let loaded = lang::load(file);
    assert_eq!(loaded.language, Language::Beginner);
    assert!(loaded.settings.true_false_empty_as_ids);
    assert_eq!(loaded.body, "(define x 1)\n");
    assert_eq!(lang::save(loaded.language, &loaded.settings, Some(std::path::Path::new("bsl.rkt")), &loaded.body), file);
}

#[test]
fn detects_hash_lang_lines() {
    assert_eq!(lang::hash_lang("#lang racket/base\n(x)").as_deref(), Some("racket/base"));
    assert_eq!(lang::hash_lang(";; comment\n#| block |#\n  #lang htdp/bsl\n").as_deref(), Some("htdp/bsl"));
    assert_eq!(lang::hash_lang("#!racket\n").as_deref(), Some("racket"));
    assert_eq!(lang::hash_lang("(define x 1)"), None);
    assert_eq!(Language::Module.stepper_level("#lang htdp/isl+\n"), Some("module"));
    assert_eq!(Language::Module.stepper_level("#lang htdp/asl\n"), None);
    assert_eq!(Language::Module.stepper_level("#lang racket\n"), None);
    assert_eq!(Language::Module.completions_language("#lang at-exp racket/base\n"), "racket/base");
    assert_eq!(Language::Module.completions_language("#lang htdp/bsl\n"), "lang/htdp-beginner");
    for l in Language::ALL {
        assert_eq!(Language::from_id(l.id()), Some(l));
    }
    let grouped: usize = lang::GROUPS.iter().map(|g| g.languages.len()).sum();
    assert_eq!(grouped, Language::ALL.len());
}

#[test]
fn discovers_racket() {
    if let Some(install) = discover(None) {
        assert!(install.racket.is_file());
        assert!(install.version().is_some());
    }
    assert!(discover(Some("/definitely/not/here/racket")).is_none());
}

#[test]
fn heavy_output_is_coalesced() {
    let b = need_racket!();
    let src = "#lang racket/base\n(for ([i 100000]) (displayln i))\n";
    let s = RunSession::start(b, &module_config(None, src, Instrument::None)).unwrap();
    let events = run_until(&s, ready(None));
    let outputs: Vec<&str> = events
        .iter()
        .filter_map(|e| if let RunEvent::Output { text, .. } = e { Some(text.as_str()) } else { None })
        .collect();
    let all: String = outputs.concat();
    assert_eq!(all.lines().count(), 100_000);
    assert!(all.ends_with("99999\n"));
    assert!(outputs.len() < 200, "{} output events", outputs.len());
}

#[test]
fn exit_reports_code() {
    let b = need_racket!();
    let s = RunSession::start(b, &module_config(None, "#lang racket/base\n(display \"bye\")\n(exit 3)\n", Instrument::None)).unwrap();
    let events = run_until(&s, |e| matches!(e, RunEvent::Exit { .. }));
    assert!(events.iter().any(|e| matches!(e, RunEvent::Output { text, .. } if text == "bye")));
    assert!(matches!(events.last(), Some(RunEvent::Exit { code: 3 })));
}

#[test]
fn memory_limit_pauses_until_told_to_keep_going() {
    let b = need_racket!();
    // Holds on to about 100 MB, a little at a time, then finishes.
    let src = "#lang racket/base\n(define v (for/list ([i 100]) (sleep 0.005) (make-bytes (* 1024 1024) 1)))\n(display (length v))\n";
    let mut config = module_config(None, src, Instrument::None);
    config.options.memory_limit_mb = Some(64);
    let mut s = RunSession::start(b, &config).unwrap();
    let events = run_until(&s, |e| matches!(e, RunEvent::MemoryLimit { .. } | RunEvent::Ready { .. } | RunEvent::Terminated { .. }));
    assert!(matches!(events.last(), Some(RunEvent::MemoryLimit { limit: 64, mb }) if *mb > 64), "{events:#?}");
    assert!(!events.iter().any(|e| matches!(e, RunEvent::Output { .. })), "ran on while paused: {events:#?}");
    s.memory_continue(Some(512));
    let events = run_until(&s, |e| matches!(e, RunEvent::Ready { .. } | RunEvent::Terminated { .. }));
    assert!(events.iter().any(|e| matches!(e, RunEvent::Output { text, .. } if text == "100")), "{events:#?}");
    assert!(!events.iter().any(|e| matches!(e, RunEvent::MemoryLimit { .. } | RunEvent::Error(_))), "{events:#?}");
}

#[test]
fn newer_check_syntax_cancels_older_for_same_file() {
    let b = need_racket!();
    let mut server = Server::start(b).unwrap();
    let req = |n: u32| ServerRequest::CheckSyntax {
        path: Some("/tmp/racketeer-supersede.rkt".into()),
        source: format!("#lang racket\n(define x {n})\n"),
        offset: 0,
        images: Default::default(),
    };
    let first = server.request(req(1));
    let second = server.request(req(2));
    let r1 = server.wait(first, TIMEOUT).unwrap();
    let r2 = server.wait(second, TIMEOUT).unwrap();
    assert!(r2.is_ok(), "{r2:?}");
    // The first either finished before the second arrived, or was cancelled.
    assert!(r1.is_ok() || r1 == Err("cancelled".into()), "{r1:?}");

    let slow = server.request(ServerRequest::DocsSearch { query: "a".into(), limit: 5 });
    server.cancel(slow);
    let r = server.wait(slow, TIMEOUT).unwrap();
    assert!(r.is_ok() || r == Err("cancelled".into()));
    // The server still works afterwards.
    let ServerResponse::Completions(words) = server_result(&mut server, ServerRequest::Completions { language: "racket/base".into() }) else {
        panic!()
    };
    assert!(words.iter().any(|w| w == "lambda"));
}

/// `cargo test latency -- --ignored --nocapture` prints how long common operations take.
#[test]
#[ignore]
fn latency_report() {
    let b = need_racket!();
    for (name, src, instrument) in [
        ("racket/base, no instrumentation", "#lang racket/base\n(+ 1 2)\n", Instrument::None),
        ("racket, errortrace", "#lang racket\n(+ 1 2)\n", Instrument::Errortrace),
    ] {
        let t = Instant::now();
        let s = RunSession::start(b, &module_config(None, src, instrument)).unwrap();
        run_until(&s, ready(None));
        eprintln!("run {name}: {:?}", t.elapsed());
    }
    let settings = Language::Beginner.default_settings();
    let (source, offset) = lang::program_text(Language::Beginner, &settings, None, "(+ 1 2)\n");
    let t = Instant::now();
    let config = RunConfig { path: None, source, offset, language: RunLanguage::Module, options: RunOptions::default() };
    let s = RunSession::start(b, &config).unwrap();
    run_until(&s, ready(None));
    eprintln!("run BSL: {:?}", t.elapsed());

    let idle = super::runner::IdleRunner::spawn(b).unwrap();
    std::thread::sleep(Duration::from_millis(800));
    let t = Instant::now();
    let s = RunSession::start_with(idle, &module_config(None, "#lang racket\n(+ 1 2)\n", Instrument::Errortrace)).unwrap();
    run_until(&s, ready(None));
    eprintln!("run racket, errortrace, prewarmed: {:?}", t.elapsed());

    let mut server = Server::start(b).unwrap();
    for i in 0..3 {
        let t = Instant::now();
        let source = format!("#lang racket\n(define x {i})\n");
        server_result(&mut server, ServerRequest::CheckSyntax { path: None, source, offset: 0, images: Default::default() });
        eprintln!("check-syntax #{i}: {:?}", t.elapsed());
    }
}

#[test]
fn teaching_language_tracing_reports_calls_by_depth() {
    let b = need_racket!();
    let body = "(define (len l) (cond [(empty? l) 0] [else (+ 1 (len (rest l)))]))\n(len (list 7 8))\n";
    let mut settings = Language::Beginner.default_settings();
    settings.tracing = true;
    let (source, offset) = lang::program_text(Language::Beginner, &settings, None, body);
    let config = RunConfig {
        path: None,
        source,
        offset,
        language: Language::Beginner.run_language(),
        options: RunOptions { trace: true, ..RunOptions::default() },
    };
    let s = RunSession::start(b, &config).unwrap();
    let events = run_until(&s, ready(None));
    let trace: String = events
        .iter()
        .filter_map(|e| match e {
            RunEvent::Output { stream: Stream::Trace, text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect();
    let lines: Vec<&str> = trace.lines().collect();
    assert!(lines.iter().any(|l| l.trim_start().starts_with("(len")), "trace: {trace:?}");
    // Recursive calls are indented deeper than the outermost call.
    let depths: Vec<usize> = lines.iter().filter(|l| l.trim_start().starts_with("(len")).map(|l| l.len() - l.trim_start().len()).collect();
    assert!(depths.windows(2).any(|w| w[1] > w[0]), "expected nested calls: {trace:?}");
    assert_eq!(values(&events), vec!["2"]);
}

#[test]
fn debugger_stops_in_required_modules_open_in_other_tabs() {
    let b = need_racket!();
    let dir = scratch_dir().join("multi-debug");
    std::fs::create_dir_all(&dir).unwrap();
    let helper_path = dir.join("helper.rkt");
    // The file on disk differs from the "editor" text: the debugger must use the editor text.
    std::fs::write(&helper_path, "#lang racket\n(provide double)\n(define (double y) 0)\n").unwrap();
    let helper_src = "#lang racket\n(provide double)\n(define (double y)\n  (* y 2))\n";
    let main_path = dir.join("main.rkt");
    let main_src = "#lang racket\n(require \"helper.rkt\")\n(double 21)\n";
    std::fs::write(&main_path, main_src).unwrap();
    let bp = helper_src.find("(* y 2)").unwrap();
    let mut config = module_config(Some(main_path), main_src, Instrument::Debug);
    config.options.debug_modules = vec![DebugModule {
        path: helper_path.to_string_lossy().into_owned(),
        source: helper_src.into(),
        offset: 0,
        breakpoints: vec![bp],
    }];
    let mut s = RunSession::start(b, &config).unwrap();
    let events = run_until(&s, |e| matches!(e, RunEvent::Paused(_)));
    let RunEvent::Paused(p) = events.last().unwrap() else { unreachable!() };
    let paused_in = std::path::Path::new(p.srcloc.path.as_deref().expect("path")).canonicalize().unwrap();
    assert_eq!(paused_in, helper_path.canonicalize().unwrap());
    assert_eq!(p.srcloc.pos, Some(bp));
    assert_eq!(p.srcloc.line, Some(4));
    assert!(p.locals.iter().any(|l| l.name == "y" && l.value == "21"), "{:?}", p.locals);
    s.debug(DebugAction::Continue);
    let events = run_until(&s, ready(None));
    assert_eq!(values(&events), vec!["42"], "the editor text of helper.rkt was used");
}

#[test]
fn visualizer_records_calls_questions_returns_and_heap_objects() {
    let b = need_racket!();
    let src = "#lang htdp/bsl\n(define-struct node (left key right))\n(define (insert t k)\n  (cond\n    [(empty? t) (make-node empty k empty)]\n    [(< k (node-key t)) (make-node (insert (node-left t) k) (node-key t) (node-right t))]\n    [else (make-node (node-left t) (node-key t) (insert (node-right t) k))]))\n(define t1 (insert (insert empty 5) 3))\n";
    let s = RunSession::start(b, &module_config(None, src, Instrument::Visualize)).unwrap();
    let events = run_until(&s, ready(None));
    let steps: Vec<&VizStep> = events.iter().filter_map(|e| if let RunEvent::VizStep(s) = e { Some(&**s) } else { None }).collect();
    let kinds: Vec<&str> = steps.iter().map(|s| s.kind.as_str()).collect();
    assert_eq!(kinds.first(), Some(&"define"), "{kinds:?}");
    assert_eq!(kinds.last(), Some(&"end"), "{kinds:?}");
    // The inner (insert empty 5) runs first: a call from its call site, the question, the return.
    let call = steps.iter().find(|s| s.kind == "call").unwrap();
    let site = &src[call.pos.unwrap()..call.pos.unwrap() + call.span];
    assert_eq!(site, "(insert empty 5)");
    assert_eq!(call.live_frames().count(), 1);
    assert_eq!(call.frames[0].name, "insert");
    assert_eq!(call.frames[0].vars[1], ("k".to_string(), VizRef::Prim { v: "5".into(), k: "num".into() }));
    let question = steps.iter().find(|s| s.kind == "question").unwrap();
    assert_eq!(&src[question.pos.unwrap()..question.pos.unwrap() + question.span], "(empty? t)");
    assert_eq!(question.value, Some(VizRef::Prim { v: "#true".into(), k: "bool".into() }));
    // Recursion: the second insert recurses into the left subtree (two frames deep).
    assert!(steps.iter().any(|s| s.kind == "call" && s.live_frames().count() == 2));
    // Nodes are heap objects with field names from define-struct; t1 is a global.
    let node = steps.iter().flat_map(|s| s.objects.iter()).find(|o| o.k == "struct").unwrap();
    assert_eq!(node.name.as_deref(), Some("node"));
    let names: Vec<&str> = node.fields.iter().map(|f| f.0.as_str()).collect();
    assert_eq!(names, ["left", "key", "right"]);
    let end = steps.last().unwrap();
    assert!(end.globals.iter().any(|(n, v)| n == "t1" && matches!(v, VizRef::Object { .. })), "{:?}", end.globals);
    assert!(!end.globals.iter().any(|(n, _)| n == "make-node"), "generated names stay hidden");
    assert!(events.iter().any(|e| matches!(e, RunEvent::VizEnd { truncated: false, .. })));
}

/// The Choose Language output settings apply to a run, as in DrRacket.
#[test]
fn run_applies_choose_language_output_settings() {
    let b = need_racket!();
    let src = "#lang htdp/bsl\n(list empty false (/ 1 3))\n";
    let run = |settings: lang::TeachingSettings| {
        let mut config = module_config(None, src, Instrument::Errortrace);
        config.options.teaching = Some(settings.run_json(Language::Beginner));
        let s = RunSession::start(b, &config).unwrap();
        values(&run_until(&s, ready(None)))
    };
    let mut settings = Language::Beginner.default_settings();
    assert_eq!(run(settings.clone()), vec!["(cons '() (cons #false (cons 0.3\u{305} '())))"]);
    settings.true_false_empty_as_ids = true;
    assert_eq!(run(settings.clone()), vec!["(cons empty (cons false (cons 0.3\u{305} empty)))"]);
    settings.printing_style = lang::PrintingStyle::Write;
    settings.fraction_style = lang::FractionStyle::MixedFraction;
    assert_eq!(run(settings), vec!["(() #false 1/3)"]);
}

/// DrRacket's "true/false/empty as ids" option: the visualizer writes values the same way.
#[test]
fn visualizer_follows_true_false_empty_as_ids() {
    let b = need_racket!();
    let src = "#lang htdp/bsl\n(define-struct node (left key right))\n(define (leaf? t) (empty? (node-left t)))\n(define t (make-node empty 5 false))\n(leaf? t)\n";
    for (as_ids, t, f, e) in [(false, "#true", "#false", "'()"), (true, "true", "false", "empty")] {
        let mut config = module_config(None, src, Instrument::Visualize);
        let mut settings = Language::Beginner.default_settings();
        settings.true_false_empty_as_ids = as_ids;
        config.options.teaching = Some(settings.run_json(Language::Beginner));
        let s = RunSession::start(b, &config).unwrap();
        let events = run_until(&s, ready(None));
        assert!(events.iter().any(|ev| matches!(ev, RunEvent::VizStart { as_ids: a, .. } if *a == as_ids)));
        let steps: Vec<&VizStep> = events.iter().filter_map(|e| if let RunEvent::VizStep(s) = e { Some(&**s) } else { None }).collect();
        let result = steps.iter().find(|s| s.kind == "result").unwrap();
        assert_eq!(result.value, Some(VizRef::Prim { v: t.into(), k: "bool".into() }), "as_ids {as_ids}");
        let node = steps.iter().flat_map(|s| s.objects.iter()).find(|o| o.k == "struct").unwrap();
        assert_eq!(node.fields[0].1, VizRef::Prim { v: e.into(), k: "empty".into() });
        assert_eq!(node.fields[2].1, VizRef::Prim { v: f.into(), k: "bool".into() });
    }
}

/// Tail calls replace the caller's frame, as Racket runs them: an accumulator loop shows
/// one frame, while non-tail recursion stacks up.
#[test]
fn visualizer_tail_calls_do_not_grow_the_stack() {
    let b = need_racket!();
    let src = "#lang htdp/bsl\n(define (sum-acc l acc)\n  (cond\n    [(empty? l) acc]\n    [else (sum-acc (rest l) (+ acc (first l)))]))\n(define (sum l)\n  (cond\n    [(empty? l) 0]\n    [else (+ (first l) (sum (rest l)))]))\n(define a (sum-acc (list 1 2 3) 0))\n(define b (sum (list 1 2 3)))\n";
    let s = RunSession::start(b, &module_config(None, src, Instrument::Visualize)).unwrap();
    let events = run_until(&s, ready(None));
    let steps: Vec<&VizStep> = events.iter().filter_map(|e| if let RunEvent::VizStep(s) = e { Some(&**s) } else { None }).collect();
    let deepest = |name: &str| steps.iter().filter(|s| s.live_frames().last().is_some_and(|f| f.name == name)).map(|s| s.live_frames().count()).max().unwrap();
    assert_eq!(deepest("sum-acc"), 1, "tail-recursive loop keeps one frame");
    assert_eq!(deepest("sum"), 4, "(sum (rest l)) is not a tail call");
    // The loop's result comes back once, from the frame that computed it.
    assert_eq!(steps.iter().filter(|s| s.kind == "return" && s.name.as_deref() == Some("sum-acc")).count(), 1);
}

/// Files with pasted images: written in DrRacket's editor format, read back to the same text
/// and image, and run with the image as a real value.
#[test]
fn wxme_files_round_trip_and_run_with_images() {
    let b = need_racket!();
    let mut server = Server::start(b).unwrap();
    let dir = scratch_dir().join("wxme");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("pic.rkt");
    // A 3x2 red PNG.
    let png = {
        let img = image::RgbaImage::from_pixel(3, 2, image::Rgba([255, 0, 0, 255]));
        let mut out = Vec::new();
        img.write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png).unwrap();
        out
    };
    let ch = char::from_u32(0xF0000).unwrap();
    let text = format!("#lang htdp/bsl\n(require 2htdp/image)\n(define pic {ch})\n(check-expect (image-width pic) 3)\n(image-height pic)\n");
    let mut images = SourceImages::new();
    images.insert("983040".into(), base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &png));
    let w = server_result(&mut server, ServerRequest::WxmeWrite { path: path.to_string_lossy().into(), text: text.clone(), images: images.clone() });
    assert_eq!(w, ServerResponse::WxmeWrite);
    let bytes = std::fs::read(&path).unwrap();
    assert!(bytes.starts_with(b"#reader(lib\"read.ss\"\"wxme\")WXME"), "saved in DrRacket's format");

    let ServerResponse::WxmeRead(f) = server_result(&mut server, ServerRequest::WxmeRead { path: path.to_string_lossy().into() }) else { panic!() };
    assert_eq!(f.text, text);
    assert_eq!(f.images.len(), 1);
    assert!(f.unsupported.is_empty());

    let mut config = module_config(Some(path), &f.text, Instrument::None);
    config.options.images = f.images;
    let s = RunSession::start(b, &config).unwrap();
    let events = run_until(&s, ready(None));
    assert_eq!(values(&events), vec!["2"], "{events:?}");
    let tests = events.iter().find_map(|e| if let RunEvent::Tests(t) = e { Some(t) } else { None }).unwrap();
    assert_eq!((tests.passed, tests.total), (1, 1));
}

/// Image values in steps arrive as placeholder characters with their PNGs.
#[test]
fn stepper_shows_images() {
    let b = need_racket!();
    let source = "(require 2htdp/image)\n(define c (circle 10 \"solid\" \"red\"))\n(beside c c)\n".to_string();
    let request = StepRequest {
        path: None,
        source: source.clone(),
        level: Language::Beginner.stepper_level(&source).unwrap().into(),
        settings: Language::Beginner.default_settings().stepper_json(),
        teachpacks: vec![],
        width: 60,
        images: Default::default(),
    };
    let events = step_until(&StepperSession::start(b, &request).unwrap());
    let last = events.iter().filter_map(|e| if let StepperEvent::Step(s) = e { Some(s) } else { None }).next_back().unwrap();
    let result = last.after.last().unwrap().text.clone();
    let c = result.chars().next().unwrap();
    assert!(crate::editor::images::is_image_char(c), "{result:?}");
    let png = last.images.get(&(c as u32).to_string()).expect("the image's PNG");
    let img = crate::editor::images::EmbeddedImage::from_b64(png).unwrap();
    assert_eq!((img.width, img.height), (40, 20), "two 20px circles side by side");
}

/// DrRacket's "Show Log": warnings the program logs reach the IDE; Racket's chatter does not.
#[test]
fn runner_forwards_logged_warnings() {
    let b = need_racket!();
    let src = "#lang racket\n(log-warning \"careful\")\n(define-logger app)\n(log-app-error \"bad ~a\" 42)\n(collect-garbage)\n";
    let s = RunSession::start(b, &module_config(None, src, Instrument::None)).unwrap();
    let events = run_until(&s, ready(None));
    let logs: Vec<(String, Option<String>, String)> = events
        .iter()
        .filter_map(|e| if let RunEvent::Log { level, topic, message } = e { Some((level.clone(), topic.clone(), message.clone())) } else { None })
        .collect();
    assert_eq!(logs, vec![("warning".into(), None, "careful".into()), ("error".into(), Some("app".into()), "app: bad 42".into())]);
}

/// The visualizer also records Scheme at the R5RS language level (top-level forms, mutable
/// pairs as lists).
#[test]
fn visualizer_records_r5rs_programs() {
    let b = need_racket!();
    let src = "(define (rev l acc)\n  (if (null? l)\n      acc\n      (rev (cdr l) (cons (car l) acc))))\n(define r (rev (list 1 2 3) '()))\n";
    let mut config = module_config(None, src, Instrument::Visualize);
    config.language = RunLanguage::Toplevel { init: "r5rs".into() };
    let s = RunSession::start(b, &config).unwrap();
    let events = run_until(&s, ready(None));
    let steps: Vec<&VizStep> = events.iter().filter_map(|e| if let RunEvent::VizStep(s) = e { Some(&**s) } else { None }).collect();
    assert!(steps.iter().filter(|s| s.kind == "call").count() >= 4, "every rev call is recorded");
    assert_eq!(steps.iter().map(|s| s.live_frames().count()).max(), Some(1), "rev's tail calls keep one frame");
    // The replaced frames are still sent (for context), marked as replaced.
    assert!(steps.iter().any(|s| s.frames.iter().any(|f| f.replaced)));
    // The result is a list of cons cells: 3, 2, 1.
    let end = steps.last().unwrap();
    let r = end.globals.iter().find(|(n, _)| n == "r").map(|(_, v)| v.clone()).unwrap();
    let objects: std::collections::HashMap<u64, &VizObject> = steps.iter().flat_map(|s| s.objects.iter()).map(|o| (o.id, o)).collect();
    let mut items = Vec::new();
    let mut cur = r;
    while let VizRef::Object { id } = cur {
        let cell = objects[&id];
        assert_eq!(cell.k, "cons");
        if let Some(VizRef::Prim { v, .. }) = &cell.first {
            items.push(v.clone());
        }
        cur = cell.rest.clone().unwrap();
    }
    assert_eq!(items, ["3", "2", "1"]);
}

/// Every call on the stack shows what it will do with the answer it's waiting for, with what
/// it already worked out as values: `(cons 37 •••)`, `(* 3 •••)`, `(+ 1 •••)` once the first
/// `fib` came back, code still to run left as code, and the top level's own expression.
#[test]
fn visualizer_waiting_calls_show_what_they_do_next() {
    let b = need_racket!();
    let src = "#lang htdp/isl+\n\
(define (rand-list n) (cond [(zero? n) empty] [else (cons (random 100) (rand-list (sub1 n)))]))\n\
(define (fact n) (cond [(zero? n) 1] [else (* n (fact (sub1 n)))]))\n\
(define (fib n) (cond [(< n 2) n] [else (+ (fib (- n 1)) (fib (- n 2)))]))\n\
(define (twice l) (cons (* 2 (first l)) (twice2 (rest l)) ))\n\
(define (twice2 l) (cond [(empty? l) empty] [else (twice l)]))\n\
(rand-list 2)\n\
(define x (+ 1 (fact 2)))\n\
(fib 2)\n\
(twice2 (list 5 6))\n\
(map (lambda (v) (fact v)) (list 1))\n";
    let s = RunSession::start(b, &module_config(None, src, Instrument::Visualize)).unwrap();
    let events = run_until(&s, ready(None));
    let mut trace = crate::visualizer::Trace::default();
    trace.set_source(src);
    for e in &events {
        if let RunEvent::VizStep(s) = e {
            trace.push((**s).clone());
        }
    }
    // Everything each step says is waiting, as (frame, what it does next).
    let mut seen: Vec<(String, String)> = Vec::new();
    for step in &trace.steps {
        let mut add = |who: String, w: Option<Vec<crate::visualizer::Seg>>| {
            if let Some(w) = w {
                let pair = (who, crate::visualizer::seg_text(&w));
                if !seen.contains(&pair) {
                    seen.push(pair);
                }
            }
        };
        add("top".into(), crate::visualizer::waiting(&trace, step, None));
        for (k, f) in step.frames.iter().enumerate() {
            add(format!("{} {}", f.name, f.vars.first().map(|v| crate::visualizer::label(&trace, &v.1, step.index)).unwrap_or_default()), crate::visualizer::waiting(&trace, step, Some(k)));
        }
    }
    let has = |who: &str, what: &str| seen.iter().any(|(w, t)| w == who && t == what);
    let random = seen.iter().find(|(w, _)| w == "rand-list 2").map(|(_, t)| t.clone()).unwrap_or_default();
    assert!(regex_like_cons_number(&random), "rand-list 2 waits with its random number in place: {seen:?}");
    assert!(has("top", "(+ 1 •••)"), "the top level's own expression: {seen:?}");
    assert!(has("fact 2", "(* 2 •••)"), "{seen:?}");
    assert!(has("fib 2", "(+ ••• (fib (- n 2)))"), "the second call hasn't run: still code: {seen:?}");
    assert!(has("fib 2", "(+ 1 •••)"), "the first call's answer fills in: {seen:?}");
    assert!(has("twice (list 5 6)", "(cons 10 •••)"), "{seen:?}");
    // Nothing waits where the answer is passed straight on: a top-level call, a `cond`
    // answer, a function given to `map` (its answers go to map, not to (map …)).
    assert!(!seen.iter().any(|(_, t)| t == "•••" || t.contains("map")), "{seen:?}");
    assert!(!seen.iter().any(|(w, _)| w.starts_with("twice2")), "{seen:?}");
}

fn regex_like_cons_number(s: &str) -> bool {
    s.strip_prefix("(cons ").and_then(|r| r.strip_suffix(" •••)")).is_some_and(|n| n.parse::<u32>().is_ok_and(|n| n < 100))
}

/// A function passed in as an argument can be called from Intermediate Student on, but not
/// in Beginning Student, where the background check flags the call.
#[test]
fn calling_a_function_argument_is_allowed_from_intermediate_on() {
    let b = need_racket!();
    let mut server = Server::start(b).unwrap();
    let body = "(define (my-map f l acc)\n  (cond\n    [(empty? l) (reverse acc)]\n    [else (my-map f (rest l) (cons (f (first l)) acc))]))\n\n(my-map sqr (list 1 2 3) empty)\n";
    let check = |server: &mut Server, language: Language| {
        let (source, offset) = lang::program_text(language, &Default::default(), None, body);
        let ServerResponse::CheckSyntax(cs) = server_result(server, ServerRequest::CheckSyntax { path: None, source, offset, images: Default::default() }) else { panic!() };
        cs.errors.iter().map(|e| e.message.clone()).collect::<Vec<_>>()
    };
    assert_eq!(check(&mut server, Language::IntermediateLambda), Vec::<String>::new());
    assert_eq!(check(&mut server, Language::Intermediate), Vec::<String>::new());
    for language in [Language::Beginner, Language::BeginnerAbbr] {
        let errors = check(&mut server, language);
        assert!(errors.iter().any(|m| m.contains("expected a function after the open parenthesis, but found a variable")), "{errors:?}");
    }
}

/// What DrRacket 9.3 writes for a new file in each HtDP language with its default settings
/// (`default-settings` in htdp-lib/lang/htdp-langs.rkt), byte for byte: course tools such as
/// Marmoset read these files.
#[test]
fn default_headers_are_what_drracket_writes() {
    // Only Advanced shows sharing by default.
    for (language, reader, sharing) in [
        (Language::Beginner, "htdp-beginner-reader.ss", "#f"),
        (Language::BeginnerAbbr, "htdp-beginner-abbr-reader.ss", "#f"),
        (Language::Intermediate, "htdp-intermediate-reader.ss", "#f"),
        (Language::IntermediateLambda, "htdp-intermediate-lambda-reader.ss", "#f"),
        (Language::Advanced, "htdp-advanced-reader.ss", "#t"),
    ] {
        let expected = format!(
            ";; The first three lines of this file were inserted by DrRacket. They record metadata\n\
             ;; about the language level of this file in a form that our tools can easily process.\n\
             #reader(lib \"{reader}\" \"lang\")((modname a01q1) (read-case-sensitive #t) (teachpacks ()) (htdp-settings #(#t constructor repeating-decimal {sharing} #t none #f () #f)))\n"
        );
        assert_eq!(lang::save(language, &language.default_settings(), Some(std::path::Path::new("/x/a01q1.rkt")), ""), expected, "{language:?}");
    }
}

/// A saved teaching-language file is a module that plain Racket loads, and whose definitions
/// a grader can reach (teaching modules export nothing; graders look inside the module).
#[test]
fn saved_teaching_files_load_in_plain_racket_like_a_grader() {
    let Some(install) = discover(std::env::var("RACKETEER_RACKET").ok().as_deref()) else { return };
    let dir = std::env::temp_dir().join(format!("drrocket-grader-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("a01q1.rkt");
    let body = "(define (double n) (* 2 n))\n(check-expect (double 4) 8)\n";
    std::fs::write(&path, lang::save(Language::Beginner, &Language::Beginner.default_settings(), Some(&path), body)).unwrap();
    let expr = format!(
        "(define m '(file {:?})) (dynamic-require m #f) (parameterize ([current-namespace (module->namespace m)]) (write (eval '(double 21))))",
        path.to_string_lossy()
    );
    let out = std::process::Command::new(&install.racket).args(["-e", &expr]).output().unwrap();
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(String::from_utf8_lossy(&out.stdout), "42", "{}", String::from_utf8_lossy(&out.stderr));
}

/// A function made inside another (`make-adder`'s `lambda`) is shown when it's called, with
/// what it uses from around it (`n`), though ISL+ drops the `lambda`'s source location.
#[test]
fn visualizer_shows_closures_with_what_they_capture() {
    let b = need_racket!();
    let src = "#lang htdp/isl+\n(define (make-adder n)\n  (lambda (x) (+ x n)))\n(define add5 (make-adder 5))\n(add5 10)\n(define (count-up k)\n  (local [(define start 1)\n          (define (go i) (if (> i k) empty (cons i (go (+ i start)))))]\n    (go 1)))\n(count-up 1)\n";
    let s = RunSession::start(b, &module_config(None, src, Instrument::Visualize)).unwrap();
    let events = run_until(&s, ready(None));
    let steps: Vec<&VizStep> = events.iter().filter_map(|e| if let RunEvent::VizStep(s) = e { Some(&**s) } else { None }).collect();
    let num = |v: &str| VizRef::Prim { v: v.into(), k: "num".into() };
    let call = steps.iter().find(|s| s.kind == "call" && s.pos.is_some_and(|p| src[p..].starts_with("(add5 10)"))).expect("the closure's call is a step");
    let f = call.frames.last().unwrap();
    assert_eq!(f.name, "λ");
    assert_eq!(f.nargs, 1, "only x is an argument");
    assert_eq!(f.vars, vec![("x".to_string(), num("10")), ("n".to_string(), num("5"))]);
    // A local function shows the locals it uses (k, start), but not itself.
    let go = steps.iter().filter(|s| s.kind == "call").filter_map(|s| s.frames.last()).find(|f| f.name == "go").expect("go is called");
    let names: Vec<&str> = go.vars.iter().map(|v| v.0.as_str()).collect();
    assert_eq!(names[0], "i");
    assert!(names.contains(&"k") && names.contains(&"start") && !names.contains(&"go"), "{names:?}");
}

#[test]
fn stepper_can_disable_test_execution() {
    let b = need_racket!();
    let source = "#lang htdp/bsl\n(define (f x) (+ x 1))\n(check-expect (f 2) 3)\n(check-within (f 3) 4 0.1)\n(f 5)\n";
    for include in [true, false] {
        let mut settings = Language::Beginner.default_settings().stepper_json();
        settings["include_tests"] = include.into();
        let request = StepRequest {
            path: None, source: source.into(), level: "module".into(), settings,
            teachpacks: vec![], width: 60, images: Default::default(),
        };
        let events = step_until(&StepperSession::start(b, &request).unwrap());
        let steps: Vec<_> = events.iter().filter_map(|e| if let StepperEvent::Step(s) = e { Some(s) } else { None }).collect();
        assert!(!events.iter().any(|e| matches!(e, StepperEvent::Error { .. } | StepperEvent::StepError { .. })), "{events:#?}");
        assert!(steps.iter().any(|s| s.after.iter().any(|e| e.text == "6")));
        for check in ["check-expect", "check-within"] {
            assert_eq!(steps.iter().any(|s| s.before.iter().chain(&s.after).any(|e| e.text.contains(check))), include, "{events:#?}");
        }
        if !include { assert_eq!(steps.len(), 2); }
    }
}

#[test]
fn functional_stepper_focus_hides_recursive_context_without_skipping_steps() {
    let b = need_racket!();
    let source = "(define (fact n) (if (zero? n) 1 (* n (fact (sub1 n)))))\n(fact 3)\n";
    let mut traces = Vec::new();
    for focus in [false, true] {
        let mut settings = Language::Beginner.default_settings().stepper_json();
        settings["functional_focus"] = focus.into();
        let request = StepRequest {
            path: None, source: source.into(), level: "beginner".into(), settings,
            teachpacks: vec![], width: 200, images: Default::default(),
        };
        let events = step_until(&StepperSession::start(b, &request).unwrap());
        assert!(!events.iter().any(|e| matches!(e, StepperEvent::Error { .. } | StepperEvent::StepError { .. })), "{events:#?}");
        let steps: Vec<Step> = events.into_iter().filter_map(|e| if let StepperEvent::Step(s) = e { Some(s) } else { None }).collect();
        assert_eq!(steps.last().unwrap().after.last().unwrap().text, "6");
        for step in &steps {
            for e in step.before.iter().chain(&step.after) {
                assert!(e.highlights.iter().all(|&(a, z)| a < z && z <= e.text.chars().count()));
            }
        }
        traces.push(steps);
    }
    assert_eq!(traces[0].len(), traces[1].len());
    let k = traces[1].iter().position(|s| s.kind == "user-application" && s.before.iter().any(|e| e.text == "(fact 1)")).expect("innermost recursive call");
    assert!(traces[0][k].before.iter().any(|e| e.text.contains("(* 3 (* 2 (fact 1)))")), "{:?}", traces[0][k]);
    assert_eq!(traces[1][k].before.len(), 1);
    assert_eq!(traces[0][k].before_src, traces[1][k].before_src);
}
