//! Game-profile loading, re-exported from `byroredux-game-detect`.
//!
//! The loader moved out of the binary so the launcher reads the same registry
//! the engine launches from (`docs/engine/launcher.md` §12 Q6). This shim
//! keeps `crate::game_profiles::…` working for every existing call site.

pub use byroredux_game_detect::profiles::{
    load_default, load_launch_defaults, resolve_games_root, resolve_profile_root,
};
