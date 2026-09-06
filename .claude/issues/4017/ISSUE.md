# #4017 — REN-2026-09-06-D2-02: `giHitIrradiance` has been dead shader code since 2026-07-29, and yesterday's `GI_VISIBLE_LIGHT_CAP` promotion pinned a CPU-side contract constant to its only (unreachable) consumer

**Labels**: low, renderer, shaders, tech-debt, bug

---

**Source**: `docs/audits/AUDIT_RENDERER_2026-09-06.md` (REN-2026-09-06-D2-02), full 23-dimension `/audit-renderer` sweep at `229306ce`.
Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.

- **Severity**: LOW
- **Dimension**: Ray Queries
- **Location**: `crates/renderer/shaders/include/lighting.glsl` —
  `giHitIrradiance` (definition) and its `GI_VISIBLE_LIGHT_CAP` loop bound.
  Live siblings: `pathHitRadiance` (same file, `visibleLightLimit` parameter)
  and `reflectionHitIrradiance`. Caller: `crates/renderer/shaders/triangle.frag`,
  the `pathHitRadiance` call in the GI path. Constant:
  `crates/renderer/src/shader_constants_data.rs` (`GI_VISIBLE_LIGHT_CAP`),
  emitted into `crates/renderer/shaders/include/shader_constants.glsl`. Stale
  comments: `crates/renderer/shaders/include/raytrace.glsl` (the
  `reflectionHitIrradiance` prototype block) and `triangle.frag`'s
  refraction-terminus lighting comment.
- **Status**: **NEW.** Verified against code, not against GitHub (the issue
  cache is open-only). `grep -rn giHitIrradiance crates/renderer/shaders`
  returns exactly three hits: the definition and two comments. No call site.
- **Description**: `f8efde63` (2026-07-29, "bounded material-aware path-traced
  GI") replaced `triangle.frag`'s last `giHitIrradiance` call with
  `pathHitRadiance`; `6c56e311` (2026-07-19) had already moved the refraction
  terminus onto `reflectionHitIrradiance`. `giHitIrradiance` — a 52-line
  candidate-selection + visibility-ray function inside the main fragment
  shader's include chain — has had **no caller for ~5½ weeks**.

  It is also the **sole consumer** of `GI_VISIBLE_LIGHT_CAP`. Yesterday's
  `78cc7a41` (#3879/#3880) identified that constant as "the one genuine
  duplicate" its widened declaration gate exposed and promoted it into
  `shader_constants_data.rs` → the generated header, i.e. into the Rust↔GLSL
  contract. The promotion is therefore anchored to unreachable code, while the
  **live** instance of the same "first two visible contributors" number —
  `uint visibleLightLimit = shadedHits == 0 ? 2u : 1u;` at the `pathHitRadiance`
  call site in `triangle.frag` — remains a bare literal. The gate #3880 widened
  matches *declarations* (`const T` and object-like `#define`); a value inlined
  into an expression is invisible to it by construction, so it cannot close
  this last copy.

  Two comments assert the retired split and are now false: `raytrace.glsl`'s
  prototype block says *"Diffuse GI and refraction termini retain the wider
  locally-selected light set in `giHitIrradiance`"* — the refraction terminus
  calls the deliberately-narrow `reflectionHitIrradiance` (which returns after
  the **first** visible light) and the GI bounce calls `pathHitRadiance`; and
  `triangle.frag`'s refraction-terminus comment names `giHitIrradiance` as the
  function it shares its evaluation with.
- **Evidence**: `grep -rn "giHitIrradiance\|pathHitRadiance(\|reflectionHitIrradiance("
  crates/renderer/shaders` → `pathHitRadiance` one call site (`triangle.frag`,
  GI path), `reflectionHitIrradiance` two (`triangle.frag` refraction terminus +
  `raytrace.glsl`'s own hit shading, plus the prototype), `giHitIrradiance`
  **zero**. `git log -S "giHitIrradiance(" -- crates/renderer/shaders/triangle.frag`
  ends at `f8efde63` (2026-07-29). `GI_VISIBLE_LIGHT_CAP` appears in
  `lighting.glsl` once (`giHitIrradiance`'s `visibleCount >=` bound), in the
  generated header, and in `shader_constants_data.rs`.
- **Impact**: No runtime effect — dead code plus three misleading contract
  statements. The cost is auditing and maintenance: a reader tracing the GI
  light budget lands on the wrong function, and a maintainer retuning
  `GI_VISIBLE_LIGHT_CAP` in `shader_constants_data.rs` will change nothing on
  screen while the live cap sits in `triangle.frag` as `2u`. That is the exact
  silent-no-op failure mode #3879 was filed to remove.
- **Related**: #3879/#3880 (`78cc7a41`, the promotion); `f8efde63` (the commit
  that orphaned the function); #3868 (`triangle.frag` present-tense comments
  describing retired pipeline stages — same class).
- **Suggested Fix**: Delete `giHitIrradiance`, and either delete
  `GI_VISIBLE_LIGHT_CAP` with it or — better — repoint it at the live copy by
  replacing `triangle.frag`'s `shadedHits == 0 ? 2u : 1u` with
  `shadedHits == 0 ? GI_VISIBLE_LIGHT_CAP : 1u`, which makes the promoted
  constant load-bearing instead of decorative. Correct the two comments to name
  `pathHitRadiance` / `reflectionHitIrradiance`.

---

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files (other shader mirrors, other passes)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **SPIRV**: If GLSL changed, recompile the `.spv` (`cd crates/renderer/shaders && glslangValidator -V -I. <shader> -o <shader>.spv`) and commit it
- [ ] **TESTS**: A regression test pins this specific fix
