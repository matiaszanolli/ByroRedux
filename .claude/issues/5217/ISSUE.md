# #5217 — REN-D9-2026-10-03-04: Doc rot in the skinning lane (bundle)

**Labels**: low,renderer,documentation,doc-rot
**Filed from**: `docs/audits/AUDIT_RENDERER_2026-10-03.md` (audit HEAD `f002763b4`)

- **Severity**: LOW
- **Dimension**: Skinning
- **Location**:
  - (a) `crates/renderer/src/vulkan/context/init.rs`, step 12d comment above `SkinComputePipeline::new`.
  - (b) `crates/renderer/src/vulkan/morph_compute.rs`, rustdoc on `MorphSlot::last_used_frame`.
- **Status**: NEW. (a) was re-wrapped by `7f6ab8e8f` (#4894) without the number being fixed. (b) predates the window; #4294 changed the behaviour but not this doc. Searched "32 skinned", "SKIN_MAX_SLOTS", "MorphSlot last_used_frame".
- **Description**:
  - **(a)** The comment says the slot ceiling matches "`MAX_TOTAL_BONES / MAX_BONES_PER_MESH = 32` skinned meshes". The value is `SKIN_MAX_SLOTS = (196608 / 144) - 1 = 1364`, and the module-level const doc in `context/mod.rs` says so. 32 was the pre-#900 value.
  - **(b)** The rustdoc says the stamp is "bumped every frame this entity appears in `draw_commands` (including skip-path entries)". Since #4294 it is stamped from entity liveness by `refresh_morph_slot_lru` → `refresh_live_slot_stamps`, so that a non-recreatable MorphSlot is never reaped from a live entity. The dispatch loop carries an explicit "#4294 — no `MorphSlot` LRU bump here".
- **Impact**: (b) points a future fixer back toward the draw-list stamping that #4294 removed as a bug. (a) misstates a capacity by about 40×.
- **Suggested Fix**: (a) Replace "= 32" with a pointer to `SKIN_MAX_SLOTS`. (b) Restate the doc as "stamped from entity liveness each frame (`VulkanContext::refresh_morph_slot_lru`, #4294); `0` is the never-stamped sentinel (`skin_lru_stamp` never writes it, #4969)".

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other call sites, other docs naming the same fact)
- [ ] **TESTS**: A regression test pins this specific fix (or a doc-pin / `_audit-validate.sh` check where one fits)
