# REN-D1-01: AS↔SSBO index contract rests on a duplicated predicate, not on build_instance_map

- **Issue**: [#2913](https://github.com/matiaszanolli/ByroRedux/issues/2913)
- **Finding ID**: `REN-D1-01`
- **Labels**: `medium,renderer,vulkan,bug`
- **Source report**: [`docs/audits/AUDIT_RENDERER_2026-08-14.md`](../../../docs/audits/AUDIT_RENDERER_2026-08-14.md)
- **Run**: `/audit-suite rt-deep`, 2026-08-14, HEAD `205744ae`

> Immutable snapshot of the issue *as filed* (TD10-001 / #1156). GitHub is
> authoritative for current state — query `gh issue view 2913 --json state`.

---

- **Severity**: MEDIUM
- **Dimension**: AS Correctness
- **Location**: `crates/renderer/src/vulkan/context/draw.rs` (`draw_frame` — the `build_instance_map` call site and the `GpuInstance` builder loop), `crates/renderer/src/vulkan/acceleration/predicates.rs` (`build_instance_map`), `crates/renderer/src/vulkan/acceleration/tlas.rs` (`build_tlas_instances`)
- **Status**: NEW
- **Description**: `build_instance_map` is documented — in its own doc comment and at the call
  site — as "the single source of truth the TLAS `instance_custom_index` and the SSBO position
  must agree on". Only one of the two consumers actually reads it. `build_tlas_instances` indexes
  `instance_map[i]`; the SSBO builder in `draw_frame` never touches the map and instead recomputes
  the compacted position as `gpu_instances.len()` behind its own copy of the predicate. The two
  agree today purely because the predicate text is duplicated correctly and because the SSBO
  loop's only index-affecting `continue` is the identical `mesh_registry.get(…)` reject. That is
  the exact "two independent filter predicates" shape #419 was filed to eliminate, and the fix
  removed the divergence without removing the fragility. No assertion or test pins the
  equivalence.
- **Evidence**:
  - `draw.rs`, TLAS side: `build_instance_map(draw_commands.len(), MAX_INSTANCES, |i| self.mesh_registry.get(draw_commands[i].mesh_handle).is_some())`.
  - `draw.rs`, SSBO side, ~800 lines later: `for draw_cmd in draw_commands { let Some(mesh) = self.mesh_registry.get(draw_cmd.mesh_handle) else { continue; }; let instance_idx = gpu_instances.len() as u32; … }`.
  - The other two `continue`s in that loop both sit *after* `gpu_instances.push(…)` (batch-skip and batch-extend), so they are index-neutral **today**; a fourth `continue` inserted anywhere between the `mesh_registry.get` reject and the `push` would silently shift every later SSBO entry while the TLAS custom indices stayed put.
  - Blast radius on divergence is the severity table's `SSBO index mismatch → CRITICAL` row: every RT hit reads the wrong `GpuInstance` (wrong model matrix, wrong `material_id`, wrong `surface_id`).
  - `debug_assert_eq!(gpu_instances.len(), previous_models.len())` already exists at the upload site — the analogous map-vs-SSBO assert does not.
- **Impact**: No wrong pixels today. The exposure is that a CRITICAL-severity contract is held
  only by a duplicated predicate ~800 lines apart in one function, with no `cargo test`-visible
  guard; the failure is silent (garbage material/transform on RT hits, not a crash or validation
  error).
- **Related**: #419 (CLOSED, fix intact), #957 / #1392 (24-bit truncation guard), #194, #2116;
  `AUDIT_RENDERER_2026-08-12b.md` §"The AS ↔ SSBO index contract" verified the *values* agree at
  HEAD but proposed no pin.
- **Suggested Fix**: Add `debug_assert_eq!(gpu_instances.len(), instance_map.iter().flatten().count())`
  immediately before the UI-quad append in `draw_frame` (the UI quad is pushed after the loop, so
  the counts must match exactly at that point), and a unit test that walks a synthetic
  draw-command list through both compaction rules. Cheaper and more robust than threading the map
  into the SSBO loop.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, the sibling BLAS/TLAS path)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **TESTS**: A regression test pins this specific fix

---

*Filed by `/audit-publish` from [`docs/audits/AUDIT_RENDERER_2026-08-14.md`](docs/audits/AUDIT_RENDERER_2026-08-14.md) — `/audit-suite rt-deep`, 2026-08-14, HEAD `205744ae`. Verified CONFIRMED against current code at publish time.*
