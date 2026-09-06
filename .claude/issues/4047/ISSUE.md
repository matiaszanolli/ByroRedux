# #4047 — REN-2026-09-06-D8-03: two stale words in `/audit-renderer`'s own Dimension 8 checklist — "fog applied to direct only" and "caustic sampled via `usampler2D`" — both describe shapes the composite no longer has

**Labels**: low, renderer, shaders, tech-debt, bug

---

**Source**: `docs/audits/AUDIT_RENDERER_2026-09-06.md` (REN-2026-09-06-D8-03), full 23-dimension `/audit-renderer` sweep at `229306ce`.
Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.

- **Severity**: LOW
- **Dimension**: Denoiser/Composite
- **Location**: `.claude/commands/audit-renderer/SKILL.md` (Dimension 8, the "Composite reassembly" and "Caustic accumulator" bullets)
- **Status**: NEW — audit-infrastructure doc-rot; **the code is correct on both counts**
- **Description**: Two Dim-8 checklist assertions no longer match
  `crates/renderer/shaders/composite.frag`:
  1. *"Fog applied to direct only, not indirect."* The live shader attenuates
     the fully reassembled term: `combined = combined * vol.a + vol.rgb` for the
     froxel integral and `combined = combined * transmittance + aerial` for the
     beyond-grid tail, where `combined` is `direct + indirect * albedo + caustic`.
     Attenuating only the direct half would be physically wrong (transmittance
     applies to all radiance leaving the surface), so the **checklist is the
     stale side**, not the code.
  2. *"Caustic accumulator (`R32_UINT`) sampled via `usampler2D`."* The
     glass/MLP accumulator is `layout(set = 0, binding = 5) uniform usampler2DArray causticTex`
     — RGB across three `R32_UINT` array layers, fetched with
     `texelFetch(causticTex, ivec3(causticPixel, c), 0)`. Only the *water*-side
     accumulator (`binding = 8`, `waterCausticTex`) is a plain `usampler2D`.
- **Evidence**: `composite.frag`'s `glassCausticRaw` block and the two
  `combined = combined * …` fog lines; `docs/engine/shader-pipeline.md` does not
  cover either point, so the SKILL is the only place carrying them.
- **Impact**: An auditor working the Dim 8 checklist literally will either file
  a false positive against correct fog handling, or spend the bullet confirming
  a `usampler2D` that only half exists. This is the class the audit-common
  "verify the premise against current code" rule exists for.
- **Related**: `feedback_audit_findings.md` (stale-premise hygiene).
- **Needs RenderDoc**: no.
- **Suggested Fix**: Reword to *"Fog/volumetric transmittance applied to the
  reassembled `combined` (direct + indirect·albedo + caustics), not to `direct`
  alone"* and *"Caustic accumulators: glass/MLP is a three-layer
  `usampler2DArray`, water-side is a `usampler2D`; both divided by
  `CAUSTIC_FIXED_SCALE`."* Also correct the Dim 13 entry-point path — the
  `(jx, jy)` block now lives in `context/assemble_camera_and_lights.rs`, not
  `context/draw.rs`. Run `.claude/commands/_audit-validate.sh` after.

---

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files (other shader mirrors, other passes)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **SPIRV**: If GLSL changed, recompile the `.spv` (`cd crates/renderer/shaders && glslangValidator -V -I. <shader> -o <shader>.spv`) and commit it
- [ ] **TESTS**: A regression test pins this specific fix
