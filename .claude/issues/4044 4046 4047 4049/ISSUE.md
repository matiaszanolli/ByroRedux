# Issues 4044, 4046, 4047, 4049

All four from `docs/audits/AUDIT_RENDERER_2026-09-06.md` (23-dimension `/audit-renderer` sweep at `229306ce`).

## #4044 — REN-2026-09-06-D6-04 (LOW, NIFAL Material)
`apply_emitter_overlays` is the single overlay boundary (#1513) and now routes
`effect_shader_flags` through it (#3589/`84c4a1df`), but `preset.greyscale_lut_index`
is still hand-copied identically at both spawn sites (`scene/nif_loader.rs` and
`cell_loader/spawn.rs`), each with a "Mirrored in the sibling site" comment.
Latent only (both copies byte-identical today) but the boundary's "single
overlay boundary" guarantee is false.
Fix: add an 11th parameter `greyscale_lut: Option<u32>` to
`apply_emitter_overlays`, move `preset.greyscale_lut_index = greyscale_lut.unwrap_or(0)`
inside it (leaving only the `resolve_texture` call, which needs `&mut VulkanContext`,
at the call sites — same shape #3589 used for `effect_shader`). Extend the two
existing overlay tests with the new field.

## #4046 — REN-2026-09-06-D8-02 (LOW, Denoiser/Composite)
`caustic_scene_key`/`caustic_scene_static` (#2468) fold in every visible light's
position/color/params so `caustic_history_valid` can detect a light change.
SVGF's `params.w` progressive-accumulation flag only gets `camera_static` —
per-pixel geometric rejection (mesh-id + normal cone) can't see a light that
changed color/intensity/position while the surface stayed still, so GI bounce
lags ~4s (256-frame time constant) after a light change even though the signal
to detect it is already computed one scope away.
Fix: thread `caustic_scene_static` (or equivalent) into the SVGF params.w
decision in `assemble_camera_and_lights.rs`, so progressive accumulation
requires camera AND light rig unchanged.

## #4047 — REN-2026-09-06-D8-03 (LOW, doc-rot, code is correct)
`.claude/commands/audit-renderer/SKILL.md` Dimension 8 checklist has two stale
claims: (1) "Fog applied to direct only, not indirect" — actually applied to
the fully reassembled `combined` (direct + indirect*albedo + caustic), which
is physically correct; (2) "Caustic accumulator sampled via usampler2D" — the
glass/MLP accumulator is actually `usampler2DArray` (3 layers), only the
water-side is `usampler2D`. Also: Dim 13 entry-point path stale — the
`(jx, jy)` jitter block moved to `context/assemble_camera_and_lights.rs`, not
`context/draw.rs`.
Fix: reword both Dim 8 bullets, correct Dim 13 path, run `_audit-validate.sh`.

## #4049 — REN-2026-09-06-D9-03 (LOW, Skinning)
The #3569 requeue makes a failed `bind_inverses` upload retry every frame
forever with an un-gated `log::warn!` — no `Once` gate, no rate limit, no
attempt counter — unlike every sibling failure path in the same file
(`failed_skin_slots` #900, `failed_skin_blas` #2802, `SkinSlotPool::overflow_warned`).
Fix: add a bounded-retry counter; after N consecutive failures stop requeuing
and log once, falling back to bind-pose rendering for the affected slots
(mirroring `overflow_attempt_count`'s one-shot-warn + silent counter shape).

## Domain
All renderer (`byroredux-renderer`), except #4044 which is split between
`byroredux-renderer` (VulkanContext, resolve_texture) and the binary crate
`byroredux` (`apply_emitter_overlays` lives in `byroredux/src/systems/particle.rs`,
called from `byroredux/src/scene/nif_loader.rs` + `byroredux/src/cell_loader/spawn.rs`).
#4047 is docs-only, no crate.
