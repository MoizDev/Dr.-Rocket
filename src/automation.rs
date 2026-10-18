//! Scripted end-to-end automation: drives the real app with injected keyboard/mouse
//! events and captures screenshots. Used for testing the UI as a user experiences it.
//!
//! `DRROCKET_AUTOMATION=script.json drrocket` where the script is
//! `{"steps": [{"open": "file.rkt"}, {"type": "(+ 1 2)"}, {"key": "cmd+r"},
//!   {"wait_ms": 500}, {"screenshot": "out.png"}, {"quit": true}]}`.

use crate::app::App;
use crate::commands::{COMMANDS, Command};
use egui::{Event, Key, Modifiers, PointerButton, pos2};
use serde::Deserialize;
use std::path::PathBuf;
use std::io::Write;
use std::process::{Command as Process, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{SyncSender, sync_channel};
use std::thread::JoinHandle;

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "snake_case")]
pub enum Step {
    Open(PathBuf),
    OpenFolder(PathBuf),
    NewFile(bool),
    Type(String),
    Key(String),
    Command(String),
    /// `["Racket", "Run"]`: invoke a native menu item.
    Menu([String; 2]),
    Click([f32; 2]),
    DoubleClick([f32; 2]),
    RightClick([f32; 2]),
    Hover([f32; 2]),
    /// Press or release the primary button at a point, each in its own frame (`click` does
    /// both in one).
    /// Show this step of the visualizer in the active tab.
    #[serde(rename = "show_viz_step")]
    VizGoto(usize),
    /// Start (true) or pause the visualizer's playback.
    VizPlay(bool),
    /// Width of the right-hand panel, in points.
    RightPanelWidth(f32),
    MouseDown([f32; 2]),
    MouseUp([f32; 2]),
    /// Press at the first point, move to the second over several frames, release.
    Drag([[f32; 2]; 2]),
    Scroll([f32; 2]),
    WaitMs(u64),
    WaitFrames(u64),
    /// Wait until a named condition holds: "racket-ready", "run-finished", "stepper-ready".
    WaitUntil(String),
    Screenshot(PathBuf),
    /// `{"record": {"path": "out.mp4", "seconds": 20}}`: film the window for that long and
    /// encode it with ffmpeg (on PATH) at 30 fps. The app's clock runs one frame per video
    /// frame while filming, so the video is smooth however long each capture takes.
    Record { path: PathBuf, seconds: f64 },
    Theme(String),
    /// Switch the active tab's language, as the Choose Language dialog does (by id:
    /// "beginner", "intermediate-lambda"...).
    Language(String),
    /// Print the active tab's error squiggles (`automation: diagnostics [...]`).
    LogDiagnostics(bool),
    Log(String),
    Quit(bool),
}

#[derive(Deserialize, Debug)]
pub struct Script {
    pub steps: Vec<Step>,
    #[serde(default)]
    pub window: Option<[f32; 2]>,
}

pub struct Automation {
    script: Script,
    index: usize,
    wait_until_time: Option<f64>,
    wait_frames: u64,
    shot_done: Option<Arc<AtomicBool>>,
    started: Option<f64>,
    reset_modifiers: bool,
    /// In-progress drag: (from, to, frame).
    drag: Option<(egui::Pos2, egui::Pos2, u32)>,
    recording: Option<Recording>,
}

/// An in-progress `record` step: every frame's screenshot goes to an encoder thread.
struct Recording {
    /// The app's time when filming started; frame `k` is shown at `start + k / RECORD_FPS`.
    start: f64,
    frame: u64,
    frames_total: u64,
    /// Dropped when filming ends; the encoder finishes once in-flight screenshots land.
    frames: Option<SyncSender<(f64, Arc<egui::ColorImage>)>>,
    encoder: JoinHandle<()>,
}

const RECORD_FPS: f64 = 30.0;

