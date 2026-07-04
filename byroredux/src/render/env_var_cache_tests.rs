//! Regression for PERF-D1-NEW-02 / #1802. `build_render_data`'s `profile`
//! flag and `collect_static_mesh_draws`'s `no_cull` flag both used to call
//! `std::env::var_os(...)` fresh on every invocation — i.e. every frame.
//! Env vars can't change mid-process, so the sibling `apply_fog_overrides`
//! already cached its `getenv` results behind a `OnceLock`; these two sites
//! didn't.
//!
//! A live behavioral test would need to toggle the env var between calls,
//! but `OnceLock::get_or_init` only ever runs its closure once for the
//! life of the process — the first test (in any order, under parallel
//! test execution) to touch either flag would permanently decide the
//! cached value for every other test in the binary. That makes an
//! env-mutation test inherently flaky/order-dependent rather than a
//! meaningful regression guard. A static source assertion pins the actual
//! invariant instead: the per-frame call site must route through a
//! `OnceLock`, not a bare `env::var_os`.

/// `build_render_data`'s `profile` flag must be `OnceLock`-cached, not a
/// bare per-call `env::var_os`.
#[test]
fn render_mod_profile_flag_is_once_lock_cached() {
    let src = include_str!("mod.rs");
    let once_lock_pos = src
        .find("static PROFILE: OnceLock<bool> = OnceLock::new();")
        .expect("`profile` must be cached behind a `OnceLock<bool>` (#1802)");
    let get_or_init_pos = src
        .find(r#"PROFILE.get_or_init(|| std::env::var_os("BYRO_PROFILE").is_some())"#)
        .expect("the cached value must come from `PROFILE.get_or_init` (#1802)");
    assert!(
        once_lock_pos < get_or_init_pos,
        "the `OnceLock` declaration must precede its `get_or_init` call"
    );

    // A bare `env::var_os("BYRO_PROFILE")` call OUTSIDE the cache closure
    // would silently reintroduce the per-frame `getenv` this fix removes.
    // The only occurrence of the literal call must be the one inside
    // `get_or_init`'s closure (i.e. exactly one occurrence in the file).
    let occurrences = src.matches(r#"env::var_os("BYRO_PROFILE")"#).count();
    assert_eq!(
        occurrences, 1,
        "`env::var_os(\"BYRO_PROFILE\")` must appear exactly once (inside the \
         OnceLock's get_or_init closure) — a second bare call would defeat the cache"
    );
}

/// `collect_static_mesh_draws`'s `no_cull` flag must be `OnceLock`-cached
/// the same way.
#[test]
fn static_meshes_no_cull_flag_is_once_lock_cached() {
    let src = include_str!("static_meshes.rs");
    let once_lock_pos = src
        .find("static NO_CULL: OnceLock<bool> = OnceLock::new();")
        .expect("`no_cull` must be cached behind a `OnceLock<bool>` (#1802)");
    let get_or_init_pos = src
        .find(r#"NO_CULL.get_or_init(|| std::env::var_os("BYRO_NO_CULL").is_some())"#)
        .expect("the cached value must come from `NO_CULL.get_or_init` (#1802)");
    assert!(
        once_lock_pos < get_or_init_pos,
        "the `OnceLock` declaration must precede its `get_or_init` call"
    );

    let occurrences = src.matches(r#"env::var_os("BYRO_NO_CULL")"#).count();
    assert_eq!(
        occurrences, 1,
        "`env::var_os(\"BYRO_NO_CULL\")` must appear exactly once (inside the \
         OnceLock's get_or_init closure) — a second bare call would defeat the cache"
    );
}
