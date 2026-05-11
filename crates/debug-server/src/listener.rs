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
    stream
        .set_nonblocking(false)
        .expect("failed to set client stream blocking");
    stream.set_read_timeout(Some(Duration::from_secs(300))).ok();

    let mut reader = stream.try_clone().expect("failed to clone TCP stream");
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

        // Create a one-shot channel for the response.
        let (tx, rx) = mpsc::channel();

        // Push the command into the queue for the drain system.
        {
            let mut q = queue.lock().unwrap();
            q.push(PendingCommand {
                request,
                response_tx: tx,
            });
        }

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
}
