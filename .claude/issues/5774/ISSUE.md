# REN-D3-2026-10-14-02: `GROUNDCOVER_BLADE_FADE_START` is a second independent 7000.0 with no `FADE_START < DRAW_DISTANCE` assertion

**Labels**: low,renderer,shaders,terrain-exterior,bug
**Source**: docs/audits/AUDIT_RENDERER_2026-10-14.md

Found by `/audit-renderer` (docs/audits/AUDIT_RENDERER_2026-10-14.md). Finding ID: `REN-D3-2026-10-14-02`. Status: NEW, confirmed against HEAD `adbd468e2`.

**Location**: `crates/renderer/src/shader_constants_data.rs` (`GROUNDCOVER_BLADE_FADE_START`, `GROUNDCOVER_DRAW_DISTANCE`)

## Description
Both fades use `smoothstep(START, GROUNDCOVER_DRAW_DISTANCE, d)`, which is undefined when START is not below the end. Nothing asserts `FADE_START < DRAW_DISTANCE`.

## Evidence
`shader_constants_data.rs:544`: `pub const GROUNDCOVER_BLADE_FADE_START: f32 = 7000.0;` (draw distance 8000 today).

## Impact
Fine today (7000 < 8000); a latent footgun on the next distance retune.

## Suggested Fix
Add a const assert, or derive START from the draw distance.

## Completeness Checks
- [ ] **TESTS**: A regression test pins this specific fix
