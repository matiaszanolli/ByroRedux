# SAFE-2026-08-20-04: if both the water-caustic accumulator and its 1x1 placeholder sink fail to create, set 2 binding 0 is never written — and bind_pass binds it anyway for an imageAtomicAdd

**Issue**: #3138 — https://github.com/matiaszanolli/ByroRedux/issues/3138
**Finding**: `SAFE-2026-08-20-04`
**Labels**: bug, medium, vulkan, safety
**Filed**: 2026-08-20 (comprehensive `/audit-suite` sweep, 25 reports)

---

**Audit**: `docs/audits/AUDIT_SAFETY_2026-08-20.md` — Dimension 5 (Vulkan spec compliance) + Dimension 3 (resource lifecycle)
**Severity**: MEDIUM · **Status**: NEW

## Location
- `crates/renderer/src/vulkan/context/mod.rs:2283-2296` — placeholder creation, `None` on failure
- `crates/renderer/src/vulkan/context/mod.rs:2355-2372` — the descriptor-wiring block that silently no-ops when `views` is `None`
- Consumed at `crates/renderer/src/vulkan/water.rs:590-629` (`bind_pass`, unconditional) and `crates/renderer/shaders/water.frag:1245` (`imageAtomicAdd`)

## Description
#2142 closed the case where `WaterCausticAccum::new` fails, by adding a 1×1 `PlaceholderImage` storage sink to rebind to. The comment it left behind states the hazard precisely:

> `record_draw` binds set 2 unconditionally and the shader now *writes* it via `imageAtomicAdd`, so leaving the descriptor unwritten (init) or pointing at a destroyed view (resize failure) is an atomic write to freed memory, not a harmless no-op.

The placeholder's own creation is likewise fallible and likewise degrades to `None` with only a `log::warn!`. When **both** are `None`, `views` is `None`, the `if let Some(views)` guard skips the descriptor write entirely — and nothing downstream disables the water pipeline. `self.water` is still `Some`, so `geometry_pass.rs:521-537` binds and draws.

The only gate on the water loop is `water.params_ready(frame)`, which is about the *UBO* (binding 1, written unconditionally in `new()` at `:466`), not the storage image (binding 0).

## Evidence
```rust
// context/mod.rs:2283-2296 — placeholder failure degrades to None
let placeholder_caustic_sink = match super::placeholder::PlaceholderImage::new_storage_sink(…) {
    Ok(p) => Some(p),
    Err(e) => { log::warn!("Caustic-sink placeholder creation failed: {e} — water set 2 \
                            has no fallback if the accumulator drops out"); None }
};
```
```rust
// context/mod.rs:2356-2372 — both None ⇒ no write, and no compensating action
let views: Option<Vec<vk::ImageView>> = match water_caustic_accum.as_ref() {
    Some(accum) => Some(…),
    None => placeholder_caustic_sink.as_ref().map(|p| vec![p.view; MAX_FRAMES_IN_FLIGHT]),
};
if let Some(views) = views { w.update_water_caustic_descriptors(&device, &views); }
// <- no `else { water = None; }`
```
```rust
// water.rs:621-628 — set 2 bound unconditionally
device.cmd_bind_descriptor_sets(cmd, GRAPHICS, self.pipeline_layout, 2,
                                &[self.water_caustic_descriptor_sets[frame]], &[]);
```
```glsl
// water.frag:187, :1245 — and written
layout(set = 2, binding = 0, r32ui) uniform uimage2D waterCausticAccum;
imageAtomicAdd(waterCausticAccum, q, fixedVal);
```
The warn text itself names the residual — "water set 2 has **no fallback** if the accumulator drops out" — which is accurate, and the code then proceeds as if it did.

All three sites verified present at HEAD; the `if let Some(views)` at `context/mod.rs:2369` still has no `else` arm.

Per the No-Speculative-Vulkan-Fixes rule this is derived from the code and the spec's VUID text, not from an observed validation-layer message.

## Impact
Requires both an accumulator allocation failure and a 1×1 image allocation failure in the same session — realistically device-OOM, so this is narrow.

But the consequence when it fires is a draw against a never-written `VkDescriptorSet` slot, i.e. **VUID-vkCmdDrawIndexed-None-08114**, and an atomic write through an undefined descriptor: undefined behaviour, plausibly `VK_ERROR_DEVICE_LOST`, on a machine that was merely low on memory. Severity is about impact, not likelihood; the narrow trigger is what holds this at MEDIUM rather than HIGH.

## Related
#2141 / #2142 (both CLOSED) built the placeholder mechanism this finding says has one unhandled arm. The AO placeholder immediately above (`:2264-2282`) has the same shape but is benign — scene binding 7 is *sampled*, not written, and the shader tolerates a stale bind.

## Suggested fix
In the `views == None` arm, set `water = None` (and log at `error!` rather than `warn!`): the water pipeline is already designed to be optional — `context/mod.rs:1513` documents "draw site is gated on `Some` so a failure simply skips water" — so this reuses the existing degradation path rather than adding one.

Alternatively make `PlaceholderImage::new_storage_sink` failure fatal to `WaterPipeline::new`, which keeps the invariant "if `self.water` is `Some`, set 2 is fully written" locally checkable.

## Completeness Checks
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct — dropping `water` at init must not strand the param UBOs or their `SharedAllocator` clone
- [ ] **SIBLING**: Every other placeholder-backed descriptor in `context/mod.rs` checked for the same both-`None` arm (the AO sibling at `:2264-2282` is benign — record why)
- [ ] **TESTS**: A regression test pins this specific fix — the invariant "if `self.water` is `Some`, set 2 binding 0 has been written" should be checkable without a device
