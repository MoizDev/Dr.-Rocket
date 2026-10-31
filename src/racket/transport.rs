//! A child Racket process speaking newline-delimited JSON: commands go to its stdin, events
//! are parsed from its stdout on a reader thread and delivered through a channel.

use base64::Engine;
use crossbeam_channel::Sender;
use parking_lot::Mutex;
use serde::Serialize;
use serde::de::DeserializeOwned;
use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::Arc;

/// How many lines of the child's stderr we keep for diagnostics.
const STDERR_TAIL: usize = 200;
pub(crate) const QUEUE_CAPACITY: usize = 16;
pub(crate) const FRAME_EVENTS: usize = 128;
const MAX_LOGICAL_MESSAGE: usize = 64 * 1024 * 1024;
const MAX_MESSAGE: usize = 4 * 1024 * 1024;
const MAX_STDERR_LINE: usize = 16 * 1024;

fn read_line_limited(reader: &mut impl BufRead, line: &mut String, limit: usize) -> std::io::Result<usize> {
    line.clear();
    let size = reader.take((limit + 1) as u64).read_line(line)?;
    if size > limit { return Err(std::io::Error::other("Racket protocol message exceeded its size limit")); }
    Ok(size)
}

/// Chunks are contiguous: the backend's single writer never interleaves messages.
/// A malformed sequence fails closed, with bounded allocation even for a hostile child.
#[derive(Default)]
struct MessageDecoder {
    assembled: Vec<u8>,
    frames: Vec<serde_json::Value>,
}

impl MessageDecoder {
    fn decode(&mut self, line: &str) -> Result<Option<serde_json::Value>, String> {
        let mut value: serde_json::Value = serde_json::from_str(line).map_err(|e| e.to_string())?;
        if let Some(chunk) = value.get("chunk") {
            let chunk = chunk.as_str().ok_or("invalid Racket chunk")?;
            let last = value.get("last").and_then(|v| v.as_bool()).ok_or("missing Racket chunk terminator")?;
            if chunk.len() > 512 * 1024 { return Err("Racket chunk exceeded its size limit".into()); }
            let bytes = base64::engine::general_purpose::STANDARD.decode(chunk).map_err(|e| e.to_string())?;
            if bytes.is_empty() || bytes.len() > MAX_LOGICAL_MESSAGE.saturating_sub(self.assembled.len()) {
                return Err("Racket response exceeded its logical size limit".into());
            }
            self.assembled.extend_from_slice(&bytes);
            if !last { return Ok(None); }
            value = serde_json::from_slice(&self.assembled).map_err(|e| e.to_string())?;
            self.assembled = Vec::new();
        } else if !self.assembled.is_empty() {
            return Err("interleaved Racket response chunks".into());
        }
        if value.get("ev").and_then(|v| v.as_str()) == Some("viz-start") {
            self.frames.clear();
        }
        if value.get("ev").and_then(|v| v.as_str()) == Some("viz-step") {
            let prefix = value.get("frame_prefix").and_then(|v| v.as_u64()).unwrap_or(0);
            if prefix > self.frames.len() as u64 { return Err("invalid visualizer frame prefix".into()); }
            let suffix = value.get("frames").and_then(|v| v.as_array()).ok_or("missing visualizer frames")?;
            if prefix as usize + suffix.len() > 64 { return Err("visualizer frame limit exceeded".into()); }
            self.frames.truncate(prefix as usize);
            self.frames.extend_from_slice(suffix);
            value["frames"] = serde_json::Value::Array(self.frames.clone());
        }
        // Printed text is carried once on the wire. Keep the public RunEvent API
        // compatible with existing consumers, including images-only values.
        if value.get("ev").and_then(|v| v.as_str()) == Some("value") && value.get("text").is_none() {
            let text: String = value.get("segments").and_then(|v| v.as_array()).into_iter().flatten()
                .filter_map(|s| s.get("text").and_then(|v| v.as_str())).collect();
            value["text"] = serde_json::Value::String(text);
        }
        Ok(Some(value))
    }
}

fn kill_child(child: &mut Child) {
    #[cfg(unix)]
    {
        unsafe extern "C" { fn killpg(pgrp: i32, sig: i32) -> i32; }
        // SAFETY: the child was spawned in its own process group.
        unsafe { killpg(child.id() as i32, 9); }
    }
    let _ = child.kill();
    let _ = child.wait();
}

