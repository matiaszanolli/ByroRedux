# REN-D8-03: Rotted file:NN anchors across the denoiser/composite sources

- **Issue**: [#2922](https://github.com/matiaszanolli/ByroRedux/issues/2922)
- **Finding ID**: `REN-D8-03`
- **Labels**: `low,renderer,tech-debt,documentation`
- **Source report**: [`docs/audits/AUDIT_RENDERER_2026-08-14.md`](../../../docs/audits/AUDIT_RENDERER_2026-08-14.md)
- **Run**: `/audit-suite rt-deep`, 2026-08-14, HEAD `205744ae`

> Immutable snapshot of the issue *as filed* (TD10-001 / #1156). GitHub is
> authoritative for current state — query `gh issue view 2922 --json state`.

---

- **Severity**: LOW
- **Dimension**: Denoiser/Composite
- **Location**: `crates/renderer/shaders/svgf_temporal.comp`,
  `crates/renderer/shaders/svgf_atrous.comp`,
  `crates/renderer/shaders/composite.frag`,
  `crates/renderer/src/vulkan/svgf.rs`,
  `crates/renderer/src/vulkan/context/post_passes.rs`
- **Status**: NEW (same shape as #2773, #2757, #2510, #2755 — none of which covers
  these sites)
- **Description**: The Dim-8 sources navigate almost entirely by bare line
  numbers, and every anchor I checked now points somewhere unrelated. Several
  point at code of the *opposite* kind, which is worse than a dangling pointer:
  a reader following `triangle.frag:267` for the octahedral-encode contract lands
  in the decal-index array and can reasonably conclude the encodings differ.
- **Evidence** (each verified against the live tree today):

  | Comment site | Cited anchor | What actually lives there |
  |---|---|---|
  | `svgf_temporal.comp` header (bindings 9/10) | `triangle.frag:644` — "reads the RG16_SNORM `outNormal` G-buffer attachment" | the `DBG_VIZ_FSR_TEMPORAL` jitter-visualisation block |
  | `svgf_temporal.comp::octDecode` | `triangle.frag:267` / `caustic_splat.comp:91` — "matches" | the `materialDecals[4]` array / a deferred-work note above the `GpuInstance` mirror |
  | `svgf_atrous.comp::octDecode` | `triangle.frag:267` (same) | as above |
  | `svgf_temporal.comp` #675 / #904 comments | "the early-out at line 93", "a plain weighted blend at line 152-153", "the `histAge` weighted-average at line 156", "the early-out near line ~97" | the early-out is the `currID == 0u \|\| (currID & 0x80000000u)` test; the blend and the `histAge +=` accumulation are ~30 lines below the cited numbers |
  | `svgf.rs` (`should_force_history_reset` doc) | `svgf_temporal.comp:81` — "`1.0 = reset history`" | `float currLum2 = currLum * currLum;`. The reset read is `params.z < 0.5` inside `reprojectOk` |
  | `svgf.rs::dispatch` | `draw.rs:170-181` — the both-slots `wait_for_fences` (#282) | a `skinnedVertexAddress` doc block. **The fence wait itself is intact**, ~1300 lines later |
  | `svgf.rs::dispatch` | `taa.rs:789`, `caustic.rs:816`, `volumetrics.rs:846` — "sibling barriers" | a `recreate_history` doc comment, a `dispatch` signature, a descriptor-binding literal — none is a barrier |
  | `composite.frag` binding 8 | `composite.rs:360` — "the existing integer-format-sampling rule" | HDR image sub-allocation. The rule is the `nearest_sampler` field doc / its `create_sampler` call |
  | `composite.frag::compute_sky` | "the sky-lower mix at L107", "the disc faded correctly (line 222)" | both are inside comment prose; the real `mix(horizon, params.sky_lower.xyz, below)` and the `sky += disc_color * …` sites are far below |
  | `post_passes.rs::record_bloom_pass` | `context/mod.rs:1715-1717` — the `rebind_hdr_views` call | a `GBufferFormats` struct literal. The rebind is ~1080 lines later |
  | `post_passes.rs::record_bloom_pass` | `context/mod.rs:1958-1967` — the bloom hard-fail | a neutral-texture upload. The hard-fail (`anyhow::anyhow!("Bloom pipeline failed to initialize — composite requires the bloom output view for binding 7 (M58)…")`) is real but ~720 lines later |
  | `post_passes.rs::record_volumetrics_pass` | `caustic.rs:627` / `draw.rs:1648` — the TLAS gate it mirrors | a view-creation error path / an index-buffer upload |
  | `post_passes.rs::record_volumetrics_pass` | `draw_frame, ~line 2960` — cluster_cull's trailing barrier | `upload_previous_models` |

- **Impact**: Documentation only, but concentrated on the load-bearing
  cross-pass contracts of this dimension: the mesh-ID/normal encoding shared by
  three shaders, the fence that makes the shared depth view and prev-slot
  G-buffer reads legal, and the bloom/TAA descriptor rewiring. Anyone auditing or
  refactoring here is routed to the wrong code by roughly a dozen pointers, and
  two of them (`triangle.frag:267`, `draw.rs:170-181`) invite the false conclusion
  that a real invariant is absent. The repo already ruled on this class in
  **#1040** ("Audit-skill anchor rot — switch bare line numbers to symbol-based
  anchors", CLOSED) and the audit protocol mandates symbols over line numbers;
  the renderer shaders never got the sweep.
- **Related**: #1040, #2773, #2757, #2510, #2755, `_audit-common.md`
  "Path-Reference Convention" and `audit-renderer/SKILL.md` "Symbols, not line
  numbers".
- **Suggested Fix**: Replace each `file:NN` with the symbol it means
  (`triangle.frag`'s `octEncode`/`outNormal` write, `svgf_temporal.comp`'s
  `octDecode`, `draw.rs`'s `wait_for_fences` on `in_flight[frame]`/`in_flight[prev]`,
  `composite.rs`'s `nearest_sampler`, `context/mod.rs`'s `rebind_hdr_views` and the
  `bloom_views` match, `caustic.rs`'s `tlas_handle` gate). Extending
  `.claude/commands/_audit-validate.sh`'s advisory pass to flag `\w+\.(rs|comp|frag|vert):\d+`
  inside `crates/renderer/` would keep it from re-accumulating.

---

## Completeness Checks
- [ ] **SIBLING**: The same doc table / anchor class is swept, not just the one row cited
- [ ] **TESTS**: A regression test pins this specific fix

---

*Filed by `/audit-publish` from [`docs/audits/AUDIT_RENDERER_2026-08-14.md`](docs/audits/AUDIT_RENDERER_2026-08-14.md) — `/audit-suite rt-deep`, 2026-08-14, HEAD `205744ae`. Verified CONFIRMED against current code at publish time.*
