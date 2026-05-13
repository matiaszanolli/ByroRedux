//! TCP listener — accepts debug CLI connections on a background thread.
//!
//! The listener runs non-blocking on a dedicated thread. Each accepted
//! client gets a reader thread that decodes requests and pushes them
//! into a shared command queue. The drain system (Late-stage exclusive)
//! processes the queue each frame and sends responses back.

use crate::system::DebugDrainSystem;
use byroredux_debug_protocol::{wire, DebugRequest, DebugResponse};
use std::io::BufWriter;
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

/// A pending command: the request and a channel to send the response back.
pub(crate) struct PendingCommand {
    pub request: DebugRequest,
    pub response_tx: mpsc::Sender<DebugResponse>,
}

/// Shared command queue between listener threads and the drain system.
pub(crate) type CommandQueue = Arc<Mutex<Vec<PendingCommand>>>;

/// Maximum number of in-flight commands across all clients before the
/// per-client `handle_client` thread synchronously rejects new commands
/// with `DebugResponse::error("server overloaded ...")`. Per-client
/// backpressure is naturally 1-in-flight (each thread blocks on
/// `recv_timeout` after pushing), so this only fires under N clients ×
/// commands-between-drains. Debug server is loopback-only (#857), so
/// the real attack surface is operator-controlled. Cap exists to bound
/// memory under a CLI-bug-driven tight-loop flood. See #1010.
pub(crate) const MAX_QUEUED_COMMANDS: usize = 64;

/// Try to enqueue a debug command into the shared `CommandQueue`,
/// returning the response receiver on success or `None` when the queue
/// is at capacity. Lock held only across the check+push pair so
/// concurrent per-client threads can't both slip past the cap. See
/// #1010.
pub(crate) fn try_enqueue_command(
    queue: &CommandQueue,
    request: DebugRequest,
) -> Option<mpsc::Receiver<DebugResponse>> {
    let (tx, rx) = mpsc::channel();
    let mut q = queue.lock().unwrap();
    if q.len() >= MAX_QUEUED_COMMANDS {
        return None;
    }
    q.push(PendingCommand {
        request,
        response_tx: tx,
    });
    Some(rx)
}

/// Handle returned by [`spawn`] / [`crate::start`]. Owning the handle keeps
/// the listener thread alive; dropping it signals shutdown and joins the
/// listener cleanly. Per-client threads stay detached (they observe the
/// same shutdown flag and self-terminate when their next read returns),
/// since their natural termination on TCP EOF / 300 s read timeout / process
/// exit was already the accepted contract — see #855 / C6-NEW-02.
pub struct DebugServerHandle {
    listener: Option<JoinHandle<()>>,
    shutdown: Arc<AtomicBool>,
}

impl DebugServerHandle {
    /// Signal shutdown to the listener and (best-effort) join its thread.
    /// Idempotent; subsequent calls are no-ops. Per-client threads stay
    /// detached but will observe the same flag on their next read.
    pub fn shutdown_and_join(&mut self) {
        self.shutdown.store(true, Ordering::Release);
        if let Some(handle) = self.listener.take() {
            if let Err(panic_payload) = handle.join() {
                log::warn!(
                    "Debug server listener thread panicked during shutdown: {:?}",
                    panic_payload
                );
            }
        }
    }
}

impl Drop for DebugServerHandle {
    fn drop(&mut self) {
        self.shutdown_and_join();
    }
}

/// Spawn the TCP listener thread and return the drain system + the
/// shutdown-aware handle. Holding the handle keeps the listener thread
/// alive; dropping it signals shutdown and joins cleanly.
pub fn spawn(port: u16) -> (DebugDrainSystem, DebugServerHandle) {
    let queue: CommandQueue = Arc::new(Mutex::new(Vec::new()));
    let system = DebugDrainSystem::new(queue.clone());
    let shutdown = Arc::new(AtomicBool::new(false));
    let shutdown_listener = Arc::clone(&shutdown);

    let handle = thread::Builder::new()
        .name("byro-debug-listener".to_string())
        .spawn(move || listener_loop(port, queue, shutdown_listener))
        .expect("failed to spawn debug listener thread");

    (
        system,
        DebugServerHandle {
            listener: Some(handle),
            shutdown,
        },
    )
}

