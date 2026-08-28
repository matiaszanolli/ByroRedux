# Issue #3459: NIFAL-2026-08-27-02: triangle.frag hard-codes Material::default()'s glass-optics values as its normalization pivots, when shader_constants.rs exists precisely to stop that drift

**Labels**: medium, nifal, shaders, renderer, bug
**Filed**: 2026-08-27 via /audit-publish

**Severity**: MEDIUM
**Dimension**: Material (Dim 1) / GPU contract (Dim 9)
**Tier Violated**: `no-fabrication` (a canonical default is restated, unguarded, downstream of the boundary)
**Game Affected**: all FO76 / Starfield / FO4 BGEM glass; and every engine-classified `MATERIAL_KIND_GLASS` surface on every game, since the pivot is applied unconditionally inside the `isGlass` branch
**Location**: `crates/renderer/shaders/triangle.frag:1498-1500` and `:1864`; the canonical values they restate are `crates/core/src/ecs/components/material.rs:536-539`; the mechanism they should use is `crates/renderer/src/shader_constants.rs:215-216`
**Source**: `docs/audits/AUDIT_NIFAL_2026-08-27.md` — NIFAL-2026-08-27-02

## Description

The BGEM v21+ glass tail reaches the shader correctly (all four scalars are in
the 432-byte `GpuMaterial` at offsets 364-388, mirrored in
`include/bindings.glsl:225-228`, and both maps are sampled). But the shader
normalizes two of them by literals that are copies of the canonical
`Material::default()` values:

```glsl
// crates/renderer/shaders/triangle.frag:1498-1500
float blurFactor = max(
    mat.glassBlurScale * mat.glassBlurScaleFactor, 0.0) / 0.4;

// crates/renderer/shaders/triangle.frag:1864
float deviationScale = clamp(mat.glassRefractionScale / 0.05, 0.0, 4.0);
```

`0.4` is `Material::default().glass_blur_scale` and `0.05` is
`Material::default().glass_refraction_scale`
(`crates/core/src/ecs/components/material.rs:537-538`). They are the *neutral*
pivots — the shader's own comment says so ("The format defaults … normalize to
the established glass result", `:1490-1494`) — so changing either default in
Rust silently shifts what "neutral" means on the GPU, with no compile error, no
layout assertion (the offsets are pinned, the *values* are not), and no test.

This codebase already solved this exact problem once. `shader_constants.rs`
emits the canonical water defaults as GLSL macros:

```rust
// crates/renderer/src/shader_constants.rs:215-216
("DEFAULT_WATER_WAVE_AMPLITUDE", format!("#define DEFAULT_WATER_WAVE_AMPLITUDE {DEFAULT_WATER_WAVE_AMPLITUDE:?}")),
("DEFAULT_WATER_WAVE_FREQUENCY", format!("#define DEFAULT_WATER_WAVE_FREQUENCY {DEFAULT_WATER_WAVE_FREQUENCY:?}")),
```

and the accompanying test asserts both that the macro value tracks
`WaterMaterial::default()` and that the shader divides by the macro rather than
a literal (`shader_constants.rs:693-705`) — the identical
divide-by-the-neutral-default shape.

## Evidence

`include/shader_constants.glsl:174-175` currently defines only the two water
macros; grepping the generated header for any glass or material default returns
nothing, while `triangle.frag` contains the two bare literals quoted above.

## Impact

No wrong value today — the four literals agree with `Material`. What is broken is
the guarantee, and it is the same guarantee `#3073` (parallax defaults duplicated
at six sites, still OPEN) is filed against, one tier further downstream: this copy
lives in GLSL, where neither `cargo test` nor `cargo check` can see it at all. A
future adjustment to the BGEM neutral (or a correction to the parsed format
default) changes the authored-to-rendered mapping for every glass surface in
every game while every test stays green.

## Related

- `#3073` (the CPU-side twin, OPEN)
- `#2589` (the precedent that fixed `grayscale_to_palette_scale` / `fresnel_power`
  neutral defaults rather than leaving `0.0` in the struct literals)
- `#2514`

## Suggested Fix

Export `DEFAULT_GLASS_BLUR_SCALE` and `DEFAULT_GLASS_REFRACTION_SCALE` from
`shader_constants.rs`, sourced from `Material::default()`, replace both literals
with the macros, and mirror the water test's two assertions (macro tracks the Rust
default; shader divides by the macro).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader defaults restated in GLSL — the full `triangle.frag` / `include/*.glsl` literal sweep)
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/mod.rs` (`extract_emitter_params` / `extract_emitter_rate`), per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix
