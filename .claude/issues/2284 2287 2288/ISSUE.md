# Batch: #2284, #2287, #2288

## #2284 — MAT-D1-NEW-04: six BSLightingShaderProperty shading scalars dropped at the canonical Material boundary

**Severity**: MEDIUM
**Domain**: nif/core/renderer (canonical `Material` boundary) — `byroredux-core` + `byroredux`

`lighting_effect_1`, `lighting_effect_2`, `subsurface_rolloff`, `rimlight_power`,
`backlight_power`, `fresnel_power` were captured on `ImportedMaterial` at NIF
import time (`#1241`, 2026-05-23) but had zero consumers past that point: no
field existed on the canonical `Material` (`crates/core/src/ecs/components/
material.rs`), so `byroredux/src/material_translate.rs::translate_material`
had nothing to copy them into. Skin/hair/cloth materials authoring non-default
rim-lighting, backlight, subsurface-rolloff, or Fresnel-exponent values
rendered with the engine's fixed response instead of the author's tuned curve.

### Investigation
Checked the BGSM merge path (`asset_provider/material.rs`) for a sibling gap —
none found: BGSM's own format has no fields for `subsurface_rolloff`/
`rimlight_power`/`backlight_power` (FO4+ inline-shader-only concepts), and
`fresnel_power`/`grayscale_to_palette_scale` are already forwarded correctly
from BGSM to `ImportedMaterial`. The only missing link was the
`ImportedMaterial` → `Material` copy.

### Fix (scoped to the issue's own "minimal" suggested fix)
- Added the 6 fields to canonical `Material`, documented as "captured, not
  yet shaded" — explicitly not wiring a `GpuMaterial`/`triangle.frag`
  consumer in this change, matching the existing (if imperfectly-realized)
  `grayscale_to_palette_scale` precedent. A GPU-side shading consumer is
  follow-up work, deliberately kept out of this fix to avoid an unreviewable
  speculative shader change with no way to verify visually in this
  environment (no Vulkan device, per project policy on Vulkan changes).
- Wired the copy in `translate_material`.
- Corrected `docs/engine/nifal.md`'s "Materials — converged" verdict to note
  the residual gap and this fix.
- Added `translate_material_copies_bslsp_shading_scalars` pinning the copy.

## #2287 — SCR-D6-NEW5-01: ScenePackagePlayback's MoveTo action never completes once its actor is despawned

**Severity**: MEDIUM
**Domain**: scripting (`byroredux-scripting`)
**Location**: `crates/scripting/src/package.rs` (`tick_command`'s `MoveTo` arm)

`tick_command`'s `MoveTo` arm returned `false` ("not yet complete") forever
whenever the actor's `Transform` couldn't be resolved — no fallback timeout
analogous to the sibling `TimedInteraction` leaf's `INTERACTION_FALLBACK_SECONDS`.
An actor despawned mid-travel (e.g. exterior cell-streaming unloading its cell)
permanently stalled any scene phase gated on `ending_actions_complete`.

### Fix
Added a `stall_seconds` counter to `ScenePackageCommand::MoveTo`, accumulated
whenever the `Transform` lookup misses and reset to `0.0` the moment it
resolves again. Once `stall_seconds` exceeds the new
`MOVE_STALL_TIMEOUT_SECONDS` (5s), `tick_command` reports the action complete
(with a `warn!` log line) instead of retrying forever. Added a regression test
alongside `resolves_template_and_moves_actor_to_authored_marker` covering the
despawn-mid-travel case.

## #2288 — SCR-D6-NEW5-02: FragmentExecutionQueue's WaitForActors3DLoaded has no retry cap

**Severity**: MEDIUM
**Domain**: scripting (`byroredux-scripting`)
**Location**: `crates/scripting/src/fragment.rs` (`FragmentExecutionQueue`,
`fragment_continuation_system`)

`fragment_continuation_system`'s `Actors3DLoaded` arm re-armed a suspended
fragment continuation forever whenever the target actors stayed unresolved —
no maximum retry count, no elapsed-time ceiling, unlike the sibling
`MAX_CASCADE` bound in `quest_fragment_dispatch_system`.

### Fix
Added an `elapsed_seconds` counter to `FragmentResumeCondition::
Actors3DLoaded`, accumulated on every failed re-poll and capped by a new
`MAX_ACTORS_3D_LOADED_WAIT_SECONDS` (30s). Once exceeded, the entry is
dropped outright (not requeued, not resumed) with a `warn!` log line,
matching the crate's "skip, never guess" contract. Added a regression test
pinning that a permanently-unresolved wait is evicted after the cap and
never fires its declined tail.
