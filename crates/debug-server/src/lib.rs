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
/// handle only after the loopback listener is bound; bind/setup failures are
/// returned without registering a drain system. Store a successful handle on
/// the App so its natural Drop signals shutdown and joins the listener thread
/// cleanly (#855 / C6-NEW-02).
///
/// Call this after all systems have been added to the scheduler so that
/// the SystemList resource is already populated.
#[must_use = "drop the returned DebugServerHandle to join the listener \
              on shutdown; discarding it detaches the thread"]
pub fn start(scheduler: &mut Scheduler, port: u16) -> std::io::Result<DebugServerHandle> {
    let (mut drain_system, handle) = listener::spawn(port)?;

    // Register all inspectable components into the drain system's registry.
    registration::register_all(drain_system.registry_mut());

    scheduler.add_exclusive(Stage::Late, drain_system);

    log::info!("Debug server listening on {}", handle.local_addr());
    Ok(handle)
}
