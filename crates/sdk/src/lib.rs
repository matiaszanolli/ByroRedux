//! Public tooling APIs for building ByroRedux-powered content applications.
//!
//! The first surface is [`studio`]: renderer-independent document state,
//! scene fitting, selection, snapshots, and typed edit commands. Hosts decide
//! how assets are imported and rendered; UI layers only consume snapshots and
//! emit commands.

#![forbid(unsafe_code)]

pub mod actor_values;
pub mod animation;
pub mod compatibility;
pub mod component;
pub mod console;
pub mod content;
pub mod event;
pub mod factions;
pub mod identity;
pub mod inventory;
pub mod legacy_containers;
pub mod manifest;
pub mod packages;
pub mod perks;
pub mod projection;
pub mod relationships;
pub mod reputation;
pub mod script_function;
pub mod service;
pub mod settings;
pub mod spatial;
pub mod storage;
pub mod studio;
