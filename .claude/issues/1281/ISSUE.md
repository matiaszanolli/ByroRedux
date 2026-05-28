# Issue #1281: Geometry/transform re-scoping post-falsification (#1277 Workstream B)

**State**: OPEN
**Labels**: enhancement, import-pipeline, medium

## Body

**Child of #1277 — Workstream B (geometry/transform).**

Re-scoping after the initial hypothesis was falsified this session.

## What was falsified (2026-05-27)

**Original hypothesis** (epic doc §3 Axis 2 first draft): `import/coord.rs::zup_matrix_to_yup_quat` → `svd_repair_to_quat` discards non-uniform scale/shear baked into `NiTriShape` 3×3 matrices, ballooning Fallout modular architecture.

**Measurement** (`crates/nif/examples/dump_transforms.rs`):

| Corpus | NIFs | AV blocks | non-identity rot | non-uniform scale | shear |
|---|---:|---:|---:|---:|---:|
| FNV `architecture/` (all) | 2034 | 10 837 | — | **0** | **0** |
| FNV `architecture/strip/` casinos | 263 | 2 089 | 427 (20%) | **0** | **0** |

Tool proven live (20% of casino matrices are genuine non-identity rotations), max column-norm spread = `0.00000`. **FNV matrices are perfectly orthonormal; the transform/coord translation loses no geometric information.**

## What's actually left

If users still report "walls 90° off or out of place" in FNV interiors on current `main`, the remaining causes are:

1. **Wrong base mesh placed at right location** — FormID remap collision, missing master, plugin load-order resolving the REFR's `name` to a different STAT than the original cell author intended. Looks geometric, isn't.
2. **Per-REFR placement defect** (position/rotation, NOT scale) — cannot be ruled out without identifying the specific REFR via interactive `pick` / `mesh.info`. If a true-geometry defect survives Workstream A (material) and Workstream C (lighting) fixes, this is where to look.

## Deliverables

- [ ] **Interactive mesh-ID diagnostic workflow**. Doc the `pick` / `mesh.info` / `cell.refrs` console-command sequence for "I'm staring at a mesh I think is misplaced — what does its REFR look like?" Today this is improvised per-debugging-session.
- [ ] **REFR-FormID-collision probe**. Extend `crates/plugin/examples/probe_form.rs` (which already walks indexed record categories) to flag form ids that resolve in multiple plugin sources at different positions/rotations — surfaces the "wrong base mesh, right location" class.
- [ ] **Static analyzer for unusual REFR data**. A `cell_refr_outliers <ESM> <CELL_EDID>` example that flags REFRs whose `(scale, rotation)` pair is in the bottom 1% of the cell's distribution — these are the most likely candidates if a true geometric defect exists.
- [ ] **Re-test after Workstream A + C land**. If the casino-interior screenshot users keep posting still has misplaced geometry after the material collapse + interior sun leak are fixed, the residual is per-REFR placement and gets a follow-up issue with a specific REFR identified.

## Note on the deferral

This is intentionally lower-priority than A / C / Task 2. The geometric brokenness perception in the screenshots that drove the original epic is mostly material (Workstream A) + lighting (Workstream C). The transform-fidelity audit ruled out a systemic translation gap. Future work here is per-REFR detective work, not a broad refactor.

## References

- Parent epic: #1277
- Falsification record: [docs/engine/nif-engine-translation-layer.md §3 Axis 2](../blob/main/docs/engine/nif-engine-translation-layer.md)
- Inspector tool: `crates/nif/examples/dump_transforms.rs` (commit `90fe5895`)
- Per-REFR Euler A/B inspector: `crates/plugin/examples/cell_rot_sweep.rs` (commit `4165f9f1`)
