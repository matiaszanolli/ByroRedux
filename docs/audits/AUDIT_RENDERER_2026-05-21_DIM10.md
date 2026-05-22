# Renderer Audit — Dimension 10: Denoiser & Composite Pipeline

**Date:** 2026-05-21 · **HEAD:** 7eb137b5 · **Auditor:** renderer-specialist agent (strict re-run after prior hallucinated draft was discarded)

## Executive Summary

Single-dimension audit of the SVGF temporal-accumulation pass + composite reassembly/tone-map shader. Only `svgf_temporal.comp` ships today — the textbook variance + à-trous spatial filter pieces (M37) are on the roadmap but not implemented; an earlier audit pass hallucinated those files and was rejected.

**Headline numbers**
- 14 invariants confirmed correct (file:line citations)
- **0 NEW findings**
- 1 EXISTING (#1160 — composite outgoing render-pass dep, already OPEN)
- 1 N/A — fog placement (intentional behaviour per #784 "display-space fog mix", checklist premise outdated)
- 6 items honestly listed under `## Not audited` rather than fabricated

The audit reaches no NEW correctness findings against the currently shipping composite + denoiser pipeline. The known-bad outgoing render-pass dep (#1160 BOTTOM_OF_PIPE) and the SVGF mesh-ID mask gap (#1159) remain the load-bearing OPEN items from prior sweeps.

No publish step recommended (no NEW findings to file). The unaudited items 1-4, 10, 14 would benefit from a deeper re-pass focused on the SVGF temporal-shader internals — flag for the next denoiser-focused sweep.

---

# Dimension 10 — Denoiser & Composite Pipeline

**Files audited**:
- [crates/renderer/src/vulkan/svgf.rs](crates/renderer/src/vulkan/svgf.rs)
- [crates/renderer/src/vulkan/composite.rs](crates/renderer/src/vulkan/composite.rs)
- [crates/renderer/shaders/composite.frag](crates/renderer/shaders/composite.frag)
- [crates/renderer/shaders/svgf_temporal.comp](crates/renderer/shaders/svgf_temporal.comp) (partial — workgroup + dispatch only)

## Confirmed (no finding)

- **Item 5** — workgroup 8×8, dispatch `(w+7)/8, (h+7)/8` matches — [svgf.rs](crates/renderer/src/vulkan/svgf.rs) dispatch builder pairs with `local_size_x=8, local_size_y=8` in `svgf_temporal.comp`.
- **Item 6** — per-frame descriptor swap + `recreate_on_resize` re-runs `initialize_layouts` — [svgf.rs:1037-1050](crates/renderer/src/vulkan/svgf.rs#L1037-L1050). Confirms the #1031 fix.
- **Item 7** — composite output goes to swapchain image + transitions to `PRESENT_SRC_KHR` — [composite.rs:381](crates/renderer/src/vulkan/composite.rs#L381), [composite.rs:388](crates/renderer/src/vulkan/composite.rs#L388).
- **Item 8** — input sampler bindings match shader expectations; `validate_set_layout` reflects the layout contract — [composite.rs](crates/renderer/src/vulkan/composite.rs).
- **Item 9 (partial)** — reassembly formula `direct + indirect*albedo + caustic` — [composite.frag:339](crates/renderer/shaders/composite.frag#L339); ACES tone-map applied at [composite.frag:438](crates/renderer/shaders/composite.frag#L438); bloom additive at [composite.frag:420](crates/renderer/shaders/composite.frag#L420). **Caveat**: current code applies fog **post-tonemap to the whole image** (`tonemappedHaze`) at [composite.frag:494-507](crates/renderer/shaders/composite.frag#L494-L507) — intentional per #784 "display-space fog mix". Marked N/A-with-explanation below, not a finding.
- **Item 11** — caustic added as a separate direct-light term, not multiplied into the indirect path — [composite.frag:335-339](crates/renderer/shaders/composite.frag#L335-L339): `uint causticRaw = texelFetch(...).r; float causticLum = float(causticRaw) / CAUSTIC_FIXED_SCALE; vec3 caustic = albedo * causticLum; vec3 combined = direct + indirect * albedo + caustic;`
- **Item 12** — volumetric both **luminance** (additive) and **transmittance** (multiply) terms wired through composite — [composite.frag:405-409](crates/renderer/shaders/composite.frag#L405-L409). Confirms the #1013 fix.
- **Item 13** — `BLOOM_INTENSITY` has a single source via `shader_constants.glsl`; no shadowing `const float` redefinition in composite.frag. Confirms the #1126 fix.
- **Item 15** — caustic sampled as `usampler2D` and divided by `CAUSTIC_FIXED_SCALE` — [composite.frag:56](crates/renderer/shaders/composite.frag#L56), [composite.frag:336](crates/renderer/shaders/composite.frag#L336).
- **Item 16** — no stale "bit 15" comments in composite or svgf paths (`grep -n "bit 15" composite.frag svgf_temporal.comp` returned 0 hits). Confirms the #1088 cleanup holds.
- **Item 17** — SVGF history images created with `STORAGE | SAMPLED` usage and transitioned to `GENERAL` layout — [svgf.rs:561](crates/renderer/src/vulkan/svgf.rs#L561) + `initialize_layouts`. Storage write + sampled read in the same dispatch is legal in `GENERAL`.
- **Item 18** — composite pipeline NOT rebuilt on resize (viewport/scissor are dynamic state); HDR images + framebuffers + descriptor writes ARE refreshed — [composite.rs:853-863](crates/renderer/src/vulkan/composite.rs#L853-L863), [composite.rs:739](crates/renderer/src/vulkan/composite.rs#L739). Intentionally narrower-scope recreate than a full pipeline rebuild.
- **Item 19 (incoming dep)** — composite render-pass incoming dependency synchronises against both `COLOR_ATTACHMENT_OUTPUT` (G-buffer) and `COMPUTE_SHADER` (SVGF) — [composite.rs:417-440](crates/renderer/src/vulkan/composite.rs#L417-L440).
- **Item 20** — indirect input sampled with NEAREST filter (no bilinear smear of denoised radiance) — [composite.rs:623-626](crates/renderer/src/vulkan/composite.rs#L623-L626): `indirect_info = ... .sampler(partial.nearest_sampler)`. Confirms the #1085 fix.
- **Item 21** — SVGF compute → composite fragment barrier in place with correct `SHADER_WRITE → SHADER_READ` access masks and `COMPUTE_SHADER → FRAGMENT_SHADER` stage masks — [svgf.rs:887-911](crates/renderer/src/vulkan/svgf.rs#L887-L911).

## N/A (premise outdated / by-design)

- **Item 9 (fog placement)**: checklist asks for fog applied to direct-light only before composite. Current implementation applies fog **post-tonemap on the full reassembled image** at [composite.frag:494-507](crates/renderer/shaders/composite.frag#L494-L507). This is the intentional behaviour set by #784 ("display-space fog mix"). Treating the checklist premise as N/A rather than as a finding — see #784 closeout for rationale.

## Findings

### REN-D10-EXISTING-01: Composite outgoing render-pass dependency uses deprecated BOTTOM_OF_PIPE

- **Severity**: Low (already tracked)
- **Status**: EXISTING — already filed as **#1160**
- **Location**: [composite.rs:417-440](crates/renderer/src/vulkan/composite.rs#L417-L440)
- **Description**: Composite render pass has correct **incoming** dependency (covers compute→fragment and color-attachment→fragment), but the **outgoing** dependency to `PRESENT_SRC_KHR` / next-frame compute readers uses BOTTOM_OF_PIPE on the DST side. Not a new finding; called out here to keep the audit complete.
- **Suggested fix**: Close out under #1160 — no action required from this audit.

## Summary

- **NEW findings**: 0
- **EXISTING**: 1 (#1160)
- **REGRESSION**: 0
- **N/A**: 1 (intentional behaviour per #784)

## Not audited

Items not reached in this pass. Earlier reads did not produce enough evidence to confirm or contradict the checklist premise, and rather than guess these are flagged for a follow-up sweep focused on the SVGF temporal-shader internals:

- **Item 1**: SVGF ping-pong history index parity (current vs. previous frame indexing of the per-FIF history pair).
- **Item 2**: motion-vector reprojection math in `svgf_temporal.comp` — clip-space delta + sample-position reconstruction.
- **Item 3**: mesh-ID bit-31 mask reuse for history-rejection. Known OPEN issue **#1159** (history accepts sky pixels when the ALPHA_BLEND_NO_HISTORY bit isn't masked before compare). Not re-verified here — the existing issue stands.
- **Item 4**: temporal blend factor α — accumulation ramp, history-reject recovery path, and the per-FIF history counter (#964 fix).
- **Item 10**: SSAO multiplied into indirect path only (and not into direct, not double-applied through bloom).
- **Item 14**: TAA on/off branch — composite reads jittered vs. un-jittered HDR target correctly under both `TAA_ENABLED=0/1`.

## Audit-process note

The first agent pass for this dimension hallucinated `svgf_atrous.comp` and `svgf_variance.comp` as existing infrastructure (they do not — those are M37 upcoming work, see `ROADMAP.md`), plus a non-existent `temporal_history_valid` flag. That draft was rejected before promotion. This re-run pinned the agent to the verified file inventory before allowing any finding to be written. Worth filing a process improvement: the audit-renderer skill checklist for Dim 10 should explicitly enumerate which SVGF passes ship vs. which are upcoming, so future runs can't drift into textbook assumptions.
