# AUDIT — Renderer Dimension 11: TAA (Temporal Antialiasing, M37.5)

**Date**: 2026-05-12
**Branch**: `main`
**Focus**: Dim 11 only (TAA pipeline + jitter + composite/resize integration).
**Methodology**: Inline audit (the orchestrator's specialist agents were terminating early this session; this dimension is well-scoped enough to handle in the main agent). Verified all 12 checklist items against current code; deduplicated against `/tmp/audit/renderer/issues.json` (0 open TAA issues) and prior `docs/audits/AUDIT_RENDERER_2026-05-08_DIM11.md`.

---

## Executive Summary

**0 findings**. TAA pipeline on current `main` is production-ready and has been hardened through five rounds of prior audits (REN-D11-NEW-01 through REN-D11-NEW-05), all closed and protected by in-source comment anchors plus `validate_set_layout` SPIR-V reflection.

| Severity | Count |
|----------|-------|
| CRITICAL | 0 |
| HIGH     | 0 |
| MEDIUM   | 0 |
| LOW      | 0 |
| **Total** | **0** |

The audit prompt's 12 checklist items all PASS. No new correctness, safety, or
hygiene work is required on the TAA surface.

---

## RT / Rasterization assessment

Dim 11 sits at the **post-render-pass / pre-composite** seam — between SVGF and
composite tone-mapping. Its correctness is gated by:

- **Per-frame fence serialisation** (`draw.rs:170-181`, both-slots wait) — covers prior-frame fragment consumption of the same history slot, allowing the pre-dispatch barrier to use `COMPUTE → COMPUTE` only (REN-D11-NEW-05, 2026-05-09).
- **`validate_set_layout` reflection** at `taa.rs:271-281` — layout drift between Rust `vk::DescriptorSetLayoutBinding` and SPIR-V `taa.comp` is a startup panic (#427).
- **Resize hygiene** at `resize.rs:560-592` — frame counter zeroed, Halton index reset, 8-frame temporal-discontinuity gate forces clean accumulations after swapchain recreate (#913).
- **Failure latch** at `context/mod.rs:822` — TAA dispatch errors trip `taa_failed`, composite rebinds to raw HDR (`composite.rs:1162` / #479), no silent stale-frame freeze.

All four are in force and tested.

---

## Checklist Status

| # | Item | Status |
|---|------|--------|
| 1 | Halton (2,3) jitter, NDC pixel units | PASS |
| 2 | Un-jittered projection alongside jittered | PASS (different design — superior; jitter applied to `gl_Position` only, un-jittered varyings carry motion-vector source) |
| 3 | Per-frame-in-flight history slot, no aliasing | PASS |
| 4 | Reprojection: linear filter or 5-tap dilation (not point) | PASS — 5-tap MV-max (REN-D11-NEW-03 / #915) |
| 5 | YCoCg neighborhood clamp before blend | PASS (gamma = 1.25) |
| 6 | Mesh-ID disocclusion via prev-frame sample | PASS — bit-15 masked (REN-D11-NEW-02 / #904) |
| 7 | First-frame path: no NaN, α forced to 1 | PASS — NaN guard at `taa.comp:192-194` (#903) |
| 8 | History in `GENERAL`, no UNDEFINED per-frame | PASS |
| 9 | 7-binding descriptor set matches docstring | PASS |
| 10 | `validate_set_layout` fires, matches bindings | PASS (#427) |
| 11 | Composite samples TAA output when TAA on | PASS |
| 12 | Disable path skips dispatch entirely | PASS |

---

## Findings (none)

No new findings. Every checklist item verified PASS against current code.

---

## Verified-still-resolved (prior audit re-verification)

| Anchor | Finding | Closure | Status |
|--------|---------|---------|--------|
| **REN-D11-NEW-01** | NaN propagation through history relied on undefined `min`/`max` semantics | `taa.comp:192-194` NaN/Inf guard before YCoCg clamp | Closed via #903 |
| **REN-D11-NEW-02** | Disocclusion used full u16 mesh_id — bit-15 alpha-blend toggle force-reset on opacity transitions | `taa.comp:143` `& 0x7FFF` mask | Closed via #904 |
| **REN-D11-NEW-03** | Motion-vector point sample produced ghosting on silhouette edges | `taa.comp:112-126` 5-tap MV-max dilation | Closed via #915 |
| **REN-D7-NEW-07** | Halton index not reset across swapchain resize → one-frame ghost | `resize.rs:580-592` `frame_counter = 0` + `signal_temporal_discontinuity(8)` | Closed via #913 |
| **REN-D11-NEW-05** (audit 2026-05-09) | Pre-dispatch src_stage_mask over-spec (FRAGMENT redundant under both-slots fence) | `taa.rs:733-742` — narrowed to `COMPUTE → COMPUTE` only | Closed in #915 cycle |

---

## Design notes (informational, not findings)

- **No separate un-jittered projection in the UBO**: the design jitters
  `gl_Position` post-shader-output and passes un-jittered clip positions via
  `fragCurrClipPos` / `fragPrevClipPos` varyings to the fragment shader. This
  is strictly superior to storing two matrices — one less 64-byte upload per
  frame, motion-vector reconstruction is per-fragment from interpolated
  un-jittered clip coords. Auditors comparing against textbook designs should
  note this is an intentional simplification.

- **M-LIGHT v1 (Dim 20) interaction**: TAA gamma = 1.25 is in-band for typical
  variance-clamp tolerances. The single-tap stochastic sun-shadow sample
  introduced by Dim 20 converges through the temporal window without persistent
  noise; YCoCg clamp doesn't over-blur per-frame noise.

---

## Prioritized fix order

**No fixes required.** TAA pipeline is steady-state production-ready.

If a future audit observes regressions, the highest-risk surfaces to re-check are:

1. **Layout transitions** on history images (UNDEFINED → GENERAL once via `initialize_layouts`, then GENERAL ↔ GENERAL per frame). A new code path that re-allocates history without calling `initialize_layouts` would skip the UNDEFINED → GENERAL transition.
2. **`validate_set_layout` panic** at startup — any change to `taa.comp` bindings without updating `taa.rs:235-269` is a panic, not silent corruption. Good failure mode.
3. **`taa_failed` latch** — once set, composite stays on raw HDR for the session. Recoverable per-frame retry was deliberately avoided to keep the failure path simple.
4. **Halton index alignment to TAA history**: `resize.rs:580` zeroes `frame_counter`; any other path that re-creates the TAA history (e.g. a hypothetical "switch quality preset" command) must also reset the Halton index.

---

Suggest: `/audit-publish docs/audits/AUDIT_RENDERER_2026-05-12_DIM11.md` (no-op — zero findings)
