//! A program run: one runner process executing the definitions, then serving the REPL.

use super::protocol::*;
use super::transport::Process;
use super::Backend;
use crossbeam_channel::{Receiver, bounded};
use std::path::PathBuf;

/// Everything needed to start a run.
#[derive(Clone, Debug)]
pub struct RunConfig {
    /// The file being run, or `None` for an unsaved buffer.
    pub path: Option<PathBuf>,
    /// The program text, including any hidden DrRacket header.
    pub source: String,
    /// Length in chars of the hidden header at the start of `source`.
    pub offset: usize,
    pub language: RunLanguage,
    pub options: RunOptions,
}

/// A running program and its interactions. Dropping it kills the process (and any windows
/// the program opened).
pub struct RunSession {
    process: Process,
    events: Receiver<RunEvent>,
    next_id: u64,
    dead: bool,
}

/// A runner process started ahead of time. It loads its tooling while idle, so starting a
/// run from it skips process startup. Keep one around and replace it after each Run.
pub struct IdleRunner {
    process: Process,
    events: Receiver<RunEvent>,
}

impl IdleRunner {
    pub fn spawn(backend: &Backend) -> std::io::Result<IdleRunner> {
        let (tx, events) = bounded(super::transport::QUEUE_CAPACITY);
        let process = Process::spawn(backend.command("runner.rkt", None), tx, backend.ctx(), |code, stderr| {
            Some(RunEvent::Terminated { code, stderr })
        })?;
        Ok(IdleRunner { process, events })
    }

    pub fn is_alive(&mut self) -> bool {
        self.process.is_alive()
    }
}

impl RunSession {
    /// Start a run in a fresh runner process.
    pub fn start(backend: &Backend, config: &RunConfig) -> std::io::Result<RunSession> {
        Self::start_with(IdleRunner::spawn(backend)?, config)
    }

    /// Start a run in a pre-spawned runner. The runner resolves relative paths against the
    /// program's directory itself, so its working directory doesn't matter.
    pub fn start_with(idle: IdleRunner, config: &RunConfig) -> std::io::Result<RunSession> {
        let IdleRunner { mut process, events } = idle;
        let path = config.path.as_ref().map(|p| p.to_string_lossy().into_owned());
        process.send(&RunnerCommand::Run {
            path: path.as_deref(),
            source: &config.source,
            offset: config.offset,
            language: &config.language,
            options: &config.options,
        })?;
        Ok(RunSession { process, events, next_id: 1, dead: false })
    }

    /// Evaluate an interaction. Its results end with `RunEvent::Ready { id: Some(id) }`.
    pub fn eval(&mut self, source: &str) -> u64 {
        let id = self.fresh_id();
        self.send(&RunnerCommand::Eval { id, source });
        id
    }

    /// Feed text to the program's stdin (include the trailing newline).
    pub fn send_input(&mut self, text: &str) {
        self.send(&RunnerCommand::Input { text });
    }

    /// Close the program's stdin.
    pub fn send_eof(&mut self) {
        self.send(&RunnerCommand::InputEof);
    }

    /// Like DrRacket's Stop button: break the running evaluation.
    pub fn interrupt(&mut self) {
        self.send(&RunnerCommand::Break);
    }

    pub fn debug(&mut self, action: DebugAction) {
        self.send(&RunnerCommand::Debug { action });
    }

    /// Replace the debugger's breakpoints (char positions in the visible buffer).
    pub fn set_breakpoints(&mut self, positions: &[usize]) {
        self.send(&RunnerCommand::Breakpoints { positions, path: None });
    }

    /// Update breakpoints in another debugged file (see `RunOptions::debug_modules`).
    pub fn set_module_breakpoints(&mut self, path: &str, positions: &[usize]) {
        self.send(&RunnerCommand::Breakpoints { positions, path: Some(path) });
    }

    /// Resume a program paused at its memory limit, now allowed `limit_mb` (`None`: no limit).
    pub fn memory_continue(&mut self, limit_mb: Option<u64>) {
        self.send(&RunnerCommand::MemoryContinue { limit_mb });
    }

    /// Ask for identifiers in the REPL namespace starting with `prefix`; answered by
    /// `RunEvent::Completions { id, .. }`.
    pub fn complete(&mut self, prefix: &str) -> u64 {
        let id = self.fresh_id();
        self.send(&RunnerCommand::Complete { id, prefix });
        id
    }

    pub fn try_recv(&self) -> Option<RunEvent> {
        self.events.try_recv().ok()
    }

    /// All events received so far.
    pub fn drain(&self) -> Vec<RunEvent> {
        let events = self.events.try_iter().take(super::transport::FRAME_EVENTS).collect();
        if !self.events.is_empty() { self.process.repaint(); }
        events
    }

    pub fn events(&self) -> &Receiver<RunEvent> {
        &self.events
    }

    pub fn is_alive(&mut self) -> bool {
        !self.dead && self.process.is_alive()
    }

    pub fn exit_code(&mut self) -> Option<i32> {
        self.process.exit_code()
    }

    pub fn stderr_tail(&self) -> String {
        self.process.stderr_tail()
    }

    /// DrRacket's Kill: terminate the program immediately.
    pub fn kill(&mut self) {
        self.dead = true;
        self.process.kill();
    }

    fn fresh_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    fn send(&mut self, cmd: &RunnerCommand<'_>) {
        if self.process.send(cmd).is_err() {
            self.dead = true;
        }
    }
}
