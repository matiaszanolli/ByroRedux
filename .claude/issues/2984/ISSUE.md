# TD9-2026-08-16-02: the shader-include allow-list covers 16 of the 17 live header consumers — presentation.frag is missing

**Issue**: #2984
**Severity**: LOW
**Dimension**: 9 — Test Hygiene (green-by-construction)
**Labels**: `low,renderer,tech-debt,bug`
**Source report**: `docs/audits/AUDIT_TECH_DEBT_2026-08-16.md`
**Filed**: 2026-08-17 via `/audit-publish`

---

Filed from `docs/audits/AUDIT_TECH_DEBT_2026-08-16.md` (Dimension 9 — Test Hygiene, green-by-construction). Effort: trivial.

**Location**: `crates/renderer/src/shader_constants.rs`:324-394
**Age**: `presentation.frag` gained the include in `5f970bae`, 2026-08-15

## Description

`affected_shaders_include_constants_header` exists because *"a shader that drops the `#include` would otherwise compile against undefined identifiers … and no `cargo test` would catch it (the SPIR-V is pre-compiled)"*.

Its own doc comment states the invariant it must satisfy: *"this allow-list MUST cover every shader that consumes a generated macro from `shader_constants.glsl`"*, with an explicit maintenance instruction — *"Cross-check when adding a shader: `grep -L` the include across `shaders/*.{comp,frag,vert}` and reconcile against this list."*

**That reconciliation has not happened.** The list holds 16 entries; `crates/renderer/shaders/presentation.frag` includes the header at line 4 and consumes four generated macros (`DBG_VIZ_SELECTED_LIGHT`, `DBG_VIZ_DIRECT`, `DBG_VIZ_RAW_INDIRECT`, `DBG_VIZ_RT_LOD`) at :136-139.

It is the **only** omission — the list is otherwise exactly right.

## Evidence

```
$ cd crates/renderer/shaders && grep -l 'include/shader_constants.glsl' *.vert *.frag *.comp | wc -l
17
```

Re-verified 2026-08-16: 17 live consumers, 16 allow-list entries, `presentation.frag` absent (`grep -c presentation` over :324-394 → 0).

Live consumers: `bloom_downsample.comp`, `bloom_upsample.comp`, `caustic_splat.comp`, `cluster_cull.comp`, `composite.frag`, **`presentation.frag`**, `skin_palette.comp`, `skin_vertices.comp`, `ssao.comp`, `svgf_atrous.comp`, `svgf_temporal.comp`, `taa.comp`, `triangle.frag`, `triangle.vert`, `volumetrics_inject.comp`, `volumetrics_integrate.comp`, `water.frag`. The test's array (`shader_constants.rs`:340-388) lists all but the bolded one.

This is the same defect the list was *last* expanded for: #1780 added six previously-unlisted header consumers.

## Impact

Removing `presentation.frag`'s `#include` — plausible during a post-pass refactor, since the shader's only generated-macro use is one debug branch — would leave `DBG_VIZ_*` undefined at recompile time **with no test failure**.

The presentation pass is the engine-default output stage since FSR phase 7, so the recompile break lands on the final swapchain write.

## Suggested Fix

Add the entry (one line — this is the quick win).

Better: replace the hand-maintained array with a compile-time enumeration in `build.rs` (which already walks `shaders/`), so the list cannot lag the directory a third time.

## Related

- #1780 (the previous round of the same omission)
- #2978 (TD2-2026-08-16-01 — the same four `DBG_VIZ_*` macros, duplicated policy in the same shader)

## Completeness Checks
- [ ] **SIBLING**: The `grep -l` reconciliation re-run across all of `shaders/`, not just `presentation.frag`
- [ ] **NO-THIRD-TIME**: Preferably driven from `build.rs`'s directory walk rather than a hand-maintained array
- [ ] **CO-RESOLVE**: Checked against #2978 — both concern the same shader and the same macros
- [ ] **TESTS**: Deleting an `#include` from any live consumer fails the suite

---

*Immutable snapshot of the issue as filed. GitHub is authoritative for current state —
query `gh issue view 2984 --json state` when live state is needed.*