/// Encode timestamped frames as a constant-rate video: each output frame shows the latest
/// capture at its time, so the video plays at the speed the app ran.
fn encode(path: PathBuf, frames: std::sync::mpsc::Receiver<(f64, Arc<egui::ColorImage>)>) {
    let mut ffmpeg: Option<std::process::Child> = None;
    let mut last: Option<Arc<egui::ColorImage>> = None;
    let mut written = 0u64;
    let write = |ffmpeg: &mut Option<std::process::Child>, img: &egui::ColorImage| {
        let child = ffmpeg.get_or_insert_with(|| {
            let [w, h] = img.size;
            Process::new("ffmpeg")
                .args(["-loglevel", "error", "-y", "-f", "rawvideo", "-pix_fmt", "rgba", "-s", &format!("{w}x{h}"), "-r", "30", "-i", "-"])
                .args(["-c:v", "libx264", "-preset", "slow", "-crf", "18", "-pix_fmt", "yuv420p", "-movflags", "+faststart"])
                .arg(&path)
                .stdin(Stdio::piped())
                .spawn()
                .expect("ffmpeg")
        });
        let _ = child.stdin.as_mut().unwrap().write_all(img.as_raw());
    };
    for (t, img) in frames {
        let due = (t * RECORD_FPS) as u64;
        if let Some(prev) = &last {
            while written < due {
                write(&mut ffmpeg, prev);
                written += 1;
            }
        } else {
            written = due;
        }
        last = Some(img);
    }
    if let Some(prev) = &last {
        write(&mut ffmpeg, prev);
    }
    if let Some(mut child) = ffmpeg {
        drop(child.stdin.take());
        let _ = child.wait();
        eprintln!("automation: recorded {} ({written} frames)", path.display());
    }
}

