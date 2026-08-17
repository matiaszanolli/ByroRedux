# TD2-2026-08-16-01: the raw-debug-output predicate is hand-written in Rust and twice in GLSL, and its guard is a four-literal subset check

**Issue**: #2978
**Severity**: LOW
**Dimension**: 2 — Logic Duplication
**Labels**: `low,renderer,tech-debt,bug`
**Source report**: `docs/audits/AUDIT_TECH_DEBT_2026-08-16.md`
**Filed**: 2026-08-17 via `/audit-publish`

---

Filed from `docs/audits/AUDIT_TECH_DEBT_2026-08-16.md` (Dimension 2 — Logic Duplication). Effort: small.

**Location**: `crates/renderer/src/shader_constants.rs`:33-41 · `crates/renderer/shaders/presentation.frag`:136-139 · `crates/renderer/shaders/composite.frag`:405-408 · guard at `crates/renderer/src/vulkan/scene_buffer/gpu_instance_layout_tests.rs`:695-710

## Description

"Which debug visualizations are correctness oracles that must bypass the post-transport frame graph" is **one policy expressed at three sites in two languages**.

Rust:
```rust
pub const fn debug_viz_requires_raw_output(flags: u32) -> bool {
    flags & (DBG_VIZ_SELECTED_LIGHT | DBG_VIZ_DIRECT | DBG_VIZ_RAW_INDIRECT) != 0
        || (flags & DBG_VIZ_RT_LOD) == DBG_VIZ_RT_LOD
}
```

GLSL (`presentation.frag`:136-139, and the same four clauses in `composite.frag`:405-408):
```glsl
bool rawDebug = (dbgFlags & DBG_VIZ_SELECTED_LIGHT) != 0u
    || (dbgFlags & DBG_VIZ_DIRECT) != 0u
    || (dbgFlags & DBG_VIZ_RAW_INDIRECT) != 0u
    || (dbgFlags & DBG_VIZ_RT_LOD) == DBG_VIZ_RT_LOD;
```

The guard that is supposed to hold them in lockstep asserts only that each of **four hardcoded strings is present** in each shader. It is a subset check against an expected set derived from nothing — so adding a fifth raw-output view to the Rust function (the natural next edit, since the sibling test `correctness_debug_views_require_raw_frame_graph_output` already lists **six** view constants) leaves both shaders silently tone-mapping a correctness oracle while the whole suite stays green.

## Evidence

- `shader_constants.rs`:93-100 asserts the Rust predicate over six inputs (`SELECTED_LIGHT`, `SHADOW_VISIBILITY`, `MATERIAL_LOBES`, `RT_LOD`, `DIRECT`, `RAW_INDIRECT`)
- `gpu_instance_layout_tests.rs`:698-706 pins the GLSL side with four `source.contains(…)` literals
- **Nothing relates the two sets**

The house pattern for exactly this problem exists two files away — `generated_header_contains_all_defines` iterates the shared `DBG_BITS` catalog precisely *"so this value-pin can never again cover a subset (#1482 / #1860)"*.

## Impact

A debug view whose entire purpose is to be an unmodified oracle gets ACES tone-mapping, exposure and grading applied on the presentation pass, making black-vs-dim and isolated-energy readings **meaningless — with no test failure**.

Blast radius is developer tooling only, not shipped rendering.

## Suggested Fix

Emit a `DBG_VIZ_RAW_OUTPUT_MASK` (and the `RT_LOD` compound test) from `shader_constants_data.rs` and have both shaders consume it, so the policy lives in one place and `generated_header_contains_all_defines` covers it for free.

## Related

- #2800, #2799, #2798 (the same "shader doc/guard describes a different thing than the code" family in the renderer)
- #1482, #1860 (the two prior rounds of subset-pin defect on the `DBG_*` catalog)
- TD9-2026-08-16-02 (the sibling allow-list gap on the same shader)

## Completeness Checks
- [ ] **SIBLING**: Both shaders converted, not just `presentation.frag`
- [ ] **SHADER-SYNC**: The generated `#define` lands in `shader_constants.glsl` via `build.rs`, never hand-edited
- [ ] **SPV-RECOMPILE**: Affected `.spv` recompiled with plain `-V` (not `-g0` — the reflection test needs OpName)
- [ ] **NO-SUBSET-PIN**: The replacement guard iterates the catalog rather than listing literals
- [ ] **TESTS**: Adding a fifth raw-output view fails the guard if a shader is not updated

---

*Immutable snapshot of the issue as filed. GitHub is authoritative for current state —
query `gh issue view 2978 --json state` when live state is needed.*
