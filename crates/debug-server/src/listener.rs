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

/// Spawn the TCP listener thread and return the drain system + listener handle.
pub fn spawn(port: u16) -> (DebugDrainSystem, JoinHandle<()>) {
    let queue: CommandQueue = Arc::new(Mutex::new(Vec::new()));
    let system = DebugDrainSystem::new(queue.clone());

    let handle = thread::Builder::new()
        .name("byro-debug-listener".to_string())
        .spawn(move || listener_loop(port, queue))
        .expect("failed to spawn debug listener thread");

    (system, handle)
}

fn listener_loop(port: u16, queue: CommandQueue) {
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
        match listener.accept() {
            Ok((stream, addr)) => {
                log::info!("Debug client connected from {}", addr);
                let q = queue.clone();
                thread::Builder::new()
                    .name(format!("byro-debug-client-{}", addr))
                    .spawn(move || handle_client(stream, q))
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

fn handle_client(stream: TcpStream, queue: CommandQueue) {
    // Set blocking mode for the client stream (reader blocks on decode).
    stream
        .set_nonblocking(false)
        .expect("failed to set client stream blocking");
    stream.set_read_timeout(Some(Duration::from_secs(300))).ok();

    let mut reader = stream.try_clone().expect("failed to clone TCP stream");
    let mut writer = BufWriter::new(stream);

    loop {
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
