**Severity**: MEDIUM
**Dimension**: Scene Graph Decomposition
**Source**: `docs/audits/AUDIT_LEGACY_COMPAT_2026-05-19.md` D1-NEW-03

`BSXFlags` is a Gamebryo extra-data block that flags the entire NIF (havok-managed, ragdoll, editor-marker, articulated, externally-emitted-particles). The parser reads it via `byroredux_nif::import::extract_bsx_flags(&scene)` (used at `cell_loader/references.rs:840` to filter editor-marker NIFs by bit 5). Beyond that filter, the bits never reach the ECS.

`crates/core/src/ecs/components/bsx.rs` exports `BSXFlags` (re-exported from `mod.rs:35`) but the component is unused outside its own tests.

### Impact
Future havok / ragdoll integration (M28 phase 3+), articulated-mesh animation wiring, and per-NIF debug introspection (`mesh.info` console command) re-derive what BSX already authoritatively says. Today only the editor-marker bit is honoured (and only inside the cell-load decision path, not as a component).

### Suggested Fix
Pass `bsx_flags: u32` through `CachedNifImport` so the spawn site can attach `BSXFlags(bits)` to the placement root. Audit downstream consumers that currently sniff for havok / ragdoll via heuristics.

### Completeness Checks
- [ ] **UNSAFE**: N/A.
- [ ] **SIBLING**: Loose-NIF spawn site `scene/nif_loader.rs`.
- [ ] **DROP**: N/A.
- [ ] **LOCK_ORDER**: N/A.
- [ ] **FFI**: N/A.
- [ ] **TESTS**: Unit test that a NIF carrying BSXFlags(0x20|0x01) (editor + havok-managed) spawns with `BSXFlags(0x21)` on the placement root.
