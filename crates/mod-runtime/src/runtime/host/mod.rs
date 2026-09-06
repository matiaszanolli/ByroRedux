//! One module per WIT host interface.
//!
//! Rust permits `impl Trait for Type` in any module of the owning crate,
//! so these 19 `impl <wit>::Host for HostState` blocks are a pure
//! relocation out of the old 3495-line `runtime.rs` (#3853) — no
//! signature changed.

mod actor_values;
mod animation;
mod console;
mod content_catalog;
mod context;
mod events;
mod faction_relationships;
mod factions;
mod inventory;
mod legacy_containers;
mod logging;
mod packages;
mod perks;
mod reputation;
mod script_functions;
mod state;
mod storage;
mod world_spatial;
mod world_state;
