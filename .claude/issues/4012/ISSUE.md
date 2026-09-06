# #4012 — REN-2026-09-06-D17-03: `disneyDiffuseSplit`'s doc block still describes two call sites; the fallback-directional arm it names was deleted

**Labels**: low, renderer, shaders, tech-debt, documentation, doc-rot

---

**Source**: `docs/audits/AUDIT_RENDERER_2026-09-06.md` (REN-2026-09-06-D17-03), full 23-dimension `/audit-renderer` sweep at `229306ce`.
Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.

- **Severity**: LOW
- **Dimension**: Disney BSDF
- **Location**: `crates/renderer/shaders/include/pbr.glsl` (the `disneyDiffuseSplit` doc block)
- **Status**: NEW
- **Description**: The split-return rationale reads "The two call sites (fallback-directional and per-light loop) need to compose them with different PI scales because the per-light loop carries a `kD * albedo` (no /PI) legacy convention." There is now exactly **one** call site. `lighting.glsl`'s own else-arm comment records the change ("Directional, point and spot sources all arrive through this function; `triangle.frag` no longer carries a duplicate synthetic no-light sun/BRDF arm"), and `disney_sheen_keeps_its_relative_weight_in_canonical_direct_path` actively asserts the second site's *absence* (`!frag.contains("diffuseBrdf = (dd.diffuse + dd.sheen)")`). The struct return is still the right shape — `disneyDiffuseSplit`'s consumer does need diffuse and sheen separable, and the doc is the only place the π-scaling contract is written down — so the fix is to restate the rationale against the surviving single site, not to collapse the struct.
- **Evidence**: `grep -rn "disneyDiffuseSplit(" crates/renderer/shaders/` returns one call (`lighting.glsl`) plus the definition.
- **Impact**: Documentation only. It misleads a reader into thinking a second composition convention exists that must be kept in step — which is how the `* PI` vs no-`* PI` asymmetry #2243 fixed got introduced in the first place. The `/audit-renderer` skill's Dimension 17 text has already absorbed the stale claim verbatim ("the normalized direct-sun path (`triangle.frag`, which keeps `(dd.diffuse + dd.sheen) * (1.0 - metalness)`)"), so it is propagating.
- **Related**: #1252 (the split return), #2243 (the π-scaling fix), #3868 (`triangle.frag`'s own present-tense doc rot). Adjacent instance worth folding into the same edit: `triangle.frag`'s RT-glass block still says "Glass has IOR ≈ 1.5 (soda-lime, window glass, drinking glass)" thirty lines after the correct note that "Canonical glass carries `mat.ior = 1.45` (`GLASS_SURFACE_BEHAVIOR`)".
- **Suggested Fix**: Rewrite the paragraph to name the single surviving call site and state why the struct is still the right return shape (the caller must apply `* PI` to the *sum*, so the two lobes must arrive separable). Update the skill's Dimension 17 bullet in the same pass.

---

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files (other shader mirrors, other passes)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **SPIRV**: If GLSL changed, recompile the `.spv` (`cd crates/renderer/shaders && glslangValidator -V -I. <shader> -o <shader>.spv`) and commit it
- [ ] **TESTS**: A regression test pins this specific fix
