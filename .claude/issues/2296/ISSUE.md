# MAT-D1-NEW-01: no cross-crate assert pins NIF importer's material_kind literals to byroredux_renderer::MATERIAL_KIND_*

Source: `docs/audits/AUDIT_NIFAL_2026-08-03.md`

**Severity**: LOW
**Dimension**: Material · **Tier Violated**: single-boundary
**Location**: `crates/nif/src/import/material/dedicated_shader.rs:336,485` and `crates/nif/src/import/material/legacy_properties.rs:407` (literal `material_kind = 101/102/103` assignments), vs `byroredux_renderer::MATERIAL_KIND_*` constants (e.g. `crates/renderer/src/shader_constants_data.rs:81`)
**Status**: NEW

## Description

No cross-crate assert pins the NIF importer's `material_kind` 101/102/103
literals to `byroredux_renderer::MATERIAL_KIND_*`. `crates/nif`'s
`Cargo.toml` depends only on `byroredux-core`, `log`, and `thiserror` — it has
no dependency on `byroredux-renderer` — so the only asserts that exist today
are literal-to-literal inside the producing crate (e.g.
`lighting_shader_pbr_tests.rs:180` asserts `info.material_kind == 103`, a
literal, not `byroredux_renderer::MATERIAL_KIND_FIRE_REFRACTION`). A future
renumber of the renderer-side constants would keep `cargo test -p
byroredux-nif` green while silently dropping every effect/no-lighting/fire-haze
surface to the default-lit arm.

## Evidence

`crates/nif/Cargo.toml`'s `[dependencies]` block: `byroredux-core`, `log`,
`thiserror` only — no `byroredux-renderer`. `MATERIAL_KIND_FIRE_REFRACTION`
is independently defined as `103u32` at both
`crates/renderer/src/shader_constants_data.rs:81` and
`crates/renderer/src/vulkan/scene_buffer/constants.rs:336`, with no shared
source of truth reachable from `crates/nif`.

## Impact

Latent drift risk — no live defect today (the literals currently agree with
the renderer constants), but nothing in the type system or test suite would
catch a future renumber before it shipped and silently misrouted shading.

## Suggested Fix

A two-line cross-crate assert in `byroredux/src/material_translate.rs`'s test
module (the crate that *does* depend on both `byroredux-nif` and
`byroredux-renderer`), pinning the raw literals used at import time to the
renderer's named constants.

## Completeness Checks
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix

## Filed as

GitHub issue #2296, labels: low, nif-parser, renderer, bug.
