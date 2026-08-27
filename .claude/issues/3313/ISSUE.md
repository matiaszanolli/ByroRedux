# FNV-RT-2026-08-26-01

**Issue**: #3313
**Filed**: 2026-08-26 from `docs/audits/AUDIT_FNV_2026-08-26.md`

---

**Severity**: CRITICAL
**Dimension**: 3 — RT Lighting Pipeline
**Status**: NEW
**Source**: `docs/audits/AUDIT_FNV_2026-08-26.md` (audit HEAD `d6e16c90`)


**File**: `crates/renderer/shaders/water.frag.spv` (stale artifact) vs
`crates/renderer/shaders/include/bindings.glsl:107-243` and
`crates/renderer/src/vulkan/material.rs:353-378` (`GpuMaterial`, 432 B)

**Premise verified** (all against current HEAD, not from memory):

1. `GpuMaterial` grew **364 → 396 → 432 bytes** inside the audit delta:
   `d9d4a6d7` added the eight BGEM glass-optics fields (offsets 364-392),
   `ceb69d24` added the nine Bethesda lighting-response fields (offsets 396-428).
   Pinned host-side by `gpu_material_size_is_432_bytes`
   (`crates/renderer/src/vulkan/material.rs:1489`) and by the per-field offset test at
   `material.rs:1876-1898` (`fresnel_power` @416 … `back_lighting_map_index` @428).
2. Both growth commits edited the **shared** `include/bindings.glsl` struct but recompiled
   **only** `triangle.frag.spv`:

   ```
   $ for f in ui.vert triangle.vert water.frag caustic_splat.comp triangle.frag; do …
   water.frag      SRC: 2c53a089 2026-08-22
   water.frag.spv  SPV: 5f4dea46 2026-08-23     ← predates d9d4a6d7 / ceb69d24 (2026-08-25)
   triangle.frag   SRC: ceb69d24 2026-08-25
   triangle.frag.spv SPV: ceb69d24 2026-08-25
   ```
3. Reflected directly out of the committed binaries — this is the proof, not an inference:

   ```
   $ for f in *.spv; do spirv-dis $f | grep -oP '_runtimearr_GpuMaterial\w* ArrayStride \K\d+'; done
   triangle.frag.spv  Material=[432]  Instance=[160]  Light=[64]
   water.frag.spv     Material=[364]  Instance=[160]  Light=[64]   ← STALE
   (every other .spv reads no material array; GpuInstance/GpuLight strides all agree)

   $ spirv-dis water.frag.spv | grep -c 'MemberDecorate %GpuMaterial'   →  91   (last: member 90 Offset 360)
   $ spirv-dis <freshly compiled>| grep -c 'MemberDecorate %GpuMaterial' → 108   (last: member 107 Offset 428)
   ```
4. `scripts/check-shader-artifacts.sh` (the repo's own gate, wired into
   `.github/workflows/ci.yml:52-69`) is **RED at HEAD**, with the *exact* glslang the script
   demands (`11:16.2.0`, verified locally):

   ```
   DRIFT crates/renderer/shaders/water.frag.spv
   DRIFT crates/renderer/shaders/composite.frag.spv
   check-shader-artifacts: committed SPIR-V is not reproducible from GLSL
   ```
5. The stale binary is what actually ships: `crates/renderer/src/vulkan/water.rs:59`
   `pub(crate) const WATER_FRAG_SPV: &[u8] = include_bytes!("../../shaders/water.frag.spv");`
   `build.rs` does **not** compile shaders (it only generates `shader_constants.glsl`), so a
   `cargo build` never regenerates it.
6. The host writes 432-byte records into the very binding `water.frag` reads:
   `scene_buffer/upload.rs:677` sizes the upload as
   `size_of::<GpuMaterial>() * count`; `scene_buffer/buffers.rs:337-343` binds it as
   set 1 / binding 13 with `FRAGMENT` stage; `bindings.glsl:245` declares
   `layout(std430, set = 1, binding = 13) readonly buffer MaterialBuffer`.
