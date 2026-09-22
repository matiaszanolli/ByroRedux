# REN-D1-2026-09-21-03: `fill_shadow_mask_census`'s #4518 comment describes an `AlphaBlend` divert cause and a pending field removal that no longer exist

**Labels**: low, renderer, vulkan, documentation, doc-rot

Filed via /audit-publish from docs/audits/AUDIT_RENDERER_2026-09-21.md.

**Severity**: LOW (stale comment) · **Dimension**: AS Correctness
**Location**: `crates/renderer/src/vulkan/context/telemetry.rs` `fill_shadow_mask_census` (~:429-435)
**Status**: NEW
**Verified against**: HEAD `f97775ca8`

## Description

The #4518 comment in `fill_shadow_mask_census` reads:

> `actor_diverted_alpha_blend` is no longer published: the divert returns `AlphaBlend` only for non-Actors (`mask_divert_cause`) while this census's breakdown is Actor-guarded, so the counter was structurally zero … Dropping the field itself spans the renderer census + core `ShadowMaskCensus` and is tracked separately.

Both halves are now stale:
- `f97775ca8` removed the blend divert. `MaskDivertCause` (`acceleration/predicates.rs`) has only `RefractiveGlass`, `EffectShader` and `FireRefraction`, and `mask_divert_cause` never returns an `AlphaBlend` cause.
- The field is already gone: `actor_diverted_alpha_blend` appears nowhere in the tree except this comment.

## Evidence

- `grep -rn "actor_diverted_alpha_blend" crates/renderer/src byroredux/src` matches only `context/telemetry.rs`, in the comment itself.
- `grep -n "AlphaBlend" crates/renderer/src/vulkan/acceleration/predicates.rs` finds no match.

## Impact

The comment at the census site is misleading. It tells a reader about a live divert cause and an open follow-up, and neither exists. No runtime effect.

## Related

- #4518 (closed): wrote this comment.
- REN-D1-2026-09-21-01 (#4576): `f97775ca8`'s divert removal, which made the comment's cause obsolete.

## Suggested Fix

Replace the comment with one line saying that both the alpha-blend divert and its census field were removed (`f97775ca8`, #4518), or delete it.

Source: docs/audits/AUDIT_RENDERER_2026-09-21.md (REN-D1-2026-09-21-03)

## Completeness Checks
- [ ] **SIBLING**: other comments naming the retired blend divert (`mask_divert_cause` doc, `rt.masks` display, the `tlas.rs` census block) checked in the same sweep
