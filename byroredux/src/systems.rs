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
mod debug;
mod particle;
mod water;
mod weather;

pub(crate) use animation::*;
pub(crate) use audio::*;
pub(crate) use billboard::*;
pub(crate) use bounds::*;
pub(crate) use camera::*;
pub(crate) use debug::*;
pub(crate) use particle::*;
pub(crate) use water::*;
pub(crate) use weather::*;
