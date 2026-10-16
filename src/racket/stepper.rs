//! The algebraic stepper: a stepper process streaming reduction steps for a program.

use super::protocol::*;
use super::transport::Process;
use super::Backend;
use crossbeam_channel::{Receiver, unbounded};
use std::path::PathBuf;

/// What to step.
#[derive(Clone, Debug)]
pub struct StepRequest {
    pub path: Option<PathBuf>,
    /// The visible buffer text (without any DrRacket header).
    pub source: String,
    /// A stepper level id from [`super::lang::Language::stepper_level`]: "beginner",
    /// "beginner-abbr", "intermediate", "intermediate-lambda", "advanced",
    /// "sdp-beginner", "sdp-vanilla", or "module" for `#lang htdp/...` buffers.
    pub level: String,
    /// Printing settings, as produced by [`super::lang::TeachingSettings::stepper_json`].
    pub settings: serde_json::Value,
    /// Teachpack require specs as s-expression text, e.g. `(lib "image.rkt" "teachpack" "2htdp")`.
    pub teachpacks: Vec<String>,
    /// Pretty-printing width in characters.
    pub width: usize,
}

/// A stepping session. Steps arrive as [`StepperEvent::Step`]; every 500 steps the
/// stepper pauses until [`StepperSession::more`] is called. Dropping it kills the process.
pub struct StepperSession {
    process: Process,
    events: Receiver<StepperEvent>,
}

impl StepperSession {
    pub fn start(backend: &Backend, request: &StepRequest) -> std::io::Result<StepperSession> {
        let cwd = request.path.as_deref().and_then(|p| p.parent());
        let (tx, events) = unbounded();
        let mut process = Process::spawn(
            backend.command("stepper.rkt", cwd),
            tx,
            backend.ctx(),
            |code, stderr| Some(StepperEvent::Terminated { code, stderr }),
        )?;
        let path = request.path.as_ref().map(|p| p.to_string_lossy().into_owned());
        process.send(&StepperCommand::Step {
            path: path.as_deref(),
            source: &request.source,
            level: &request.level,
            settings: &request.settings,
            teachpacks: &request.teachpacks,
            width: request.width,
        })?;
        Ok(StepperSession { process, events })
    }

    /// Continue after a `Paused` event.
    pub fn more(&mut self) {
        let _ = self.process.send(&StepperCommand::More);
    }

    pub fn try_recv(&self) -> Option<StepperEvent> {
        self.events.try_recv().ok()
    }

    pub fn drain(&self) -> Vec<StepperEvent> {
        self.events.try_iter().collect()
    }

    pub fn events(&self) -> &Receiver<StepperEvent> {
        &self.events
    }

    pub fn is_alive(&mut self) -> bool {
        self.process.is_alive()
    }

    pub fn kill(&mut self) {
        self.process.kill();
    }
}
