//! Tiny argv parsers used by `App::new` and `main`. Free functions
//! split out of `main.rs` to stay below the 2000-LOC ceiling
//! (TD9-NEW-01 / #1267).

use std::sync::OnceLock;

/// Process-wide effective args list. Phase 20 / 20.1 — main()
/// computes the expanded args (after `--game <key>` expansion)
/// once at startup and seeds this slot via
/// [`set_effective_args`]. Every site that previously called
/// `std::env::args().collect()` now reads through
/// [`effective_args`] so the expansion is universal — without
/// this indirection, scene loading + asset providers re-read
/// the raw argv and lose the synthesized `--esm` / `--bsa` /
/// etc. flags. See the Phase-20.1 commit for the
/// debugging journey.
static EFFECTIVE_ARGS: OnceLock<Vec<String>> = OnceLock::new();

/// Store the post-expansion args list. Call exactly once at
/// program start, after `--game` expansion. Re-call panics —
/// the singleton is set-once for the lifetime of the process.
pub fn set_effective_args(args: Vec<String>) {
    EFFECTIVE_ARGS
        .set(args)
        .expect("set_effective_args called more than once");
}

/// Read the effective args list. Falls back to
/// `std::env::args()` when the singleton hasn't been seeded —
/// preserves behaviour for unit tests / dev paths that don't
/// run through `main`.
pub fn effective_args() -> Vec<String> {
    if let Some(args) = EFFECTIVE_ARGS.get() {
        return args.clone();
    }
    std::env::args().collect()
}

pub fn parse_string_arg(args: &[String], flag: &str) -> Option<String> {
    args.iter()
        .position(|a| a == flag)
        .and_then(|i| args.get(i + 1).cloned())
}

/// Parse `x,y,z` into a `(f32, f32, f32)` tuple — stored as a plain
/// triple here to avoid leaking the renderer's `Vec3` into main.rs.
pub fn parse_vec3_arg(args: &[String], flag: &str) -> Option<(f32, f32, f32)> {
    let s = parse_string_arg(args, flag)?;
    let parts: Vec<f32> = s.split(',').filter_map(|p| p.trim().parse().ok()).collect();
    match parts.as_slice() {
        [x, y, z] => Some((*x, *y, *z)),
        _ => {
            log::warn!("`{flag} {s}` could not be parsed as x,y,z floats; ignoring");
            None
        }
    }
}
