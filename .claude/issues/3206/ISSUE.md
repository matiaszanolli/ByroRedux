# TD3-2026-08-20-03: shader-pipeline.md descriptor table stops at Set 2 Binding 0 - GpuWaterParams is in no document

**Issue**: #3206 — https://github.com/matiaszanolli/ByroRedux/issues/3206
**Severity**: LOW
**Labels**: `low,renderer,tech-debt,documentation`
**Source report**: `docs/audits/AUDIT_TECH_DEBT_2026-08-20.md`
**Filed**: 2026-08-20 · `/audit-publish` · verified against HEAD `bb0b92f2`

---

Filed from `docs/audits/AUDIT_TECH_DEBT_2026-08-20.md` § TD3-2026-08-20-03 (Dimension 3 — Stale Documentation & Comments).

**Severity**: LOW · **Effort**: trivial
**Location**: `docs/engine/shader-pipeline.md:375-398` (the Set 0–2 descriptor table). The undocumented resource is `crates/renderer/src/vulkan/water.rs:316` (`.binding(1)`) / `:65-140` (`GpuWaterParams`).

## Description

`_audit-common.md:121` designates `shader-pipeline.md` as the authority for *"descriptor set bindings (Set 0–2)."* Its table's last row is:

```
| 2 | 0 | `STORAGE_IMAGE` (`R32_UINT`) | Water caustic accumulator | water.frag (atomic add) |
```

**Set 2 Binding 1 has no row.** That is the `WaterParamsBlock` UBO — `GpuWaterParams[186]`, 352 B per record, ~65.5 KB, bound by **both** `water.vert` and `water.frag`. It is the largest per-draw GPU contract added in this delta.

`GpuWaterParams` appears in **no file under `docs/` at all**, while its four siblings (`GpuCamera`, `GpuInstance`, `GpuMaterial`, `GpuLight`) each get a full offset/size/field table in the same document's "GPU Data Types" section.

## Evidence (verified at HEAD `bb0b92f2`)

```
$ grep -rn "GpuWaterParams" docs/ | grep -v docs/audits
(no output)

$ sed -n '396,398p' docs/engine/shader-pipeline.md
| 1 | 18 | STORAGE_BUFFER | Previous-frame rigid instance model matrices …
| 2 | 0  | STORAGE_IMAGE (R32_UINT) | Water caustic accumulator | water.frag |
                                     ← table ends; no `| 2 | 1 |` row

$ grep -n "binding(1)" crates/renderer/src/vulkan/water.rs
316:            .binding(1)
```

## Impact

Documentation *gap* rather than rot, but it compounds two live findings:

- **#3124** (REN-D15-01) — the struct has three hand-mirrored declarations and no lockstep guard
- The `uv_offset`/`absorption` lane-semantics drift filed alongside — those three declarations document their lanes inconsistently

A `GpuWaterParams` offset table in the authoritative doc would give all three a **single reference to diff against** — and would make the **64-byte UBO headroom** (per `AUDIT_SAFETY_2026-08-20`'s `MAX_WATER_DRAWS` finding) visible where someone will actually see it *before* adding a field.

## Suggested Fix

1. Add the descriptor row:
   `| 2 | 1 | UNIFORM_BUFFER | GpuWaterParams[186] (352 B each, ~65.5 KB) | water.vert, water.frag |`
2. Add a `### GpuWaterParams — 352 bytes` subsection to "GPU Data Types", mirroring the `GpuCamera` table's offset/size/field format, and note the remaining headroom.

## Related

- **#3124** (REN-D15-01) — three declaration sites, no lockstep guard
- The `uv_offset` / `absorption` lane-semantics finding filed from this same report
- `AUDIT_SAFETY_2026-08-20` — the `MAX_WATER_DRAWS` vs `maxUniformBufferRange` headroom finding

## Completeness Checks
- [ ] **SIBLING**: Both the descriptor row *and* the GPU Data Types subsection added — the row alone leaves the field layout undocumented
- [ ] **HEADROOM**: The remaining UBO headroom is stated where a field-adder will read it
- [ ] **CONSISTENT**: The new offset table matches all three live declarations (Rust, `water.vert`, `water.frag`)