fn listener_loop(port: u16, queue: CommandQueue, shutdown: Arc<AtomicBool>) {
    // Bind hostname is currently hardcoded to 127.0.0.1 — debug
    // server is loopback-only by design (no exposed port to the
    // network). The matching log line in `lib.rs::start` says the
    // same thing; both must move in lockstep if a future feature
    // adds a host argument. See #857.
    let listener = match TcpListener::bind(format!("127.0.0.1:{}", port)) {
        Ok(l) => l,
        Err(e) => {
            log::error!("Debug server failed to bind port {}: {}", port, e);
            return;
        }
    };

    // Non-blocking accept so the thread can be joined on shutdown.
    listener
        .set_nonblocking(true)
        .expect("failed to set listener non-blocking");

    loop {
        if shutdown.load(Ordering::Acquire) {
            log::info!("Debug listener received shutdown signal — exiting cleanly");
            return;
        }
        match listener.accept() {
            Ok((stream, addr)) => {
                // Don't accept new clients after shutdown was signalled —
                // they'd never observe it and would survive past the
                // listener join.
                if shutdown.load(Ordering::Acquire) {
                    drop(stream);
                    return;
                }
                log::info!("Debug client connected from {}", addr);
                let q = queue.clone();
                let s = Arc::clone(&shutdown);
                thread::Builder::new()
                    .name(format!("byro-debug-client-{}", addr))
                    .spawn(move || handle_client(stream, q, s))
                    .ok();
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                // No pending connection — sleep briefly to avoid busy-spin.
                thread::sleep(Duration::from_millis(50));
            }
            Err(e) => {
                log::error!("Debug listener accept error: {}", e);
                thread::sleep(Duration::from_millis(100));
            }
        }
    }
}

