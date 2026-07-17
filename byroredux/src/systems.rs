//! ECS systems for the application — organised by subsystem.
//!
//! Each submodule owns one system (plus any pure helpers + regression
//! tests that pin that system's contract). Items are re-exported here
//! so call sites in `main.rs` can keep importing
//! `crate::systems::{name}` without caring which file a given system
//! lives in.

mod animation;
mod audio;
mod billboard;
mod bounds;
mod camera;
mod character;
mod debug;
mod escort;
mod follow;
mod guard;
mod light_anim;
mod locomotion;
mod metrics;
mod particle;
mod patrol;
mod sandbox;
mod travel;
mod wander;
mod water;
// `pub(crate)` so the EXAL bootstrap (`scene::world_setup`) can seed the
// initial sun direction from the same `compute_sun_arc` model the per-frame
// system runs (EXAL step 4).
pub(crate) mod weather;

pub(crate) use animation::*;
pub(crate) use audio::*;
pub(crate) use billboard::*;
pub(crate) use bounds::*;
pub(crate) use camera::*;
pub(crate) use character::*;
pub(crate) use debug::*;
pub(crate) use escort::*;
pub(crate) use follow::*;
pub(crate) use guard::*;
pub(crate) use light_anim::*;
pub(crate) use metrics::*;
pub(crate) use particle::*;
pub(crate) use patrol::*;
pub(crate) use sandbox::*;
pub(crate) use travel::*;
pub(crate) use wander::*;
pub(crate) use water::*;
pub(crate) use weather::*;