pub(crate) struct Process {
    child: Arc<Mutex<Child>>,
    stdin: Option<Sender<String>>,
    stderr_tail: Arc<Mutex<VecDeque<String>>>,
    killed: bool,
    ctx: Option<egui::Context>,
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
        let mut stdin = child.stdin.take().expect("piped stdin");
        let stdout = child.stdout.take().expect("piped stdout");
        let stderr = child.stderr.take().expect("piped stderr");
        let stderr_tail = Arc::new(Mutex::new(VecDeque::new()));
        let child = Arc::new(Mutex::new(child));
        let (commands, input) = crossbeam_channel::bounded::<String>(QUEUE_CAPACITY);
        std::thread::Builder::new().name("racket-stdin".into()).spawn(move || {
            for line in input {
                if stdin.write_all(line.as_bytes()).and_then(|()| stdin.flush()).is_err() { break; }
            }
        })?;

        let tail = stderr_tail.clone();
        let stderr_child = child.clone();
        std::thread::Builder::new()
            .name("racket-stderr".into())
            .spawn(move || {
                let mut reader = BufReader::new(stderr);
                let mut line = String::new();
                loop {
                    match read_line_limited(&mut reader, &mut line, MAX_STDERR_LINE) {
                        Ok(0) => break,
                        Ok(_) => {},
                        Err(e) => {
                            tail.lock().push_back(e.to_string());
                            kill_child(&mut stderr_child.lock());
                            break;
                        }
                    }
                    let mut t = tail.lock();
                    if t.len() == STDERR_TAIL {
                        t.pop_front();
                    }
                    t.push_back(line.trim_end().to_string());
                }
            })?;

        let repaint = ctx.clone();
        let tail = stderr_tail.clone();
        let stdout_child = child.clone();
        std::thread::Builder::new()
            .name("racket-stdout".into())
            .spawn(move || {
                let mut reader = BufReader::new(stdout);
                let mut decoder = MessageDecoder::default();
                let mut line = String::new();
                loop {
                    line.clear();
                    match read_line_limited(&mut reader, &mut line, MAX_MESSAGE) {
                        Ok(0) => break,
                        Err(e) => {
                            tail.lock().push_back(e.to_string());
                            kill_child(&mut stdout_child.lock());
                            break;
                        }
                        Ok(_) => {}
                    }
                    let trimmed = line.trim_end();
                    if trimmed.is_empty() {
                        continue;
                    }
                    let value = match decoder.decode(trimmed) {
                        Ok(Some(value)) => value,
                        Ok(None) => continue,
                        Err(e) => {
                            tail.lock().push_back(format!("invalid Racket protocol: {e}"));
                            kill_child(&mut stdout_child.lock());
                            break;
                        }
                    };
                    match serde_json::from_value::<E>(value) {
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

        Ok(Process { child, stdin: Some(commands), stderr_tail, killed: false, ctx: repaint })
    }

    pub(crate) fn repaint(&self) {
        if let Some(ctx) = &self.ctx { ctx.request_repaint(); }
    }

    /// Send one JSON message. Errors mean the process is gone.
    pub(crate) fn send<T: Serialize>(&mut self, msg: &T) -> std::io::Result<()> {
        let stdin = self
            .stdin
            .as_mut()
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::BrokenPipe, "stdin closed"))?;
        let mut line = serde_json::to_string(msg).map_err(std::io::Error::other)?;
        line.push('\n');
        if line.len() > MAX_MESSAGE { return Err(std::io::Error::other("Racket command exceeded its size limit")); }
        stdin.try_send(line).map_err(|e| std::io::Error::other(format!("Racket command queue unavailable: {e}")))
    }

    pub(crate) fn is_alive(&mut self) -> bool {
        !self.killed && matches!(self.child.lock().try_wait(), Ok(None))
    }

    pub(crate) fn stderr_tail(&self) -> String {
        self.stderr_tail.lock().iter().cloned().collect::<Vec<_>>().join("\n")
    }

    pub(crate) fn pid(&self) -> u32 {
        self.child.lock().id()
    }

    /// The exit code, once the process has exited on its own.
    pub(crate) fn exit_code(&mut self) -> Option<i32> {
        self.child.lock().try_wait().ok().flatten().and_then(|s| s.code())
    }

    /// Kill the process and its whole process group (windows opened by the program,
    /// subprocesses it started). Idempotent.
    pub(crate) fn kill(&mut self) {
        if self.killed {
            return;
        }
        self.killed = true;
        self.stdin = None;
        kill_child(&mut self.child.lock());
    }
}

impl Drop for Process {
    fn drop(&mut self) {
        self.kill();
    }
}

/// On Windows, a GUI app's child processes would each flash a console window; don't.
pub(crate) fn no_console(cmd: &mut Command) -> &mut Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

/// A `racket <script>` command with a sane environment.
pub(crate) fn racket_command(racket: &Path, script: &Path, cwd: Option<&Path>) -> Command {
    let mut cmd = Command::new(racket);
    no_console(&mut cmd);
    cmd.arg(script);
    if let Some(dir) = cwd.filter(|d| d.is_dir()) {
        cmd.current_dir(dir);
    }
    // Racket writes UTF-8 regardless of locale, but child programs may not.
    cmd.env("LANG", std::env::var("LANG").unwrap_or_else(|_| "en_US.UTF-8".into()));
    cmd
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protocol_lines_are_bounded_even_without_newlines() {
        let mut line = String::new();
        assert_eq!(read_line_limited(&mut &b"ok\nnext"[..], &mut line, 3).unwrap(), 3);
        assert_eq!(line, "ok\n");
        assert!(read_line_limited(&mut &b"abcdef"[..], &mut line, 3).is_err());
        assert!(line.len() <= 4);
        assert!(read_line_limited(&mut &b"abcd\n"[..], &mut line, 3).is_err());
        assert_eq!(read_line_limited(&mut &b""[..], &mut line, 3).unwrap(), 0);
    }

    #[test]
    fn event_backlog_and_frame_work_are_bounded() {
        let (tx, rx) = crossbeam_channel::bounded(QUEUE_CAPACITY);
        for n in 0..QUEUE_CAPACITY { tx.try_send(n).unwrap(); }
        assert!(tx.try_send(0).is_err());
        assert_eq!(rx.try_iter().take(FRAME_EVENTS).count(), QUEUE_CAPACITY.min(FRAME_EVENTS));
        assert_eq!(rx.len(), QUEUE_CAPACITY.saturating_sub(FRAME_EVENTS));
    }
}

#[cfg(test)]
mod decoder_tests {
    use super::*;
    use serde_json::json;