fn handle_client(stream: TcpStream, queue: CommandQueue, shutdown: Arc<AtomicBool>) {
    // Set blocking mode for the client stream (reader blocks on decode).
    // #1008 — pre-fix this site used `.expect()`, panicking the
    // per-client thread on FD exhaustion / socket-level kernel errors
    // without any `log::error!` to surface the cause. Listener kept
    // running and other clients were unaffected, but the failure mode
    // was invisible until process exit (default panic hook prints to
    // stderr). Mirror `cell_pre_parse_worker`'s log+return recovery.
    if let Err(e) = stream.set_nonblocking(false) {
        log::warn!(
            "debug-server: client setup failed (set_nonblocking): {} — closing connection",
            e
        );
        return;
    }
    stream.set_read_timeout(Some(Duration::from_secs(300))).ok();

    let mut reader = match stream.try_clone() {
        Ok(r) => r,
        Err(e) => {
            log::warn!(
                "debug-server: client setup failed (try_clone): {} — closing connection",
                e
            );
            return;
        }
    };
    let mut writer = BufWriter::new(stream);

    loop {
        // Server-wide shutdown check between requests so a flag flipped
        // after the previous response was sent terminates this thread
        // before it blocks on the next read (#855 / C6-NEW-02). Still
        // best-effort — a flag flipped *during* a long-idle read is only
        // observed when the read returns (EOF / next message / 300 s
        // timeout). Tighter responsiveness would require either a
        // shorter read timeout (which would disconnect idle CLI users)
        // or shutting down the socket from the listener side.
        if shutdown.load(Ordering::Acquire) {
            return;
        }
        // Read one request from the client.
        let request: DebugRequest = match wire::decode(&mut reader) {
            Ok(req) => req,
            Err(e) => {
                if e.kind() == std::io::ErrorKind::UnexpectedEof {
                    log::info!("Debug client disconnected");
                } else {
                    log::warn!("Debug client read error: {}", e);
                }
                return;
            }
        };

        // #1010 — atomic check-and-push: if the queue is at capacity,
        // synchronously reject with an overload error without ever
        // enqueueing. Lock is held briefly across both ops so two
        // concurrent clients can't both slip past the cap.
        let Some(rx) = try_enqueue_command(&queue, request) else {
            let response = DebugResponse::error(format!(
                "debug-server overloaded ({} commands in flight) — drop and retry",
                MAX_QUEUED_COMMANDS
            ));
            if let Err(e) = wire::send(&mut writer, &response) {
                log::warn!("Debug client write error during overload reject: {}", e);
                return;
            }
            continue;
        };

        // Wait for the drain system to process it (next frame).
        match rx.recv_timeout(Duration::from_secs(5)) {
            Ok(response) => {
                if let Err(e) = wire::send(&mut writer, &response) {
                    log::warn!("Debug client write error: {}", e);
                    return;
                }
            }
            Err(_) => {
                // Timeout — engine might be paused or very slow frame.
                let timeout_resp = DebugResponse::error("timeout waiting for engine response");
                if wire::send(&mut writer, &timeout_resp).is_err() {
                    return;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    /// Regression for #855 / C6-NEW-02. Dropping a [`DebugServerHandle`]
    /// signals shutdown to the listener thread and joins it cleanly
    /// within the listener's 50 ms `WouldBlock` poll cadence. Pre-fix
    /// the listener was detached and would only exit on process exit.
    ///
    /// Uses port 0 so the OS picks a free port — the test only verifies
    /// the lifecycle, not that we can connect to a known port (and
    /// running multiple cargo-test invocations in parallel would fight
    /// over a hardcoded port).
    #[test]
    fn dropping_handle_joins_listener_thread() {
        let (drain, handle) = spawn(0);
        // Drain system is held just to mirror the production flow where
        // it's moved into the scheduler. Dropping the handle is what
        // exercises the bug.
        drop(drain);

        let t0 = Instant::now();
        drop(handle);
        let elapsed = t0.elapsed();

        // Listener polls shutdown every 50 ms; allow a generous 2 s
        // ceiling so this test is robust on contended CI runners. The
        // pre-fix behaviour was an *infinite* hang on join, so any
        // bounded elapsed time below this ceiling proves the fix.
        assert!(
            elapsed < Duration::from_secs(2),
            "DebugServerHandle Drop took {:?} — listener join did not honour shutdown",
            elapsed,
        );
    }

    fn empty_queue() -> CommandQueue {
        Arc::new(Mutex::new(Vec::new()))
    }

    fn ping_request() -> DebugRequest {
        DebugRequest::Stats
    }

    /// #1010 — `try_enqueue_command` admits commands when the queue is
    /// below `MAX_QUEUED_COMMANDS`.
    #[test]
    fn try_enqueue_accepts_when_queue_has_capacity() {
        let queue = empty_queue();
        let rx = try_enqueue_command(&queue, ping_request());
        assert!(rx.is_some(), "enqueue must succeed on empty queue");
        assert_eq!(queue.lock().unwrap().len(), 1, "command landed");
    }

    /// #1010 — `try_enqueue_command` rejects when the queue is at
    /// capacity. Drains the receiver to prove the rejection is
    /// synchronous (no enqueue happened).
    #[test]
    fn try_enqueue_rejects_when_queue_is_full() {
        let queue = empty_queue();
        for _ in 0..MAX_QUEUED_COMMANDS {
            let rx = try_enqueue_command(&queue, ping_request());
            assert!(rx.is_some());
        }
        // Cap reached — next enqueue returns None.
        let rx = try_enqueue_command(&queue, ping_request());
        assert!(rx.is_none(), "enqueue at cap must reject");
        assert_eq!(
            queue.lock().unwrap().len(),
            MAX_QUEUED_COMMANDS,
            "no overflow push slipped through"
        );
    }

    /// #1010 — after draining, capacity returns and subsequent enqueues
    /// succeed.
    #[test]
    fn try_enqueue_accepts_again_after_drain() {
        let queue = empty_queue();
        for _ in 0..MAX_QUEUED_COMMANDS {
            try_enqueue_command(&queue, ping_request());
        }
        // Drain (simulating DebugDrainSystem).
        let _ = std::mem::take(&mut *queue.lock().unwrap());
        // Fresh capacity.
        let rx = try_enqueue_command(&queue, ping_request());
        assert!(rx.is_some(), "post-drain enqueue must succeed");
    }
}