impl Automation {
    pub fn from_env() -> Option<Self> {
        let path = std::env::var("DRROCKET_AUTOMATION").ok()?;
        let text = std::fs::read_to_string(&path).ok()?;
        // A broken script must not leave a normal app running unattended: stop right away.
        let script: Script = match serde_json::from_str(&text) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("automation script error: {e}");
                std::process::exit(2);
            }
        };
        Some(Self { script, index: 0, wait_until_time: None, wait_frames: 3, shot_done: None, started: None, reset_modifiers: false, drag: None, recording: None })
    }

    pub fn window_size() -> Option<[f32; 2]> {
        let path = std::env::var("DRROCKET_AUTOMATION").ok()?;
        let text = std::fs::read_to_string(path).ok()?;
        serde_json::from_str::<Script>(&text).ok()?.window
    }

    /// Advance the script. Called from `raw_input_hook`, so injected events enter egui exactly
    /// like real OS input (pointer state, hover and clicks all update normally).
    pub fn step(&mut self, app: &mut App, ctx: &egui::Context, raw: &mut egui::RawInput) {
        // The window never takes OS focus (see main.rs), so tell egui it has it: widgets only
        // accept keyboard input in a focused viewport.
        raw.focused = true;
        let now = ctx.input(|i| i.time);
        self.started.get_or_insert(now);
        // Advance an in-progress drag one frame at a time, like a real mouse.
        if let Some((from, to, frame)) = self.drag {
            const FRAMES: u32 = 8;
            if frame < FRAMES {
                let t = (frame + 1) as f32 / FRAMES as f32;
                raw.events.push(Event::PointerMoved(from + (to - from) * t));
                self.drag = Some((from, to, frame + 1));
            } else {
                raw.events.push(Event::PointerButton { pos: to, button: PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE });
                self.drag = None;
                self.wait_frames = 2;
            }
            return;
        }
        if self.reset_modifiers {
            raw.events.push(Event::ModifiersChanged(Modifiers::NONE));
            self.reset_modifiers = false;
        }
        if let Some(rec) = &mut self.recording {
            if rec.frame < rec.frames_total
                && let Some(frames) = &rec.frames
            {
                // A virtual clock: this frame is drawn at its video time, not the wall clock,
                // and every frame is captured (waiting for the encoder rather than dropping).
                let t = rec.frame as f64 / RECORD_FPS;
                raw.time = Some(rec.start + t);
                raw.predicted_dt = (1.0 / RECORD_FPS) as f32;
                rec.frame += 1;
                let frames = frames.clone();
                // Stamped mid-frame so the encoder's rounding puts it in exactly one slot.
                let stamp = t + 0.5 / RECORD_FPS;
                ctx.request_screenshot(move |img| {
                    let _ = frames.send((stamp, img));
                });
                ctx.request_repaint();
                return;
            }
            // Keep rendering (so pending screenshot callbacks run and release the channel)
            // until the encoder is done.
            rec.frames = None;
            if !rec.encoder.is_finished() {
                return;
            }
            self.recording = None;
        }
        // Keep frames coming only while the script needs them, so idle stretches (waits, the
        // end of the script) measure the app's own repainting, not the harness's.
        if self.wait_frames > 0 {
            self.wait_frames -= 1;
            ctx.request_repaint();
            return;
        }
        if let Some(t) = self.wait_until_time {
            if now < t {
                ctx.request_repaint_after(std::time::Duration::from_secs_f64(t - now));
                return;
            }
            self.wait_until_time = None;
        }
        if self.index < self.script.steps.len() {
            ctx.request_repaint();
        }
        if let Some(done) = &self.shot_done {
            if !done.load(Ordering::SeqCst) {
                return;
            }
            self.shot_done = None;
        }
        // Global timeout so a broken script never hangs.
        if now - self.started.unwrap_or(now) > 300.0 {
            eprintln!("automation: timeout");
            std::process::exit(2);
        }
        let Some(step) = self.script.steps.get(self.index).cloned() else { return };
        match &step {
            Step::Open(p) => app.open_file(p),
            Step::OpenFolder(p) => app.open_folder(p),
            Step::NewFile(yes) => {
                if *yes {
                    app.new_file();
                }
            }
            Step::Type(t) => {
                // One event per keystroke, like real typing; "\n" presses Enter.
                for ch in t.chars() {
                    if ch == '\n' {
                        raw.events.push(Event::Key { key: Key::Enter, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE });
                    } else {
                        raw.events.push(Event::Text(ch.to_string()));
                    }
                }
            }
            Step::Key(k) => {
                if let Some((key, modifiers)) = parse_key(k) {
                    raw.events.push(Event::ModifiersChanged(modifiers));
                    raw.events.push(Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers });
                    raw.events.push(Event::Key { key, physical_key: None, pressed: false, repeat: false, modifiers });
                    self.reset_modifiers = true;
                } else {
                    eprintln!("automation: unknown key {k}");
                }
            }
            Step::Command(name) => {
                if let Some(c) = COMMANDS.iter().find(|c| format!("{:?}", c.cmd) == *name || c.label == name) {
                    let cmd: Command = c.cmd;
                    app.run_command(cmd);
                } else {
                    eprintln!("automation: unknown command {name}");
                }
            }
            Step::Menu([menu, item]) => {
                if !crate::platform::perform_menu_item(menu, item) {
                    eprintln!("automation: no menu item {menu} / {item}");
                }
            }
            Step::Click(p) | Step::DoubleClick(p) | Step::RightClick(p) => {
                let pos = pos2(p[0], p[1]);
                let button = if matches!(step, Step::RightClick(_)) { PointerButton::Secondary } else { PointerButton::Primary };
                let times = if matches!(step, Step::DoubleClick(_)) { 2 } else { 1 };
                raw.events.push(Event::PointerMoved(pos));
                for _ in 0..times {
                    raw.events.push(Event::PointerButton { pos, button, pressed: true, modifiers: Modifiers::NONE });
                    raw.events.push(Event::PointerButton { pos, button, pressed: false, modifiers: Modifiers::NONE });
                }
            }
            Step::Hover(p) => raw.events.push(Event::PointerMoved(pos2(p[0], p[1]))),
            Step::MouseDown(p) | Step::MouseUp(p) => {
                let pos = pos2(p[0], p[1]);
                raw.events.push(Event::PointerMoved(pos));
                raw.events.push(Event::PointerButton { pos, button: PointerButton::Primary, pressed: matches!(step, Step::MouseDown(_)), modifiers: Modifiers::NONE });
            }
            Step::Drag([a, b]) => {
                let from = pos2(a[0], a[1]);
                raw.events.push(Event::PointerMoved(from));
                raw.events.push(Event::PointerButton { pos: from, button: PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE });
                self.drag = Some((from, pos2(b[0], b[1]), 0));
            }
            Step::Scroll(d) => raw.events.push(Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: egui::vec2(d[0], d[1]),
                phase: egui::TouchPhase::Move,
                source: egui::MouseWheelSource::Wheel,
                modifiers: Modifiers::NONE,
            }),
            Step::WaitMs(ms) => self.wait_until_time = Some(now + *ms as f64 / 1000.0),
            Step::WaitFrames(n) => self.wait_frames = *n,
            Step::WaitUntil(cond) => {
                if !crate::racket_ui::condition(app, cond) {
                    return;
                }
            }
            Step::Screenshot(path) => {
                let done = Arc::new(AtomicBool::new(false));
                let d2 = done.clone();
                let path = path.clone();
                let ctx2 = ctx.clone();
                ctx.request_screenshot(move |img| {
                    let [w, h] = img.size;
                    let mut buf = image::RgbaImage::new(w as u32, h as u32);
                    for (k, px) in img.pixels.iter().enumerate() {
                        let [r, g, b, a] = px.to_srgba_unmultiplied();
                        buf.put_pixel((k % w) as u32, (k / w) as u32, image::Rgba([r, g, b, a]));
                    }
                    if let Some(dir) = path.parent() {
                        let _ = std::fs::create_dir_all(dir);
                    }
                    match buf.save(&path) {
                        Ok(()) => eprintln!("automation: saved {}", path.display()),
                        Err(e) => eprintln!("automation: screenshot failed: {e}"),
                    }
                    d2.store(true, Ordering::SeqCst);
                    ctx2.request_repaint();
                });
                self.shot_done = Some(done);
            }
            Step::Record { path, seconds } => {
                let (frames, rx) = sync_channel(4);
                let path = path.clone();
                let encoder = std::thread::spawn(move || encode(path, rx));
                self.recording = Some(Recording { start: now, frame: 0, frames_total: (seconds * RECORD_FPS).round() as u64, frames: Some(frames), encoder });
            }
            Step::VizGoto(n) => {
                if let Some(v) = app.active_tab_mut().and_then(|t| t.rk.viz.as_mut()) {
                    v.playing = false;
                    v.current = (*n).min(v.trace.steps.len().saturating_sub(1));
                }
            }
            Step::VizPlay(on) => {
                if let Some(v) = app.active_tab_mut().and_then(|t| t.rk.viz.as_mut()) {
                    v.playing = *on;
                }
            }
            Step::RightPanelWidth(w) => app.layout.right.size = *w,
            Step::Theme(name) => app.set_theme(name, false),
            Step::Language(id) => match crate::racket::lang::Language::from_id(id) {
                Some(l) => crate::racket_ui::set_language(app, l, l.default_settings()),
                None => eprintln!("automation: unknown language {id}"),
            },
            Step::LogDiagnostics(on) if *on => {
                let d: Vec<String> = app.tabs.get(app.active).map(|t| t.editor.deco.diagnostics.iter().map(|d| format!("{}..{} {}{}", d.range.start, d.range.end, if t.editor.deco.shown(d) { "" } else { "(held) " }, d.message)).collect()).unwrap_or_default();
                eprintln!("automation: diagnostics {d:?}");
            }
            Step::LogDiagnostics(_) => {}
            Step::Log(msg) => eprintln!("automation: {msg}"),
            Step::Quit(yes) => {
                if *yes {
                    app.quit_confirmed = true;
                    std::process::exit(0);
                }
            }
        }
        self.index += 1;
        self.wait_frames = 2;
    }
}

/// Parse "cmd+shift+r", "Enter", "ctrl+space"...
pub fn parse_key(s: &str) -> Option<(Key, Modifiers)> {
    let mut m = Modifiers::NONE;
    let mut key = None;
    for part in s.split('+') {
        match part.to_ascii_lowercase().as_str() {
            "cmd" | "command" => {
                m.command = true;
                m.mac_cmd = cfg!(target_os = "macos");
            }
            "shift" => m.shift = true,
            "alt" | "option" | "opt" => m.alt = true,
            "ctrl" | "control" => m.ctrl = true,
            other => {
                key = Key::from_name(other).or_else(|| Key::from_name(&capitalize(other))).or(match other {
                    "enter" | "return" => Some(Key::Enter),
                    "esc" => Some(Key::Escape),
                    "up" => Some(Key::ArrowUp),
                    "down" => Some(Key::ArrowDown),
                    "left" => Some(Key::ArrowLeft),
                    "right" => Some(Key::ArrowRight),
                    "." => Some(Key::Period),
                    "\\" => Some(Key::Backslash),
                    "/" => Some(Key::Slash),
                    "`" => Some(Key::Backtick),
                    _ => None,
                });
            }
        }
    }
    key.map(|k| (k, m))
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}
