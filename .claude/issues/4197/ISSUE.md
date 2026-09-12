# PERF-D3-2026-09-11-02: `flush_pending_uploads` holds every staged texture's staging buffer live at once, and the flush trigger is a texture count, not a byte budget

Labels: medium,performance,renderer,memory,bug

**Description**: Peak host-visible staging residency for one flush is the sum of that batch's staging sizes (each `StagedUpload`'s guard is returned to the pool only after submit+fence completes). `StagingPool`'s budget is enforced on `release`, not `acquire`, so an over-budget batch simply allocates fresh buffers. The batching policy (`should_flush_pending_cell_textures`) fires at a texture *count* of 64, a floor not a ceiling — 64 x a 4096^2 BC7 mip chain (~22 MB) is ~1.4 GB of simultaneous staging plus ~1.4 GB of retained `dds_bytes` in host RAM, versus 64 x a 512^2 BC1 (~0.17 MB) at ~11 MB — two orders of magnitude under the same policy decision. Two forced flush call sites have no threshold at all.

**Evidence**:
`crates/renderer/src/texture_registry/upload.rs:384-390,448-473,540-544`, `byroredux/src/cell_loader/references/mod.rs:779-788` (`YIELDED_TEXTURE_UPLOAD_BATCH_MIN = 64`), `crates/renderer/src/vulkan/buffer.rs:115-124` ("the budget only bounds *retained* size, not in-flight allocations"), `texture_registry/mod.rs:76-85,258`.

**Impact**: A transient host-visible VRAM/BAR spike proportional to a cell's texture bytes, unbilled in `memory-budget.md`'s VRAM roll-up, competing with the BLAS budget's `screen_scaled_reservation_bytes` (which doesn't know about it) on a 6 GB card during a texture-heavy cell load. Fails gracefully (dropped upload -> checkerboard texture), not a crash.

**Related**: #881/CELL-PERF-03, #239, #1922, `memory-budget.md` "Not yet ledgered" section.

**Suggested Fix**: Add a byte accumulator to the enqueue path; make the flush trigger `count >= 64 || bytes >= BUDGET` sized against `DEFAULT_STAGING_BUDGET_BYTES`. Chunk the forced-completion flush into bounded byte-sized sub-batches.



## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, other systems)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **TESTS**: A regression test pins this specific fix



*Filed via /audit-publish from `docs/audits/AUDIT_PERFORMANCE_2026-09-11.md`.*
