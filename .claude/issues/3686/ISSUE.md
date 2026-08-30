# #3686 — PERF-D5-2026-08-30-06: the `svgf` GPU timer bracket is named for one dispatch but encloses four screen-sized ones

- **Source**: `docs/audits/AUDIT_PERFORMANCE_2026-08-30.md`
- **Finding ID**: `PERF-D5-2026-08-30-06`
- **Filed**: 2026-08-30 (HEAD `64f64480`)
- **Labels**: low,performance,renderer,doc-rot,documentation
- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/3686

> Immutable snapshot of the issue as filed (TD10-001 / #1156). GitHub is authoritative for current state.

---

- **Severity**: LOW
- **Dimension**: GPU Pipeline
- **Location**: `crates/renderer/src/vulkan/gpu_timers.rs:21-22` (slot table) and
  `:149-151` (`svgf_ms` doc comment); actual bracket contents
  `crates/renderer/src/vulkan/svgf.rs:1290-1385` inside one `dispatch` call,
  bracketed at `crates/renderer/src/vulkan/context/post_passes.rs:330-334`
- **Status**: NEW
- **Description**: The query-slot table calls slots 12/13 "SVGF temporal dispatch",
  and `svgf_ms`'s doc says "SVGF temporal accumulation compute dispatch —
  motion-vector reprojection of last frame's denoised indirect." The bracket
  actually wraps `SvgfPipeline::dispatch` in full: one temporal dispatch **plus**
  `ATROUS_ITERATIONS` (= 3, `crates/renderer/src/vulkan/svgf.rs:98`) à-trous
  dispatches, each `width.div_ceil(8) × height.div_ceil(8)`, each followed by a
  COMPUTE→COMPUTE barrier. Three of the four full-screen dispatches under the
  number are not what the number is named after.
- **Evidence**: `post_passes.rs:330-334` places `cmd_svgf_start`/`cmd_svgf_end`
  around the single `svgf.dispatch(...)` call; `svgf.rs:1339-1385` runs the
  `for k in 0..ATROUS_ITERATIONS` loop inside that same call.
  `docs/engine/shader-pipeline.md:104-108` documents the pipeline correctly
  ("`svgf_atrous.comp` ×3"), so only the instrument's own labels are wrong.
- **Impact**: Directly degrades the instrument this audit dimension is required to
  cite. An operator reading a high `svgf_ms` will chase temporal reprojection when
  75% of the bracketed dispatches are the spatial filter — and the à-trous loop is
  precisely where a past audit found redundant per-iteration variance work
  (`AUDIT_PERFORMANCE_2026-07-02.md:351`, then at `ATROUS_ITERATIONS = 5`). It also
  blocks the obvious next question — "temporal or spatial?" — which two brackets
  would answer for free.
- **Related**: `#2278`/`PERF-D9-01` (the `_active` flags) is the prior work on this
  instrument's honesty.
- **Suggested Fix**: Minimum: rename the slot-table rows and the `svgf_ms` doc to
  "SVGF temporal + à-trous (`ATROUS_ITERATIONS`) dispatches". Better: split into
  *svgf_temporal_ms* / *svgf_atrous_ms* (query pool 28→30) so the two costs are
  separable — the à-trous loop is the tunable one.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/mod.rs` (`extract_emitter_params` / `extract_emitter_rate`), per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix

---
*Filed from `docs/audits/AUDIT_PERFORMANCE_2026-08-30.md` (HEAD `64f64480`). Report status: NEW; re-verified CONFIRMED against HEAD at publish time.*
