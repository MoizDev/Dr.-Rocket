//! Client for the long-lived language server (check syntax, docs, completions, macro
//! stepper, module graph). Requests are asynchronous: [`Server::request`] returns an id and
//! results are collected with [`Server::poll`] / [`Server::take`]. If the process dies, the
//! pending requests fail and the next request restarts it.

use super::protocol::*;
use super::transport::Process;
use super::Backend;
use crossbeam_channel::{Receiver, bounded};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{Duration, Instant};

pub type RequestId = u64;

#[derive(Deserialize)]
struct RawResponse {
    id: Option<serde_json::Value>,
    ok: bool,
    #[serde(default)]
    result: serde_json::Value,
    #[serde(default)]
    error: Option<String>,
    /// Set locally when the process ends.
    #[serde(skip)]
    ended: Option<String>,
}

#[derive(Serialize)]
struct Envelope<'a> {
    id: RequestId,
    #[serde(flatten)]
    request: &'a ServerRequest,
}

#[derive(Serialize)]
struct Cancel {
    cmd: &'static str,
    id: RequestId,
}

pub struct Server {
    backend: Backend,
    process: Option<Process>,
    raw: Receiver<RawResponse>,
    pending: HashMap<RequestId, (ServerRequestKind, Instant)>,
    results: HashMap<RequestId, Result<ServerResponse, String>>,
    next_id: RequestId,
    restarts: u32,
}

impl Server {
    pub fn start(backend: &Backend) -> std::io::Result<Server> {
        let mut server = Server {
            backend: backend.clone(),
            process: None,
            raw: bounded(super::transport::QUEUE_CAPACITY).1,
            pending: HashMap::new(),
            results: HashMap::new(),
            next_id: 1,
            restarts: 0,
        };
        server.spawn()?;
        server.restarts = 0;
        Ok(server)
    }

    fn spawn(&mut self) -> std::io::Result<()> {
        let (tx, rx) = bounded(super::transport::QUEUE_CAPACITY);
        let process = Process::spawn(self.backend.command("server.rkt", None), tx, self.backend.ctx(), |_code, stderr| {
            Some(RawResponse { id: None, ok: false, result: serde_json::Value::Null, error: None, ended: Some(stderr) })
        })?;
        self.process = Some(process);
        self.raw = rx;
        self.restarts += 1;
        Ok(())
    }

    /// Number of times the server process has been (re)started after the first.
    pub fn restarts(&self) -> u32 {
        self.restarts
    }

    /// Send a request. Its result becomes available through [`Server::take`].
    pub fn request(&mut self, request: ServerRequest) -> RequestId {
        let id = self.next_id;
        self.next_id += 1;
        if self.pending.len() >= 32 {
            self.results.insert(id, Err("Too many pending Racket requests".into()));
            return id;
        }
        let alive = self.process.as_mut().is_some_and(|p| p.is_alive());
        if !alive && let Err(e) = self.spawn() {
            self.results.insert(id, Err(format!("could not start the Racket server: {e}")));
            return id;
        }
        let sent = self
            .process
            .as_mut()
            .map(|p| p.send(&Envelope { id, request: &request }))
            .unwrap_or_else(|| Err(std::io::ErrorKind::BrokenPipe.into()));
        match sent {
            Ok(()) => {
                self.pending.insert(id, (request.kind(), Instant::now()));
            }
            Err(e) => {
                self.results.insert(id, Err(format!("the Racket server is not running: {e}")));
            }
        }
        id
    }

    /// Cancel a pending request; it completes with `Err("cancelled")`.
    pub fn cancel(&mut self, id: RequestId) {
        if self.pending.remove(&id).is_some() {
            if let Some(p) = self.process.as_mut() { let _ = p.send(&Cancel { cmd: "cancel", id }); }
            self.results.insert(id, Err("cancelled".into()));
        }
    }

    pub fn is_pending(&self, id: RequestId) -> bool {
        self.pending.contains_key(&id)
    }

    /// Collect responses that have arrived.
    pub fn poll(&mut self) {
        if self.pending.values().any(|(_, start)| start.elapsed() > Duration::from_secs(30)) {
            // Restart the whole process: a timed-out loader can corrupt the shared module registry.
            self.process = None;
            self.raw = bounded(1).1;
            for (id, _) in self.pending.drain() {
                self.results.insert(id, Err("Racket analysis exceeded its 30-second time limit; server restarted".into()));
            }
        }
        let start = Instant::now();
        for _ in 0..super::transport::FRAME_EVENTS {
            if start.elapsed() > Duration::from_millis(2) { break; }
            let Ok(raw) = self.raw.try_recv() else { break };
            self.accept(raw);
        }
        if (!self.raw.is_empty() || !self.pending.is_empty())
            && let Some(ctx) = self.backend.ctx() {
            ctx.request_repaint_after(Duration::from_millis(50));
        }
    }

    fn accept(&mut self, raw: RawResponse) {
        if let Some(stderr) = raw.ended {
            let detail = stderr.lines().rev().take(8).collect::<Vec<_>>().into_iter().rev().collect::<Vec<_>>().join("\n");
            let msg = if detail.is_empty() {
                "the Racket server stopped".to_string()
            } else {
                format!("the Racket server stopped:\n{detail}")
            };
            for (id, _) in self.pending.drain() {
                self.results.insert(id, Err(msg.clone()));
            }
            self.process = None;
            return;
        }
        let Some(id) = raw.id.as_ref().and_then(|v| v.as_u64()) else { return };
        let Some((kind, _)) = self.pending.remove(&id) else { return };
        let result = if raw.ok {
            ServerResponse::decode(kind, raw.result)
        } else {
            Err(raw.error.unwrap_or_else(|| "request failed".into()))
        };
        self.results.insert(id, result);
    }

    /// Take the result of `id` if it has arrived (call [`Server::poll`] first).
    pub fn take(&mut self, id: RequestId) -> Option<Result<ServerResponse, String>> {
        self.results.remove(&id)
    }

    /// Poll and take every finished result.
    pub fn take_all(&mut self) -> Vec<(RequestId, Result<ServerResponse, String>)> {
        self.poll();
        self.results.drain().collect()
    }

    /// Block until `id` completes or `timeout` elapses.
    pub fn wait(&mut self, id: RequestId, timeout: Duration) -> Option<Result<ServerResponse, String>> {
        let deadline = Instant::now() + timeout;
        loop {
            self.poll();
            if let Some(r) = self.take(id) {
                return Some(r);
            }
            let left = deadline.checked_duration_since(Instant::now())?;
            if let Ok(raw) = self.raw.recv_timeout(left.min(Duration::from_millis(100))) {
                self.accept(raw);
            }
        }
    }

    pub fn stderr_tail(&self) -> String {
        self.process.as_ref().map(|p| p.stderr_tail()).unwrap_or_default()
    }
}
