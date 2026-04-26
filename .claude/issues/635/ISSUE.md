# FNV-D3-LOW: NifImportRegistry not shrunk on cell unload + PKIN nested dispatch single-level

## Finding: FNV-D3-LOW (bundle of FNV-D3-05 + FNV-D3-06)

- **Severity**: LOW (both)
- **Source**: `docs/audits/AUDIT_FNV_2026-04-24.md`

## FNV-D3-05: unload_cell doesn't shrink NifImportRegistry — CPU-side scenes persist forever (LOW, design gap, M40 work)

**Location**: [byroredux/src/cell_loader.rs:188-321](byroredux/src/cell_loader.rs#L188-L321); doc comment at lines 73-77 acknowledges "unbounded for now".

`unload_cell` drops mesh / BLAS / texture GPU resources but the process-lifetime `NifImportRegistry` `Arc<CachedNifImport>` entries persist. Memory bound across a long playthrough is unbounded; ~14k FNV NIFs at hundreds of MB of CPU-side scene graph (vertex arrays, indices, collision shapes, particle emitters) survives.

GPU memory is reclaimed (good); CPU-side cache is not.

**Fix**: either (a) clear the registry inside `unload_cell` when the caller signals a hard reset, or (b) add an LRU cap surfaced via a `mesh.cache` debug command (already mentioned in the doc comment). Belongs in M40 doorwalking work.

## FNV-D3-06: PKIN expansion is single-level; nested PKIN/SCOL/LVLI silently miss (LOW, FO4-only impact)

**Location**: [byroredux/src/cell_loader.rs:1188-1228](byroredux/src/cell_loader.rs#L1188-L1228).

`expand_pkin_placements` returns leaf `child_form_id`s looked up via `index.statics.get()`. If the child is itself a PKIN, SCOL, or LVLI, the lookup misses and the placement is dropped at line 1226 with only a `log::debug!`.

PKIN is FO4+; FNV ships zero PKIN content, so this is a forward-looking gap when sharing this loader with FO4 cell loads.

**Fix**: hoist PKIN/SCOL/LVLI dispatch into a recursive helper bounded by depth (≤ 4) so PKIN-of-PKIN and SCOL-of-PKIN cases fan out properly. Defer until FO4 cell loads need it (M32+ exterior).

## Completeness Checks

- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: Check if `expand_lvli_placements` has the same single-level limitation (LVLI-of-LVLI is rare but possible in mods).
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: For D3-05 — load 100 cells in sequence; assert NifImportRegistry stays under a configured cap. For D3-06 — synthetic PKIN-of-PKIN; assert leaf placements all spawn.

_Filed from audit `docs/audits/AUDIT_FNV_2026-04-24.md`._
