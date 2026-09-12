# PERF-D6-2026-09-11-01: The bone-palette compute pass still dispatches the full dense slot range every frame, after #3665 narrowed its own input upload to dirty slots

Labels: medium,performance,renderer,bug

**Description**: Three of the four stages of the skin-preparation chain are dirty-narrowed (staging memcpy + `cmd_copy_buffer` walk `bone_world_copy_regions[frame]`, built from `dirty_slot_offsets` by #3665). The palette dispatch that consumes them was not narrowed with them: its extent is `bone_world_dispatch_bytes`, unconditionally the whole high-water array length (`(pool.max_used_slot() + 1) * MAX_BONES_PER_MESH`), regardless of how many entities actually moved. The only existing skip is all-or-nothing (`skip_skin_gpu_refresh` requires the *entire* pose-dirty set empty for `MAX_FRAMES_IN_FLIGHT` consecutive frames) — one twitching NPC anywhere in loaded cells re-arms the full-range recompute for every resident skinned entity.

**Evidence**:
`crates/renderer/src/vulkan/scene_buffer/upload.rs:301-303` (confirmed: `count` = high-water length, independent of dirty set); `crates/renderer/src/vulkan/context/dispatch_skin_and_cluster.rs:142-188`; `crates/renderer/shaders/skin_palette.comp:73-79` (`if (slot >= bone_count) return;` — the only bound check); `render/skinned.rs:148-149` (sizes at `(max_used_slot()+1) * MAX_BONES_PER_MESH`).

**Impact**: On the Freeside baseline (~677 live skinned entities -> ~97.5K palette slots), a frame with a single dirty actor still moves ~18 MB and issues ~1.5K workgroups — order 30-50 microseconds of the `skin_palette_ms` bracket, ~100% waste in the common case. Scales with *population*, the wrong axis for a streaming open-world renderer; blunts the payoff of the #3665/#1811 state machine.

**Related**: #3665 (the state machine to reuse), #1811 (all-idle skip), #1195 (per-entity gate this pass doesn't consume), #3676 (`skin_palette` timer that would measure the win).

**Suggested Fix**: Reuse `bone_world_copy_regions[frame]` as the palette work list instead of the dense extent — either one `cmd_dispatch` per region (few-regions case) or an index SSBO of region slot-bases (many-regions case), with the existing dense path kept behind a region-count threshold as a fallback.



## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, other systems)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **TESTS**: A regression test pins this specific fix



*Filed via /audit-publish from `docs/audits/AUDIT_PERFORMANCE_2026-09-11.md`.*
