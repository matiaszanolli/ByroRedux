# Issues 2247, 2248, 2249, 2250

## #2247 — REN-D20-01: A skipped debug-UI frame permanently drops that frame's egui texture delta

**Location**: `crates/renderer/src/vulkan/context/mod.rs:3251` (`submit_egui_frame` — plain overwrite of `egui_pending_output`); `crates/renderer/src/vulkan/context/draw.rs:2726` (`egui_pending_output.take()`, sole consumer)

`submit_egui_frame` overwrites `egui_pending_output` instead of accumulating. If `draw_frame` skips consumption on some iteration, the dropped frame's `textures_delta.set`/`.free` is lost forever — a missed texture upload permanently blanks part of the UI, and a missed free leaks that frame's orphaned textures for the session.

**Fix**: merge `textures_delta.set`/`.free` across `submit_egui_frame` calls instead of overwriting.

## #2248 — REN-D21-01: Cornell RT harness has no FogVolume probe and its global fog medium rounds to ~0 optical depth at Cornell scale

**Location**: `byroredux/src/cornell.rs`

No `FogVolume` entity is spawned in the Cornell scene, and the existing global fog medium is authored at Bethesda-cell scale, producing ~0 optical depth across the ~14-unit Cornell box — the same trap #1942 fixed for the sun path. `--cornell` gives a false all-clear for any fog regression.

**Fix**: add a `FogVolume` probe entity, and scale up (or Cornell-override) the global fog medium's extinction to produce measurable optical depth.

## #2249 — REN-D21-03: Cornell harness cannot exercise fire-refraction: mat.set has no ior field and Cornell probes carry no normal map

**Location**: `byroredux/src/commands/scene.rs:598-668` (`MatSetCommand` field match); `byroredux/src/cornell.rs` (no normal-map-carrying probe)

`mat.set` has no `ior`/`distortion_strength` field case (fire-refraction overloads `ior` as distortion strength per REN-D6-01), and no Cornell probe carries a normal map, so `tangentWarp = N - macroN * dot(N, macroN)` is structurally a no-op even at max authored strength. Every fire-refraction gap found this session (REN-D2-01/D11-02/D12-02) had to be found by static reading, not the harness.

**Fix**: add an `ior` case to `MatSetCommand`, and add a normal-map-carrying probe to the Cornell scene.

## #2250 — REN-D22-01: Session 62's shadow-policy flag decode bypasses the per-game canonicalization boundary and reads raw TES5 bit layout unconditionally

**Location**: `crates/core/src/ecs/components/light.rs:96-106` (`LIGHT_FLAG_SHADOW_*` — raw TES5 LIGH DATA flags); `byroredux/src/render/lights.rs:184` (consumes `light.flags & LIGHT_FLAG_SHADOW_MASK` directly)

Shadow-projection flag bits (0x400/0x800/0x1000) are read as raw TES5 bit positions across all 6 `GameKind` variants, with no per-game canonicalization boundary — unlike the sibling `canonical_light_animation_flags(game, source_flags)` in `byroredux/src/systems/light_anim.rs:47`, which explicitly branches per game. A game whose LIGH flag layout diverges from Skyrim's at these bit positions would silently decode the wrong shadow type.

**Fix**: add `canonical_light_shadow_flags(game, source_flags)` mirroring `canonical_light_animation_flags`, route `lights.rs`'s `casts_shadows` decode through it.

## Domain classification
- #2247: renderer (byroredux-renderer) — Vulkan/egui context
- #2248: renderer (byroredux-renderer) + binary (byroredux/src/cornell.rs) — Cornell harness lives in the binary crate
- #2249: binary (byroredux) — commands/scene.rs + cornell.rs, both binary-crate
- #2250: ecs (byroredux-core, light.rs) + binary (byroredux/src/render/lights.rs, systems/light_anim.rs)
