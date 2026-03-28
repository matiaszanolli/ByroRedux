//! System trait — the unit of game logic.
//!
//! Systems operate on the World through queries. They take `&World`
//! (not `&mut World`) because mutation goes through `QueryWrite`,
//! which uses interior mutability via RwLock.

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
}

/// Blanket impl: any `Fn(&World, f32) + Send + Sync` is a System.
///
/// ```ignore
/// scheduler.add(|world: &World, dt: f32| {
///     // system logic here
/// });
/// ```
impl<F: Fn(&World, f32) + Send + Sync> System for F {
    fn run(&mut self, world: &World, dt: f32) {
        self(world, dt);
    }
}
