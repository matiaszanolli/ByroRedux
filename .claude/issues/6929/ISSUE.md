# #6929 — INC-2026-11-14-17: `ini_f32` accepts `inf`/`NaN`; an infinite `iMusicTransitionInMS` panics in `Duration::from_secs_f32`

**Source**: `docs/audits/AUDIT_INCREMENTAL_2026-11-14.md` (INC-17). Verified against HEAD `77eb7e6c0` at publish time.

- **Severity**: LOW
- **Location**: `byroredux/src/game_ini.rs` (`ini_f32`); `byroredux/src/music/translate.rs` (`Catalog::new`, `iMusicTransitionInMS`); `crates/audio/src/cache.rs` (`Duration::from_secs_f32(fade.max(0.0))`)
- **Changed in**: `game_ini.rs` (commit `45b3b0a64`)

## Description
`ini_f32` is a bare `.parse()`, which accepts `inf`, `infinity` and `NaN`. `iMusicTransitionInMS=inf` (or `1e39`) becomes an infinite fade; `inf.max(0.0)` is still infinite and `Duration::from_secs_f32` panics. A `NaN` volume passes `clamp(0.0, 1.0)` unchanged.

## Impact
A garbled local `*_default.ini` panics the music start (local config, not game-archive input).

## Suggested Fix
`.filter(|v| v.is_finite())` in `ini_f32`; add a non-finite test case.

## Completeness Checks
- [ ] **TESTS**: A regression test pins this specific fix
