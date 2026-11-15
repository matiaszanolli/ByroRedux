# #6931 — INC-2026-11-14-19: Music `battle()` / `sting()` lack the `untranslated()` per-game decline; directives are deep-cloned every frame

**Source**: `docs/audits/AUDIT_INCREMENTAL_2026-11-14.md` (INC-19). Verified against HEAD `77eb7e6c0` at publish time.

- **Severity**: LOW
- **Location**: `byroredux/src/music/translate.rs` (`battle`, `sting`; compare `location_with` / `music_type`); `byroredux/src/music/controller.rs` (`play` clones the directive); `byroredux/src/music/mod.rs` (`current_place`)
- **Changed in**: commits `45b3b0a64`, `bf42dca77`

## Description
`location_with` and `music_type` consult `untranslated()`; `battle()` and `sting()` do not, so Skyrim/FO4/Starfield combat and death walk the FNV marker and region path and `sting()` scans every `SOUN` linearly. They resolve to Silent/Unresolved today, so nothing is audible — the explicit decline is missing. Separately, `play` deep-clones each layer's directive (`Vec<String>` playlist) every frame and `current_place` builds lowercased `String`s every frame.

## Suggested Fix
Gate both functions on `untranslated()`; borrow `&Directive` (or `Arc` the playlist); compare the place without allocating.

## Completeness Checks
- [ ] **TESTS**: A regression test pins this specific fix