7. The water pass really does index it — five live sites reachable from `water.frag`:
   `water.frag:398` (`GpuMaterial mat = materials[inst.materialId];`),
   `include/shadow_transport.glsl:64` and `:126`,
   plus `include/ray_hit.glsl`'s consumers of the record it is handed.

**Impact (FNV-visible)**: every FNV **exterior water body** — Lake Mead, the Colorado,
Vegas fountains/pools, Camp Golf pond, and the flooded Vault interiors — shades its RT
reflection / refraction / alpha-skip / transmittance hits from a **misaligned material
record** whenever `materialId != 0`. Element *k* is fetched from byte `364·k` instead of
`432·k`, i.e. `68·k` bytes early: at `materialId = 1` the shader reads the tail of material 0
concatenated with the head of material 1; by `materialId = 8` it is 544 bytes adrift.
Only `materialId == 0` (the neutral seeded default) decodes correctly.

Concretely wrong, in order of severity:
- **Bindless descriptor indices become arbitrary u32s.** `ray_hit.glsl:331/342/351` sample
  `textures[nonuniformEXT(mat.parallaxMapIndex)]` and `:415`
  `textures[nonuniformEXT(mat.glowMapIndex)]`. Those u32 lanes now carry whatever float bits
  landed at the shifted offset. The bindless array is a variable-count descriptor array
  (`bindings.glsl:8`, `texture_registry.rs:267`); an out-of-range `nonuniformEXT` index is
  undefined behaviour — the realistic failure modes are a garbage sample, a device fault, or
  a TDR. This is the reason the finding is CRITICAL rather than HIGH.
- `mat.alphaThreshold` / `alphaTestFunc` / `materialKind` are garbage, so
  `rayHitHasCoverage` (`ray_hit.glsl:389-401`) makes wrong alpha-skip decisions —
  alpha-tested foliage/grates either become solid slabs in water reflections or vanish.
- `mat.uvScaleU/V`, `uvOffsetU/V` (`ray_hit.glsl:154`) garbage ⇒ reflected/refracted surfaces
  sample the wrong UVs; `diffuseR/G/B`, `emissiveR/G/B`, `emissiveMult` garbage ⇒ arbitrary
  reflected-hit albedo and emission.

The SSBO itself is never read out of bounds (`364·k + 364 < 432·k + 432` for all `k`, and
`MAX_MATERIALS = 16384`), so this produces **no Vulkan validation error** — it is a pure
data-interpretation fault, invisible to the 755 green renderer tests. The only automated
detector is the CI shell script, which is currently failing on `main`.

**Fix sketch**: recompile the two drifted artifacts with the pinned compiler and commit them
alongside — no source change, no pipeline/barrier change (so it is *not* a speculative
Vulkan fix; the defect is statically proven by `spirv-dis`):
```
glslangValidator -V -I crates/renderer/shaders crates/renderer/shaders/water.frag \
    -o crates/renderer/shaders/water.frag.spv
glslangValidator -V -I crates/renderer/shaders crates/renderer/shaders/composite.frag \
    -o crates/renderer/shaders/composite.frag.spv
scripts/check-shader-artifacts.sh   # must exit 0
```
Use plain `-V` (never `-g0`) per project convention. Longer term the structural gap is
#2748's: nothing in `cargo test` reflects SSBO `ArrayStride` out of the committed SPIR-V —
a `spirv-dis`-free reflection assert (`reflect.rs` already parses these binaries at
`reflect.rs:1138`) comparing each `.spv`'s `GpuMaterial`/`GpuInstance`/`GpuLight` ArrayStride
against `size_of::<…>()` would have failed this commit in the normal test run.

---

---


## HIGH (6)

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **TESTS**: A regression test pins this specific fix
