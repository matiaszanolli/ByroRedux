//! Debug server for ByroRedux — TCP listener + expression evaluator.
//!
//! Embeds into the engine as a Late-stage exclusive system that drains
//! a command queue between frames. Zero cost when no debugger is connected.

pub mod evaluator;
pub mod listener;
pub mod registration;
pub mod system;

use byroredux_core::ecs::scheduler::{Scheduler, Stage};

// Re-export core's SystemList so the evaluator can find it.
pub use byroredux_core::ecs::resources::SystemList;
pub use listener::DebugServerHandle;

/// Start the debug server: register components, spawn the TCP listener,
/// and add the drain system to the scheduler. Returns the shutdown-aware
/// handle — store it on the App so its natural Drop signals shutdown and
/// joins the listener thread cleanly (#855 / C6-NEW-02). Discarding the
/// handle detaches the listener and reverts to the pre-fix behaviour.
///
/// Call this after all systems have been added to the scheduler so that
/// the SystemList resource is already populated.
#[must_use = "drop the returned DebugServerHandle to join the listener \
              on shutdown; discarding it detaches the thread"]
pub fn start(scheduler: &mut Scheduler, port: u16) -> DebugServerHandle {
    let (mut drain_system, handle) = listener::spawn(port);

    // Register all inspectable components into the drain system's registry.
    registration::register_all(drain_system.registry_mut());

    scheduler.add_exclusive(Stage::Late, drain_system);

    // Hostname here mirrors `listener_loop`'s `TcpListener::bind`
    // hardcoded `127.0.0.1` — both must move in lockstep if a
    // future host arg lands. See #857.
    log::info!("Debug server listening on 127.0.0.1:{}", port);
    handle
}
