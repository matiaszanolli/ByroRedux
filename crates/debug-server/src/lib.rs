//! Debug server for ByroRedux — TCP listener + expression evaluator.
//!
//! Embeds into the engine as a Late-stage exclusive system that drains
//! a command queue between frames. Zero cost when no debugger is connected.

pub mod evaluator;
pub mod listener;
pub mod registration;
pub mod system;

use byroredux_core::ecs::scheduler::{Scheduler, Stage};
use byroredux_core::ecs::world::World;

// Re-export core's SystemList so the evaluator can find it.
pub use byroredux_core::ecs::resources::SystemList;

/// Start the debug server: register components, spawn the TCP listener,
/// and add the drain system to the scheduler.
///
/// Call this after all systems have been added to the scheduler so that
/// the SystemList resource is already populated.
pub fn start(_world: &mut World, scheduler: &mut Scheduler, port: u16) {
    let (mut drain_system, _listener_handle) = listener::spawn(port);

    // Register all inspectable components into the drain system's registry.
    registration::register_all(drain_system.registry_mut());

    scheduler.add_exclusive(Stage::Late, drain_system);

    // Hostname here mirrors `listener_loop`'s `TcpListener::bind`
    // hardcoded `127.0.0.1` — both must move in lockstep if a
    // future host arg lands. See #857.
    log::info!("Debug server listening on 127.0.0.1:{}", port);
}
