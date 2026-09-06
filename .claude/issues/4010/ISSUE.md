# #4010 — REN-2026-09-06-D15-01: `water.frag`'s caustic refraction hardcodes `1.0 / 1.33` while its own primary refraction ray uses the authored `WaterMaterial::ior`

**Labels**: low, renderer, shaders, water, bug

---

**Source**: `docs/audits/AUDIT_RENDERER_2026-09-06.md` (REN-2026-09-06-D15-01), full 23-dimension `/audit-renderer` sweep at `229306ce`.
Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.

- **Severity**: LOW (dormant today — nothing in the cell loader currently overrides the 1.33 default; becomes a visible divergence the moment a WATR record or a tuning pass sets one)
- **Dimension**: Water (water-side caustics)
- **Location**: `crates/renderer/shaders/water.frag` — the caustic block's `refract(-sunDir, causticNormal, 1.0 / 1.33)`, versus the primary refraction's `float eta = viewFromPositiveSide ? (1.0 / max(ior, 1.0)) : max(ior, 1.0);` where `float ior = push.timing.w;` (`push` is the `#define push waterParams.params[drawPush.waterIndex]` alias for one `WaterParams` SSBO record, **not** a Vulkan push constant). CPU side: `WaterMaterial::ior` (`crates/core/src/ecs/components/water.rs`, default `1.33`) → `GpuWaterParams::timing[3]` (`crates/renderer/src/vulkan/water.rs`), filled from `mat.ior` in `byroredux/src/render/water.rs`. Sibling writer: `crates/renderer/shaders/caustic_splat.comp`.
- **Status**: **NEW.** No open issue (searched `ior`, `1.33`, `caustic`). Not raised by the 2026-09-04 `water-deep` run, which examined this block for its bounds guard (`REN-WD-D2-01`, now fixed as #3820) rather than its eta.
- **Description**: The two caustic writers were deliberately aligned on everything else — the `CAUSTIC_FIXED_SCALE` fixed-point basis, the normalised 5×5 footprint, the `sunDirection` points-to-the-sun convention (`#1635`/`#1459`), `offsetRayOriginForDirection`'s zero-`tMin` origin contract, and (as of #3820) the `imageSize`-based bounds rule. They are **not** aligned on where the refractive index comes from, and the glass side is the one that does it correctly:

  ```glsl
  // caustic_splat.comp — reads the per-draw value, falls back to the pipeline default
  float instanceIor = instances[instIdx].ior;
  float ior = instanceIor > 1.0 ? instanceIor : causticTune.y;

  // water.frag — primary refraction ray, authored value
  float ior = push.timing.w;
  float eta = viewFromPositiveSide ? (1.0 / max(ior, 1.0)) : max(ior, 1.0);

  // water.frag — caustic refraction ray, literal
  vec3 refractDir = refract(-sunDir, causticNormal, 1.0 / 1.33);
  ```

  `WaterMaterial::ior` is canonical, authorable WATAL state whose own doc says *"1.33 = clean water; bumping up to 1.5 for stylised reads or thick visc fluid"* — i.e. the field exists precisely to be varied. The block's comment (*"η = 1.0/1.33 (air → water)"*) reads as a restatement of the default, not as a deliberate decision to ignore the authored value; nothing nearby argues for independence, and the same block already reuses `sunVisibility` computed further up rather than recomputing it.
- **Evidence**: `grep -n "ior\|1\.33" crates/renderer/shaders/water.frag` → `float ior = push.timing.w;` and `1.0 / max(ior, 1.0)` in the refraction block, `1.0 / 1.33` in the caustic block. `grep -n "ior" crates/core/src/ecs/components/water.rs` → `pub ior: f32` with `ior: 1.33` in `Default`. `grep -rn "ior" byroredux/src/render/water.rs` → `mat.ior` into `timing`. No cell-loader or EXAL site currently writes `WaterMaterial::ior`, so the two agree at runtime today. The Rust↔GLSL agreement of the record itself is separately pinned by `gpu_water_params_rust_and_glsl_copies_stay_in_lockstep` — the *record* is guarded, only its consumer diverges.
- **Impact**: Latent. Any authored or tuned water IOR ≠ 1.33 makes the caustic pattern on the lake bed refract at a different angle than the visible refraction of the same surface — the caustic focus and the seen-through geometry disagree, which reads as the caustic being registered to the wrong place rather than as a colour/intensity error. This is also exactly the failure mode `86976f56` (#3912) swept for the glass defaults one day earlier: a canonical constant plumbed to one consumer and hardcoded at another.
- **Related**: #3912 / `86976f56` (the named-default doctrine this violates), #3745 (`887c5d18`, which consolidated `water.frag`'s three RT reach budgets into `shader_constants_data.rs` — the precedent for removing literals from this shader), #1210 / #1255 (the water caustic phases), #3820 (the last time the two writers were brought into agreement).
- **Suggested Fix**: One-line change — `refract(-sunDir, causticNormal, 1.0 / max(ior, 1.0))`, reusing the `ior` local already in scope from line ~624, and drop the `1.33` from the comment. Pinnable by the `water.rs` source-assertion test style already used for the `#3820` bound (`crates/renderer/src/vulkan/water.rs` has a matching test for the `imageSize` rule); a negative assertion that `water.frag` contains no `1.0 / 1.33` would be the direct guard. Requires a `.spv` recompile.

---

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files (other shader mirrors, other passes)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **SPIRV**: If GLSL changed, recompile the `.spv` (`cd crates/renderer/shaders && glslangValidator -V -I. <shader> -o <shader>.spv`) and commit it
- [ ] **TESTS**: A regression test pins this specific fix
