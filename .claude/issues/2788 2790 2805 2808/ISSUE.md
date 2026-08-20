# #2788: REN-D4-06: shader-pipeline.md Per-Frame Submission Order omits copy_depth_to_history step and health-counter harvest

**Labels**: documentation, renderer, low
**State**: OPEN

## Description
The authoritative 22-step order omits `copy_depth_to_history` (a whole `TRANSFER`-stage step between 5 and 6 that transitions the depth image twice) and the step-21 health-counter harvest. `depth_history_image` is absent from the doc entirely, including the G-Buffer table — and **#2484 and #2485 are open findings about exactly that barrier and that image**.

## Location
`docs/engine/shader-pipeline.md` (§ Per-Frame Submission Order)

---
Filed from docs/audits/AUDIT_RENDERER_2026-08-12b.md (finding REN-D4-06).

---

# #2790: REN-D15-08: watal.md §2 stale re #1502 fix status and resolve_water_material line cite

**Labels**: documentation, renderer, low
**State**: OPEN

## Description
(a) Lists the #1502 procedural-noise banding as a *current* fragility; it is fixed — `sampleScrollingNormal` and `foamFlowStreaks` both subtract `originOffset` before hashing, and the textured branch stays absolute *deliberately* with the #2496 texel-integral bound. The Dim-15 brief asks that #1502 be recast as a regression guard; the doc contradicts that and invites a re-fix that would re-break the deliberate absolute-UV branch. (b) Cites `resolve_water_material` at `env_translate.rs:89-176`; the function is near line 352.

## Location
`docs/engine/watal.md` §2

---
Filed from docs/audits/AUDIT_RENDERER_2026-08-12b.md (finding REN-D15-08).

---

# #2805: REN-D16-06: BLOOM_INTENSITY has two contradictory documented derivations

**Labels**: documentation, renderer, low
**State**: OPEN

## Description
`BLOOM_INTENSITY = 0.15` carries **two mutually exclusive documented derivations** — one says it absorbs the un-normalised 5× DC gain relative to Frostbite's 0.04, the other says it compensates LDR-authored Bethesda content; the 4× factor is spent once in each comment on a different justification, and absorbing a 5× gain against 0.04 would require ≈ 0.008. Measurable consequence: the effective DC weight is **0.75×** the local blurred average (~19× the normalised reference), and `bloom_downsample.comp` applies **no bright-pass threshold or Karis average** (`DownsampleParams` carries only `inv_resolutions`), so this is a broadband lift, not a highlight-only glow. Filed as a documentation contradiction plus a quantified observation — **not** a claim the image is wrong.

## Location
`shader_constants_data.rs`, `crates/renderer/src/vulkan/bloom.rs`, `crates/renderer/shaders/bloom_upsample.comp`, `crates/renderer/shaders/bloom_downsample.comp`

---
Filed from docs/audits/AUDIT_RENDERER_2026-08-12b.md (finding REN-D16-06).

---

# #2808: REN-D17-07: Stale spec-color-as-F0 comment block in triangle.frag contradicts live F0 assignment

**Labels**: documentation, renderer, low
**State**: OPEN

## Description
Still documents, in the present tense, the spec-colour-as-F0 branch that `31c99bb3` deleted ("So for PBR materials we use the authored spec_color as F0 directly"), reversing course only in its final third. There is no such branch: `F0` is assigned exactly twice, both `f0Dielectric`-derived. The stale half also contradicts the live CPU contract described by #2703, so a reader trusting it looks for the bug in the wrong layer — on the single largest comment block in the F0 region, which is where someone goes to ask "why does my FO4 metal panel look plastic".

## Location
`crates/renderer/shaders/triangle.frag` (the ~60-line F0 comment block)

---
Filed from docs/audits/AUDIT_RENDERER_2026-08-12b.md (finding REN-D17-07).
