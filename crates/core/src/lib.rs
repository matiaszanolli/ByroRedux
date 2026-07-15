//! Core ECS, math, and shared types for ByroRedux.
//!
//! This is the foundation crate every other crate builds on: the ECS World
//! (components, storage backends, queries, scheduler, resources — see
//! [`ecs`]), interned strings ([`string`]), stable content-addressed Form
//! IDs ([`form_id`]), and glam-backed math ([`math`]). It has no knowledge
//! of Vulkan, NIF, or any specific game format — those live in downstream
//! crates (`renderer`, `nif`, `plugin`, …) that depend on this one.
//!
//! See `docs/engine/ecs.md` and `docs/engine/architecture.md` for the design
//! rationale (interior mutability via `RwLock`, `PackedStorage` vs
//! `SparseSetStorage`, TypeId-sorted lock acquisition).

pub mod animation;
pub mod character;
pub mod combat;
pub mod console;
pub mod ecs;
pub mod form_id;
pub mod math;
pub mod stealth;
pub mod string;
pub mod types;
