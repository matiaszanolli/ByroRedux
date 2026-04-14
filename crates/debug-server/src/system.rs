//! Late-stage exclusive system that drains the debug command queue.
//!
//! Runs after all other systems, with exclusive access to the World.
//! Processes pending debug requests and sends responses back to clients.

use crate::evaluator;
use crate::listener::CommandQueue;
use byroredux_core::ecs::system::System;
use byroredux_core::ecs::world::World;
use byroredux_debug_protocol::registry::ComponentRegistry;

/// The drain system that processes debug commands each frame.
///
/// Stored in the scheduler as an exclusive Late-stage system.
/// The component registry is owned by this system (not a World resource)
/// to avoid coupling debug-protocol with the core Resource trait.
pub struct DebugDrainSystem {
    queue: CommandQueue,
    registry: ComponentRegistry,
}

impl DebugDrainSystem {
    pub(crate) fn new(queue: CommandQueue) -> Self {
        Self {
            queue,
            registry: ComponentRegistry::new(),
        }
    }

    /// Access the registry for component registration during setup.
    pub fn registry_mut(&mut self) -> &mut ComponentRegistry {
        &mut self.registry
    }
}

impl System for DebugDrainSystem {
    fn run(&mut self, world: &World, _dt: f32) {
        // Fast path: check without draining if queue is empty.
        let commands = {
            let mut q = self.queue.lock().unwrap();
            if q.is_empty() {
                return;
            }
            std::mem::take(&mut *q)
        };

        for cmd in commands {
            let response = evaluator::evaluate(world, &self.registry, &cmd.request);
            // Ignore send errors — client may have disconnected.
            let _ = cmd.response_tx.send(response);
        }
    }

    fn name(&self) -> &str {
        "debug_drain_system"
    }
}