    fn chunk(bytes: &[u8], last: bool) -> String {
        json!({"chunk": base64::engine::general_purpose::STANDARD.encode(bytes), "last": last}).to_string()
    }

    #[test]
    fn chunked_messages_preserve_unicode_segments_and_images() {
        let mut decoder = MessageDecoder::default();
        let bytes = br#"{"ev":"value","segments":[{"text":"\u03bb"},{"png":"image","width":1,"height":1}]}"#;
        assert!(decoder.decode(&chunk(&bytes[..31], false)).unwrap().is_none());
        let value = decoder.decode(&chunk(&bytes[31..], true)).unwrap().unwrap();
        assert_eq!(value["text"], "λ");
        assert_eq!(value["segments"][1]["png"], "image");
        assert!(decoder.assembled.is_empty());
    }

    #[test]
    fn chunk_sequences_reject_interleaving_invalid_base64_and_oversize() {
        let mut decoder = MessageDecoder::default();
        decoder.decode(&chunk(b"{", false)).unwrap();
        assert!(decoder.decode("{}").is_err());
        assert!(MessageDecoder::default().decode(r#"{"chunk":"!", "last":true}"#).is_err());
        decoder.assembled.resize(MAX_LOGICAL_MESSAGE - 1, b' ');
        assert!(decoder.decode(&chunk(b"xx", true)).is_err());
        assert!(decoder.assembled.len() < MAX_LOGICAL_MESSAGE);
    }

    #[test]
    fn visualizer_suffixes_reconstruct_complete_snapshots_and_reset_between_runs() {
        let mut decoder = MessageDecoder::default();
        decoder.decode(r#"{"ev":"viz-start"}"#).unwrap();
        decoder.decode(r#"{"ev":"viz-step","frame_prefix":0,"frames":[{"name":"outer"}]}"#).unwrap();
        let value = decoder.decode(r#"{"ev":"viz-step","frame_prefix":1,"frames":[{"name":"inner"}]}"#).unwrap().unwrap();
        assert_eq!(value["frames"], json!([{"name":"outer"},{"name":"inner"}]));
        decoder.decode(r#"{"ev":"viz-start"}"#).unwrap();
        assert!(decoder.decode(r#"{"ev":"viz-step","frame_prefix":1,"frames":[]}"#).is_err());
    }
}
