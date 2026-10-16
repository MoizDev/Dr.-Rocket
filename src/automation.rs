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
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "snake_case")]
pub enum Step {
    Open(PathBuf),
    OpenFolder(PathBuf),
    NewFile(bool),
    Type(String),
    Key(String),
    Command(String),
    Click([f32; 2]),
    DoubleClick([f32; 2]),
    RightClick([f32; 2]),
    Hover([f32; 2]),
    Scroll([f32; 2]),
    WaitMs(u64),
    WaitFrames(u64),
    /// Wait until a named condition holds: "racket-ready", "run-finished", "stepper-ready".
    WaitUntil(String),
    Screenshot(PathBuf),
    Theme(String),
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
}

impl Automation {
    pub fn from_env() -> Option<Self> {
        let path = std::env::var("DRROCKET_AUTOMATION").ok()?;
        let text = std::fs::read_to_string(&path).ok()?;
        let script: Script = serde_json::from_str(&text).map_err(|e| eprintln!("automation script error: {e}")).ok()?;
        Some(Self { script, index: 0, wait_until_time: None, wait_frames: 3, shot_done: None, started: None })
    }

    pub fn window_size() -> Option<[f32; 2]> {
        let path = std::env::var("DRROCKET_AUTOMATION").ok()?;
        let text = std::fs::read_to_string(path).ok()?;
        serde_json::from_str::<Script>(&text).ok()?.window
    }

    pub fn step(&mut self, app: &mut App, ctx: &egui::Context) {
        let now = ctx.input(|i| i.time);
        self.started.get_or_insert(now);
        ctx.request_repaint();
        if self.wait_frames > 0 {
            self.wait_frames -= 1;
            return;
        }
        if let Some(t) = self.wait_until_time {
            if now < t {
                return;
            }
            self.wait_until_time = None;
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
            Step::NewFile(_) => app.new_file(),
            Step::Type(t) => ctx.input_mut(|i| i.events.push(Event::Text(t.clone()))),
            Step::Key(k) => {
                if let Some((key, modifiers)) = parse_key(k) {
                    ctx.input_mut(|i| {
                        i.modifiers = modifiers;
                        i.events.push(Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers });
                        i.events.push(Event::Key { key, physical_key: None, pressed: false, repeat: false, modifiers });
                    });
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
            Step::Click(p) | Step::DoubleClick(p) | Step::RightClick(p) => {
                let pos = pos2(p[0], p[1]);
                let button = if matches!(step, Step::RightClick(_)) { PointerButton::Secondary } else { PointerButton::Primary };
                let times = if matches!(step, Step::DoubleClick(_)) { 2 } else { 1 };
                ctx.input_mut(|i| {
                    i.events.push(Event::PointerMoved(pos));
                    for _ in 0..times {
                        i.events.push(Event::PointerButton { pos, button, pressed: true, modifiers: Modifiers::NONE });
                        i.events.push(Event::PointerButton { pos, button, pressed: false, modifiers: Modifiers::NONE });
                    }
                });
            }
            Step::Hover(p) => ctx.input_mut(|i| i.events.push(Event::PointerMoved(pos2(p[0], p[1])))),
            Step::Scroll(d) => ctx.input_mut(|i| {
                i.events.push(Event::MouseWheel { unit: egui::MouseWheelUnit::Point, delta: egui::vec2(d[0], d[1]), phase: egui::TouchPhase::Move, source: egui::MouseWheelSource::Wheel, modifiers: Modifiers::NONE })
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
            Step::Theme(name) => app.set_theme(name, false),
            Step::Log(msg) => eprintln!("automation: {msg}"),
            Step::Quit(_) => {
                app.quit_confirmed = true;
                std::process::exit(0);
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
