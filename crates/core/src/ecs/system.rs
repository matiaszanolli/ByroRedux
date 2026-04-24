//! System trait — the unit of game logic.
//!
//! Systems operate on the World through queries. They take `&World`
//! (not `&mut World`) because mutation goes through `QueryWrite`,
//! which uses interior mutability via RwLock.

use super::access::Access;
use super::world::World;

/// A system that runs game logic against the world each frame.
///
/// Implement this for stateful systems (accumulators, cooldowns, etc).
/// For stateless logic, plain functions and closures work via the
/// blanket impl — no boilerplate needed.
pub trait System: Send + Sync {
    fn run(&mut self, world: &World, dt: f32);

    fn name(&self) -> &str {
        std::any::type_name::<Self>()
    }

    /// Optional declared access pattern (R7).
    ///
    /// When `Some(_)`, the [`crate::ecs::scheduler::Scheduler`] uses
    /// the declaration to compute a static conflict report — every
    /// pair of parallel-stage systems is classified as `None`,
    /// `Conflict`, or `Unknown` (when one or both sides are
    /// undeclared). The default `None` is intentional: existing
    /// systems and closures keep working unchanged, with conflict
    /// analysis falling back to the pessimistic "Unknown" classifier.
    ///
    /// To declare access from a closure (which can't override trait
    /// methods), use [`crate::ecs::scheduler::Scheduler::add_to_with_access`].
    fn access(&self) -> Option<Access> {
        None
    }
}

/// Blanket impl: any `Fn(&World, f32) + Send + Sync` is a System.
///
/// ```ignore
/// // Stateless closure:
/// scheduler.add(|world: &World, dt: f32| { /* ... */ });
///
/// // Stateful closure (FnMut — captures mutable state):
/// let mut counter = 0u32;
/// scheduler.add(move |_world: &World, _dt: f32| { counter += 1; });
/// ```
impl<F: FnMut(&World, f32) + Send + Sync> System for F {
    fn run(&mut self, world: &World, dt: f32) {
        self(world, dt);
    }
}
