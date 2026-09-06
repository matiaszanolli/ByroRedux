# #4008 — REN-2026-09-06-D13-02: `#3607` closed the discoverability half of the five-copy `octDecode` duplication but not the drift half — every guard is a name/count pin, and the shared copy in `include/math_common.glsl` is declared non-standalone

**Labels**: low, renderer, shaders, tech-debt, test-gap, bug

---

**Source**: `docs/audits/AUDIT_RENDERER_2026-09-06.md` (REN-2026-09-06-D13-02), full 23-dimension `/audit-renderer` sweep at `229306ce`.
Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.

- **Severity**: LOW
- **Dimension**: TAA
- **Location**: `crates/renderer/shaders/taa.comp`, `crates/renderer/shaders/svgf_temporal.comp`, `crates/renderer/shaders/svgf_atrous.comp`, `crates/renderer/shaders/caustic_splat.comp`, `crates/renderer/shaders/include/math_common.glsl`; guard `taa_comp_octahedral_decoder_is_named_octdecode` (`crates/renderer/src/vulkan/taa.rs`)
- **Status**: NEW (residual of closed `#3607`; the rename itself verified complete — see Coverage)
- **Description**: The rename landed correctly and the maintenance comments in
  all four `.comp` copies now enumerate each other. But `taa.comp`'s own comment
  states the residual hazard verbatim: *"a divergence here is a silent, per-pixel
  difference in a history-rejection predicate, invisible to every existing test
  since all of them are source-scan pins."* That is still true. The two guards
  are `taa_comp_octahedral_decoder_is_named_octdecode` (asserts the string
  `vec3 octDecode(vec2 e)` is present, that `oct_decode` is absent, and that
  `octDecode(` occurs exactly 3×) and
  `taa_comp_keeps_history_bounded_and_rejects_unstable_surfaces` (asserts the
  reject-list expression). Neither compares the *bodies*. A one-line edit to
  three of the four copies would still pass everything.
  A fifth copy exists that the enumerations do not name:
  `include/math_common.glsl` already defines `octDecode` next to `octEncode`.
  Note the obvious fix is **not** a drop-in: that header opens with *"NON-STANDALONE
  shader fragment … it references symbols (structs, SSBO/UBO bindings, helper
  functions, constants) defined in `shader_constants.glsl` and in earlier
  includes"* (`sampleDalcCube` reads `dalcPosX` &c.), so the compute shaders
  cannot `#include` it as-is; the codec would have to be split into its own
  standalone header first.
- **Evidence**: I extracted the five bodies and compared them — identical apart
  from where the `vec2(...)` argument list wraps. `grep -rn "math_common.glsl" crates/renderer/shaders/`
  returns four prose mentions plus exactly one real `#include`, from
  `triangle.frag`.
- **Impact**: Drift risk only, on the predicate that gates TAA history
  acceptance (`dot(currNormal, prevNormal) < 0.85`) and SVGF's bilinear
  consistency loop (`dot(currN, prevN) < 0.9`). A future correction to the codec
  (precision, dropping the `normalize`, an snorm-range change) applied to some
  copies leaves TAA rejecting history differently from SVGF and from the
  `octEncode` producer, with no test failing.
- **Related**: `#3607` (`20f5f476`); the 2026-08-30 `D13-04`.
- **Needs RenderDoc**: no.
- **Suggested Fix**: Cheapest closure is a body-equality source scan next to the
  existing pin: extract the `vec3 octDecode(vec2 e) { … }` span from all five
  files, strip whitespace, and assert all five are equal. Real fix is to split a
  standalone `include/oct_codec.glsl` out of `math_common.glsl` and have all
  five `#include` it.

---

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files (other shader mirrors, other passes)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **SPIRV**: If GLSL changed, recompile the `.spv` (`cd crates/renderer/shaders && glslangValidator -V -I. <shader> -o <shader>.spv`) and commit it
- [ ] **TESTS**: A regression test pins this specific fix
