# #3984 — REN-2026-09-06-D17-02: the anisotropic GGX branch is the one TBN builder in the shader tree without the #2815 post-Gram-Schmidt zero guard — `normalize()` on a zero vector poisons `Lo`, the ReSTIR reservoir and the EMA history with NaN

**Labels**: medium, renderer, shaders, bug

---

**Source**: `docs/audits/AUDIT_RENDERER_2026-09-06.md` (REN-2026-09-06-D17-02), full 23-dimension `/audit-renderer` sweep at `229306ce`.
Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.

- **Severity**: MEDIUM
- **Dimension**: Disney BSDF
- **Location**: `crates/renderer/shaders/include/lighting.glsl` (`shadowableLightRadiance`, the `mat.anisotropic > 0.0` branch)
- **Status**: NEW
- **Description**: The anisotropic branch rebuilds a tangent frame from the interpolated vertex tangent:

  ```glsl
  vec3 T = normalize(fragTangent.xyz);
  T = normalize(T - dot(T, N) * N);
  ```

  The guard above it (`dot(fragTangent.xyz, fragTangent.xyz) > 1e-4`) proves only that the **raw** tangent is non-zero, not that it is non-parallel to the shading normal `N`. When `T ∥ N` the Gram-Schmidt projection is the zero vector and `normalize()` on it is `0/0` → NaN. This is precisely the hazard #2815 / REN-D19-04 fixed in `perturbNormal`, and `material_sampling.glsl`'s comment there enumerates the sibling builders that already carry the guard — `parallaxDisplaceUV`'s `if (dot(T, T) < 1e-8 || heightScale <= 0.0) return uv;` and `getRayHitTangentFrame`'s `if (dot(worldT, worldT) < 1e-8) return false;`. This fourth builder is not in that list and does not have the guard.

  `N` here is the *normal-mapped* shading normal (or `glassViewNormal`), not the geometric normal, so a strongly-perturbing normal map is enough to rotate `N` into the authored tangent's direction; `perturbNormal`'s own guard exists on exactly that reasoning.
- **Evidence**: The three guarded siblings vs the unguarded fourth, all in the same `#include` chain:
  - `material_sampling.glsl` / `perturbNormal`: `if (dot(Tproj, Tproj) < 1e-8) { return N; }`
  - `material_sampling.glsl` / `parallaxDisplaceUV`: `if (dot(T, T) < 1e-8 || heightScale <= 0.0) { return uv; }`
  - `ray_hit.glsl` / `getRayHitTangentFrame`: `worldT -= dot(worldT, N) * N; if (dot(worldT, worldT) < 1e-8) { return false; }`
  - `lighting.glsl` / `shadowableLightRadiance`: `T = normalize(T - dot(T, N) * N);` — no post-projection test.
- **Trigger Conditions**: Requires `mat.anisotropic > 0.0`. `translate_material` pins `anisotropic: 0.0` for every source format (guarded by `anisotropic_rationale_matches_what_the_source_formats_carry`), so **no shipped game content reaches this branch** — but it is deliberately reachable through the two producers built for it: `cornell::pbr_bsdf_lobes` (the `--cornell` Disney-lobe probe, `anisotropic = 0.1`) and the live `mat.set <id> anisotropic <v>` console arm (`commands/scene.rs`), both added by #2514 / REN-D21-2026-08-07-02 precisely so the anisotropic lobe could be exercised.
- **Impact**: A NaN emitted here does not stay local. `shadowableLightRadiance` is the shared BRDF for the pass-1 accumulation, the ReSTIR `pHat` score, and the legacy shadow subtraction, so a single bad fragment produces `restirWSum`/`restirPHat` NaN (→ `restirW` NaN, which the `!isnan(rp.W)` reuse gates then propagate to *neighbouring* pixels through spatial reuse) and a NaN `accum` in the EMA history, which the `mix(prevAccum, …)` recurrence can never clear. The result is a persistent, spreading black/white blot rather than a one-frame speck — and it lands first on the Cornell harness built to validate this exact lobe, which is where the false all-clear costs most.
- **Related**: #2815 / REN-D19-04 (the identical fix in `perturbNormal`), #2512 (the sibling `fragTangent.w` ±1 clamp this branch *does* have), #2514 (the constructors that make the branch reachable), #1250 (the anisotropic lobe itself).
- **Suggested Fix**: Mirror `perturbNormal` exactly — project first, test the projected length, and fall back to the isotropic `distributionGGX(NdotH, aaRoughness)` when it collapses:
  ```glsl
  vec3 Tproj = T - dot(T, N) * N;
  bool anisoUsable = dot(Tproj, Tproj) >= 1e-8;
  ```
  gating the existing `distributionGGXAniso` call on `anisoUsable`. Add it to the sibling list in `material_sampling.glsl`'s comment so the four builders stay enumerated, and pin with a `shader_contract_tests.rs` assertion that the branch contains a post-projection length test.

---

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files (other shader mirrors, other passes)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **CANONICAL-BOUNDARY**: per-game logic stays at the NIFAL parser->`Material` boundary - never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix
