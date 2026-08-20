# REN-D15-02: WaterPipeline's doc comment tells the reader the opposite of what resize.rs now does

**Issue**: #3126 — https://github.com/matiaszanolli/ByroRedux/issues/3126
**Labels**: `low,renderer,documentation`
**Filed**: 2026-08-20 · comprehensive audit suite
**Report**: `docs/audits/AUDIT_RENDERER_2026-08-20.md`

---

**Severity**: LOW
**Dimension**: Water / documentation accuracy
**Source**: `docs/audits/AUDIT_RENDERER_2026-08-20.md` (REN-D15-02)

## Location

`crates/renderer/src/vulkan/water.rs` — the doc comment above `pub struct WaterPipeline` (`:231-238` at HEAD)

## Description

The comment reads:

> *"Extent-independent: … no descriptor binds reference a fixed-extent resource … No `recreate_on_resize` method exists — and intentionally so … if water ever picks up such a resource (e.g. a dedicated caustic accumulator), wire the resize hook at that time."*

**Every clause is now false.** Water owns exactly the resource the comment names as the hypothetical trigger — the per-FIF screen-sized `R32_UINT` `WaterCausticAccum`, bound as set 2 via `update_water_caustic_descriptors` — and `crates/renderer/src/vulkan/context/resize.rs` already handles it.

## Evidence

`resize.rs` does all three things the comment says do not happen:

1. `if let Some(mut old) = self.water.take()` → `WaterPipeline::new(...)` — the pipeline **is** destroyed and rebuilt (it depends on the render pass).
2. The `#1255 / Phase C of #1210` block calls `wca.recreate_on_resize`.
3. The `if let Some(w) = self.water.as_ref()` block calls `update_water_caustic_descriptors`, rebinding set 2 to the new views.

Two tests already pin that shape:
- `water_caustic_rebind_is_not_gated_on_accumulator_presence`
- `init_path_water_set_2_falls_back_and_drops_the_stale_comment`

The comment is verbatim at HEAD:
```
crates/renderer/src/vulkan/water.rs:231:/// Extent-independent: viewport and scissor are dynamic state set per
crates/renderer/src/vulkan/water.rs:234:/// `recreate_on_resize` method exists — and intentionally so. Other
crates/renderer/src/vulkan/water.rs:237:/// resource (e.g. a dedicated caustic accumulator), wire the resize
```

## Impact

Documentation only, but of the **actively misleading** kind: the comment instructs a future reader to *add* a resize hook when a condition is met, and that condition has been met for some time by code the comment is unaware of. A reader trusting it would either duplicate the existing handling (double-recreating the accumulator) or conclude the accumulator is unhandled and go looking for a bug that does not exist.

## Suggested Fix

Rewrite the comment to state what is true — water owns a fixed-extent resource (`WaterCausticAccum`), the pipeline is destroyed and recreated by `recreate_swapchain`, and its set 2 is rebound there under a fallback that survives the accumulator going away — and point at the two guard tests that pin it.

## Related

- #1130 / `REN-D17-NEW-01` (the finding the comment was originally written to close)
- #2142 (the set-2 rebind fallback the comment is unaware of)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files — other "extent-independent, no resize hook needed" claims in the renderer that have since acquired screen-sized resources
- [ ] **TESTS**: A regression test pins this specific fix (the two named tests already exist — reference them from the corrected comment so the next edit sees them)
