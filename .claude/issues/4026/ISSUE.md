# #4026 — REN-2026-09-06-D23-04: the FSR plan's phase-3 status line still says exposure is "consumed by the composite tonemap" — contradicted five lines later in the same header

**Labels**: low, renderer, shaders, documentation, doc-rot

---

**Source**: `docs/audits/AUDIT_RENDERER_2026-09-06.md` (REN-2026-09-06-D23-04), full 23-dimension `/audit-renderer` sweep at `229306ce`.
Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.

- **Severity**: LOW
- **Dimension**: FSR/Presentation (doc-rot)
- **Location**: `docs/engine/fsr3-upscaler-integration-plan.md`, the status
  header's phase-3 paragraph ("…the 1×1 `R32_SFLOAT` exposure producer consumed
  by the composite tonemap…")
- **Status**: NEW
- **Description**: Phase 4 moved exposure and ACES out of composite into the
  output-resolution presentation pass, and the very next paragraph of the same
  header says so ("an output-resolution presentation pass that owns exposure +
  ACES"). The phase-3 sentence was never updated. It is checkable and wrong:
  `composite.frag` contains no `exposure` uniform and no `aces()` — verified by
  grep — and `composite.rs`'s single mention of exposure is a comment pointing
  the reader at `frame_upscaler.rs` / `exposure.rs`. The live consumer is
  `presentation.frag`'s `vec3 presented = aces(graded * params.exposure)`.
- **Evidence**: `grep -i "exposure\|aces" crates/renderer/shaders/composite.frag`
  returns only prose comments about pre-ACES linear space (composite's *output*
  is pre-tone-map by design); the sole `params.exposure` reader in the tree is
  `presentation.frag`.
- **Impact**: This is the authoritative FSR document, and exposure agreement
  between the upscaler and the tone-mapper is exactly the invariant #2833 was
  filed about (`NO_EXPOSURE_RESOURCE_FALLBACK`). A reader chasing an exposure
  mismatch is sent to the wrong shader. Documentation only.
- **Related**: #2833, `docs/engine/shader-pipeline.md` (which the SKILL's Dim 8
  bullet already records correctly: "ACES tone-map is NOT in `composite.frag` —
  it lives in `presentation.frag`")
- **Suggested Fix**: Change "consumed by the composite tonemap" to "consumed by
  the presentation tone-map (`presentation.frag`, since phase 4)". One clause.

---

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files (other shader mirrors, other passes)
- [ ] **SPIRV**: If GLSL changed, recompile the `.spv` (`cd crates/renderer/shaders && glslangValidator -V -I. <shader> -o <shader>.spv`) and commit it
- [ ] **TESTS**: A regression test pins this specific fix
