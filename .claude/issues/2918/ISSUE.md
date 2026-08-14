# REN-D2-03: shader-pipeline.md Set-1 table omits binding 18 and material_kind 103

- **Issue**: [#2918](https://github.com/matiaszanolli/ByroRedux/issues/2918)
- **Finding ID**: `REN-D2-03`
- **Labels**: `low,renderer,documentation`
- **Source report**: [`docs/audits/AUDIT_RENDERER_2026-08-14.md`](../../../docs/audits/AUDIT_RENDERER_2026-08-14.md)
- **Run**: `/audit-suite rt-deep`, 2026-08-14, HEAD `205744ae`

> Immutable snapshot of the issue *as filed* (TD10-001 / #1156). GitHub is
> authoritative for current state — query `gh issue view 2918 --json state`.

---

- **Severity**: LOW
- **Dimension**: SSBO/Indexing
- **Location**: `docs/engine/shader-pipeline.md` (`## Descriptor Sets` table; `**material_kind**
  (offset 88)` table); live contract: `build_scene_descriptor_bindings` in
  `crates/renderer/src/vulkan/scene_buffer/buffers.rs`, `MATERIAL_KIND_FIRE_REFRACTION` in
  `crates/renderer/shaders/include/shader_constants.glsl`
- **Status**: NEW (sibling of OPEN #2781, which covers a different row of the same table)
- **Description**: Two omissions in the tables this dimension audits against.
  (a) `build_scene_descriptor_bindings` declares **binding 18** — the previous-frame rigid
  instance model matrices, `STORAGE_BUFFER`, vertex stage, whose entries deliberately align
  index-for-index with binding 4 so `gl_InstanceIndex` addresses both. It landed in `33d9a468`
  (2026-07-22) and has never appeared in the doc's Set-1 table, which ends at 17. That is
  precisely the "does the shader read the offsets the Rust upload writes" question this
  dimension exists to answer, and the doc silently claims the answer for a binding it doesn't
  list.
  (b) The `material_kind` table lists 0–19, 100, 101, 102 but not `103`
  (`MATERIAL_KIND_FIRE_REFRACTION`), even though it is a live generated constant that
  `shadow_transport.glsl` branches on (`effectCard`, the skip that keeps fire proxies from
  casting shadows, #2224) and that `triangle.frag` uses to reinterpret `mat.ior` as a 0–1
  distortion scalar (#2232).
- **Evidence**:
  - `buffers.rs`: "// Binding 18: previous rigid-instance model matrices (vertex shader).
    Entries align with binding 4's current-frame instance array after sorting/batching, so
    `gl_InstanceIndex` addresses both…" followed by the `.binding(18)` push. The doc's table's
    last row is `| 1 | 17 | STORAGE_BUFFER | ReSTIR reservoir buffer (previous frame) |`.
  - `shader_constants.glsl`: `#define MATERIAL_KIND_FIRE_REFRACTION 103u`;
    `shadow_transport.glsl`: `bool effectCard = hitMat.materialKind ==
    MATERIAL_KIND_EFFECT_SHADER || hitMat.materialKind == MATERIAL_KIND_FIRE_REFRACTION;`.
    `grep -n "FIRE_REFRACTION\|103" docs/engine/shader-pipeline.md` returns nothing.
- **Impact**: Documentation only. Cost is paid by future readers and by audits that use the
  table as the completeness reference for Set 1 — an undocumented binding is one nothing
  checks for lockstep, and #2748 already shows this family of guard being presence-only.
- **Related**: #2781 (OPEN, binding-11 row of the same table), #1948 / #1915 (CLOSED, the
  previous round of Set-1 table catch-up for bindings 15/16/17), #2224, #2232.
- **Suggested Fix**: Add the binding-18 row (noting the index-alignment contract with binding
  4) and the `103 | MATERIAL_KIND_FIRE_REFRACTION` row. Worth folding into #2781's fix so the
  table is corrected in one pass.

---

## Completeness Checks
- [ ] **SIBLING**: The same doc table / anchor class is swept, not just the one row cited
- [ ] **TESTS**: A regression test pins this specific fix

---

*Filed by `/audit-publish` from [`docs/audits/AUDIT_RENDERER_2026-08-14.md`](docs/audits/AUDIT_RENDERER_2026-08-14.md) — `/audit-suite rt-deep`, 2026-08-14, HEAD `205744ae`. Verified CONFIRMED against current code at publish time.*
