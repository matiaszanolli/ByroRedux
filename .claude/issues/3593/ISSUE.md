# #3593 — REN-2026-08-30-D7-03: the dedup hash's own doc understates its field count by 16, and the intern call site quotes a superseded `GpuMaterial` size

**Labels**: `low,renderer,doc-rot,documentation`
**Filed**: 2026-08-30 via `/audit-publish`
**Report**: `docs/audits/AUDIT_RENDERER_2026-08-30.md`

> Immutable snapshot of the issue as filed (TD10-001 / #1156). GitHub is
> authoritative for current state — `gh issue view 3593 --json state`.

---

- **Severity**: LOW
- **Dimension**: Material Table
- **Location**: `crates/renderer/src/vulkan/material.rs:998` (`hash_gpu_material_fields` doc), `byroredux/src/render/static_meshes.rs:886` (`intern_by_hash` call site)
- **Status**: OPEN
- **Description**: Two stale numerics on the R1 dedup hot path:
  - `material.rs:998` — "Canonical material hash — FxHash (#1368) over the **92 live scalar fields** of `GpuMaterial` in declaration order." The struct declares 108 scalar fields and the function hashes all 108.
  - `static_meshes.rs:886` — "`intern_by_hash` skips the `to_gpu_material()` **364-byte** construction on the dedup-hit path". `GpuMaterial` has been 432 B since the 2026-08-25 BGEM-glass-optics + Bethesda-lighting growth; 364 B was the `#2221` intermediate, and the size history on `material.rs:40` records the two later steps (396 B, 432 B).
- **Evidence**: `awk` over the `pub struct GpuMaterial` body yields 108 `pub <ident>:` fields; 108 × 4 B = 432 B, matching `gpu_material_size_is_432_bytes` (`material.rs:1494`, passing). The same extraction over the `hash_gpu_material_fields` body yields 108 distinct `mat.<field>` identifiers. `grep -rn "364-byte\|92 live scalar" crates/renderer/src/vulkan/material.rs byroredux/src/render/static_meshes.rs` returns exactly these two sites.
- **Impact**: The "92 fields" line is the doc a reader consults before extending the walk — the point at which under-counting is most likely to become the D7-01 bug. `#1368`/`#2273` already established the convention of pointing at `gpu_material_size_is_432_bytes` instead of restating a drifting field count (see `intern_by_hash`'s collision-policy paragraph, `material.rs:1332`); this doc predates that convention and never got converted.
- **Suggested Fix**: Replace "the 92 live scalar fields" with a reference to `gpu_material_size_is_432_bytes` (or to the field-coverage guard proposed in D7-01, once it exists) rather than a fresh literal; update `static_meshes.rs:886` to 432 B, or drop the byte figure and say "the full `GpuMaterial` construction".

**Source**: `docs/audits/AUDIT_RENDERER_2026-08-30.md` — REN-2026-08-30-D7-03

## Completeness Checks
- [ ] **SIBLING**: Same stale claim checked in related files (other docs, other in-code comments, audit SKILL files)
- [ ] **TESTS**: Where the codebase already pins a doc/code agreement with an `include_str!` scan, extend that pin rather than relying on review
