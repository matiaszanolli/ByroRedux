//! Late-stage exclusive system that drains the debug command queue.
//!
//! Runs after all other systems, with exclusive access to the World.
//! Processes pending debug requests and sends responses back to clients.

use crate::evaluator;
use crate::listener::CommandQueue;
use byroredux_core::ecs::resources::{
    ScreenshotBridge, SCREENSHOT_OWNER_CLI, SCREENSHOT_OWNER_DEBUG_SERVER,
};
use byroredux_core::ecs::system::System;
use byroredux_core::ecs::world::World;
use byroredux_debug_protocol::registry::ComponentRegistry;
use byroredux_debug_protocol::{DebugRequest, DebugResponse};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};

/// A screenshot request waiting for the renderer to produce the PNG.
struct PendingScreenshot {
    response_tx: mpsc::Sender<DebugResponse>,
    save_path: Option<String>,
    frames_waited: u32,
    /// Cancel flag shared with the per-client thread. The client's
    /// `recv_timeout` (5 s) sets this when it gives up on the wait;
    /// the drain system checks it each frame and cancels the in-
    /// flight GPU capture so a straggler PNG isn't left in
    /// `ScreenshotBridge.result` for the next request to mistakenly
    /// claim. See #1007.
    cancel: Arc<AtomicBool>,
}

/// The drain system that processes debug commands each frame.
///
/// Stored in the scheduler as an exclusive Late-stage system.
/// The component registry is owned by this system (not a World resource)
/// to avoid coupling debug-protocol with the core Resource trait.
pub struct DebugDrainSystem {
    queue: CommandQueue,
    registry: ComponentRegistry,
    pending_screenshot: Option<PendingScreenshot>,
}

impl DebugDrainSystem {
    pub(crate) fn new(queue: CommandQueue) -> Self {
        Self {
            queue,
            registry: ComponentRegistry::new(),
            pending_screenshot: None,
        }
    }

    /// Access the registry for component registration during setup.
    pub fn registry_mut(&mut self) -> &mut ComponentRegistry {
        &mut self.registry
    }
}

impl System for DebugDrainSystem {
    fn run(&mut self, world: &World, _dt: f32) {
        // Check if a pending screenshot result is ready.
        if let Some(ref mut pending) = self.pending_screenshot {
            pending.frames_waited += 1;

            // #1007 — per-client thread's 5 s `recv_timeout` outraced
            // the engine's 10-frame ceiling on paused / GPU-stalled
            // engines, leaking a straggler PNG into the bridge's
            // result slot that the next request would mistakenly
            // claim. Honour the abandonment signal: cancel the in-
            // flight GPU capture and clear our bookkeeping. The
            // already-disconnected `response_tx.send` would Err
            // anyway, so skip it.
            if pending.cancel.load(Ordering::Acquire) {
                if let Some(bridge) = world.try_resource::<ScreenshotBridge>() {
                    bridge.cancel();
                }
                self.pending_screenshot = None;
                return;
            }

            if let Some(bridge) = world.try_resource::<ScreenshotBridge>() {
                // #1006 — owner-gated take ensures we don't steal
                // bytes intended for the CLI screenshot path.
                if let Some(png_bytes) = bridge.take_result_for(SCREENSHOT_OWNER_DEBUG_SERVER) {
                    let response = match &pending.save_path {
                        Some(path) => match std::fs::write(path, &png_bytes) {
                            Ok(()) => DebugResponse::ScreenshotSaved { path: path.clone() },
                            Err(e) => {
                                DebugResponse::error(format!("failed to write screenshot: {}", e))
                            }
                        },
                        None => {
                            // No path specified — save to timestamped file.
                            let auto_path = format!(
                                "screenshot_{}.png",
                                std::time::SystemTime::now()
                                    .duration_since(std::time::UNIX_EPOCH)
                                    .unwrap_or_default()
                                    .as_secs()
                            );
                            match std::fs::write(&auto_path, &png_bytes) {
                                Ok(()) => DebugResponse::ScreenshotSaved { path: auto_path },
                                Err(e) => DebugResponse::error(format!(
                                    "failed to write screenshot: {}",
                                    e
                                )),
                            }
                        }
                    };
                    let _ = pending.response_tx.send(response);
                    self.pending_screenshot = None;
                } else if pending.frames_waited > 10 {
                    // Timeout: renderer didn't produce a screenshot in 10 frames.
                    // #1011 — cancel the bridge state too. If the renderer
                    // hasn't yet observed `requested = true` (paused,
                    // swapchain recreate), a later draw would write a
                    // straggler result that the next screenshot command
                    // would mistakenly claim. `cancel()` clears both
                    // the AtomicBool and any buffered bytes atomically
                    // from the system's POV.
                    bridge.cancel();
                    let _ = pending.response_tx.send(DebugResponse::error(
                        "screenshot timed out (renderer did not respond)",
                    ));
                    self.pending_screenshot = None;
                }
            } else {
                // No ScreenshotBridge — renderer not initialized yet.
                let _ = pending.response_tx.send(DebugResponse::error(
                    "screenshot not available (renderer not initialized)",
                ));
                self.pending_screenshot = None;
            }
        }

        // Drain new commands.
        let commands = {
            let mut q = self.queue.lock().unwrap();
            if q.is_empty() {
                return;
            }
            std::mem::take(&mut *q)
        };

        for cmd in commands {
            // Handle screenshot requests specially — they span multiple frames.
            if let DebugRequest::Screenshot { ref path } = cmd.request {
                if self.pending_screenshot.is_some() {
                    let _ = cmd
                        .response_tx
                        .send(DebugResponse::error("screenshot already in progress"));
                    continue;
                }

                match world.try_resource::<ScreenshotBridge>() {
                    Some(bridge) => {
                        // #1006 — owner-tagged claim. If the CLI
                        // `--screenshot` path is already in-flight,
                        // reject with a precise error so the user
                        // knows which consumer owns the bridge.
                        if !bridge.try_claim(SCREENSHOT_OWNER_DEBUG_SERVER) {
                            let owner = bridge.current_owner();
                            let owner_label = if owner == SCREENSHOT_OWNER_CLI {
                                "CLI --screenshot"
                            } else {
                                "another debug-server request"
                            };
                            let _ = cmd.response_tx.send(DebugResponse::error(format!(
                                "screenshot bridge already claimed by {} — retry later",
                                owner_label
                            )));
                            continue;
                        }
                        self.pending_screenshot = Some(PendingScreenshot {
                            response_tx: cmd.response_tx,
                            save_path: path.clone(),
                            frames_waited: 0,
                            cancel: cmd.cancel,
                        });
                    }
                    None => {
                        let _ = cmd.response_tx.send(DebugResponse::error(
                            "screenshot not available (renderer not initialized)",
                        ));
                    }
                }
                continue;
            }

            let response = evaluator::evaluate(world, &self.registry, &cmd.request);
            let _ = cmd.response_tx.send(response);
        }
    }

    fn name(&self) -> &str {
        "debug_drain_system"
    }
}
