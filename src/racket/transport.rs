//! A child Racket process speaking newline-delimited JSON: commands go to its stdin, events
//! are parsed from its stdout on a reader thread and delivered through a channel.

use crossbeam_channel::Sender;
use parking_lot::Mutex;
use serde::Serialize;
use serde::de::DeserializeOwned;
use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::Arc;

/// How many lines of the child's stderr we keep for diagnostics.
const STDERR_TAIL: usize = 200;

pub(crate) struct Process {
    child: Child,
    stdin: Option<ChildStdin>,
    stderr_tail: Arc<Mutex<VecDeque<String>>>,
    killed: bool,
}

impl Process {
    /// Spawn `command` (with piped stdio) and start reader threads. Each stdout line is
    /// decoded as `E` and sent on `events`; when stdout closes, `on_end` may produce a final
    /// event from the exit code and the tail of stderr.
    pub(crate) fn spawn<E>(
        mut command: Command,
        events: Sender<E>,
        ctx: Option<egui::Context>,
        on_end: impl FnOnce(Option<i32>, String) -> Option<E> + Send + 'static,
    ) -> std::io::Result<Process>
    where
        E: DeserializeOwned + Send + 'static,
    {
        command.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
        #[cfg(unix)]
        {
            // Own process group, so killing it also kills anything the program started.
            use std::os::unix::process::CommandExt;
            command.process_group(0);
        }
        let mut child = command.spawn()?;
        let stdin = child.stdin.take();
        let stdout = child.stdout.take().expect("piped stdout");
        let stderr = child.stderr.take().expect("piped stderr");
        let stderr_tail = Arc::new(Mutex::new(VecDeque::new()));

        let tail = stderr_tail.clone();
        std::thread::Builder::new()
            .name("racket-stderr".into())
            .spawn(move || {
                for line in BufReader::new(stderr).lines() {
                    let Ok(line) = line else { break };
                    let mut t = tail.lock();
                    if t.len() == STDERR_TAIL {
                        t.pop_front();
                    }
                    t.push_back(line);
                }
            })?;

        let tail = stderr_tail.clone();
        std::thread::Builder::new()
            .name("racket-stdout".into())
            .spawn(move || {
                let mut reader = BufReader::new(stdout);
                let mut line = String::new();
                loop {
                    line.clear();
                    match reader.read_line(&mut line) {
                        Ok(0) | Err(_) => break,
                        Ok(_) => {}
                    }
                    let trimmed = line.trim_end();
                    if trimmed.is_empty() {
                        continue;
                    }
                    match serde_json::from_str::<E>(trimmed) {
                        Ok(ev) => {
                            if events.send(ev).is_err() {
                                return;
                            }
                            if let Some(ctx) = &ctx {
                                ctx.request_repaint();
                            }
                        }
                        Err(e) => {
                            let preview: String = trimmed.chars().take(200).collect();
                            eprintln!("racketeer: ignoring malformed message from racket ({e}): {preview}");
                        }
                    }
                }
                // Give stderr a moment to drain so the final report includes it.
                std::thread::sleep(std::time::Duration::from_millis(30));
                let stderr: Vec<String> = tail.lock().iter().cloned().collect();
                // The exit code is not observable here (the `Child` is owned by `Process`);
                // sessions expose it through `Process::exit_code`.
                if let Some(ev) = on_end(None, stderr.join("\n")) {
                    let _ = events.send(ev);
                    if let Some(ctx) = &ctx {
                        ctx.request_repaint();
                    }
                }
            })?;

        Ok(Process { child, stdin, stderr_tail, killed: false })
    }

    /// Send one JSON message. Errors mean the process is gone.
    pub(crate) fn send<T: Serialize>(&mut self, msg: &T) -> std::io::Result<()> {
        let stdin = self
            .stdin
            .as_mut()
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::BrokenPipe, "stdin closed"))?;
        let mut line = serde_json::to_string(msg).map_err(std::io::Error::other)?;
        line.push('\n');
        stdin.write_all(line.as_bytes())?;
        stdin.flush()
    }

    pub(crate) fn is_alive(&mut self) -> bool {
        !self.killed && matches!(self.child.try_wait(), Ok(None))
    }

    pub(crate) fn stderr_tail(&self) -> String {
        self.stderr_tail.lock().iter().cloned().collect::<Vec<_>>().join("\n")
    }

    pub(crate) fn pid(&self) -> u32 {
        self.child.id()
    }

    /// The exit code, once the process has exited on its own.
    pub(crate) fn exit_code(&mut self) -> Option<i32> {
        self.child.try_wait().ok().flatten().and_then(|s| s.code())
    }

    /// Kill the process and its whole process group (windows opened by the program,
    /// subprocesses it started). Idempotent.
    pub(crate) fn kill(&mut self) {
        if self.killed {
            return;
        }
        self.killed = true;
        self.stdin = None;
        #[cfg(unix)]
        {
            unsafe extern "C" {
                fn killpg(pgrp: i32, sig: i32) -> i32;
            }
            const SIGKILL: i32 = 9;
            // SAFETY: plain libc call; the group id is our child's pid (process_group(0)).
            unsafe {
                killpg(self.child.id() as i32, SIGKILL);
            }
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Drop for Process {
    fn drop(&mut self) {
        self.kill();
    }
}

/// A `racket <script>` command with a sane environment.
pub(crate) fn racket_command(racket: &Path, script: &Path, cwd: Option<&Path>) -> Command {
    let mut cmd = Command::new(racket);
    cmd.arg(script);
    if let Some(dir) = cwd.filter(|d| d.is_dir()) {
        cmd.current_dir(dir);
    }
    // Racket writes UTF-8 regardless of locale, but child programs may not.
    cmd.env("LANG", std::env::var("LANG").unwrap_or_else(|_| "en_US.UTF-8".into()));
    cmd
}
