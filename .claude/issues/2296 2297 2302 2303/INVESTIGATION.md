# #2297 investigation notes — stale premise corrected

The issue's "Evidence" quoted `draw_command_eligible_for_tlas` as:
```rust
pub(super) fn draw_command_eligible_for_tlas(draw_cmd: &DrawCommand) -> bool {
    draw_cmd.in_tlas
        && !draw_cmd.is_water
        && draw_cmd.material_kind != crate::MATERIAL_KIND_EFFECT_SHADER
}
```
That shape **never existed** in this function's git history (`git log -S` across
every commit that ever touched it — `8ee3a749` created it as
`draw_cmd.in_tlas && !draw_cmd.is_water`, `29e9f450` only moved it during a module
split; no material_kind check was ever present). The `material_kind !=
MATERIAL_KIND_EFFECT_SHADER` string does exist in the repo's history, but in a
different function/era, not this one — the audit that produced this finding
appears to have conflated two different sites.

**Current real architecture**: `MATERIAL_KIND_FIRE_REFRACTION` IS already
excluded from the TLAS today, via `render::static_meshes`'s `in_tlas` computation
(`byroredux/src/render/static_meshes.rs:219-222`) — the single production
producer of `in_tlas == true` draws for real mesh geometry (static and skinned
both flow through this one loop). Its comment is explicit that
`MATERIAL_KIND_EFFECT_SHADER` is **deliberately retained** in the TLAS ("optical/GI
rays can see them while opaque shadow masks cannot"), confirmed independently by
the pre-existing test `effect_shader_surface_is_tlas_eligible_for_optical_rays`
in `acceleration/tests.rs`. Excluding EFFECT_SHADER in
`draw_command_eligible_for_tlas` — what the issue's evidence implied "fixing"
would require — would have been a real regression against that documented,
tested design decision.

**What was actually applied**: added ONLY the `MATERIAL_KIND_FIRE_REFRACTION`
exclusion to `draw_command_eligible_for_tlas`, as genuine defense-in-depth
(matching the issue's own LOW-severity "no live defect, defense-in-depth gap
only" framing) — it's currently redundant with the `static_meshes.rs` producer
gate, but protects against a future producer that forgets to compute `in_tlas`
correctly for this material kind, mirroring the existing `is_water` precedent in
the same function (which IS load-bearing — water.rs sets `in_tlas: true`
unconditionally and relies entirely on this predicate). Did NOT add an
EFFECT_SHADER exclusion.
