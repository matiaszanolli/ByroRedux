# #4335 — TD7-001: The ground-cover blade buffer is sized by a bare Rust `16` that must equal the std430 stride of the GLSL-only `GroundCoverBlade` struct

**Labels**: high, renderer, shaders, terrain-exterior, tech-debt, bug
**Filed from**: `docs/audits/AUDIT_TECH_DEBT_2026-09-14.md`
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/4335

- **Severity**: HIGH (magic number that silently overflows under a documented design change) · **Dimension**: 7 · **Kind**: tech-debt · **Effort**: small
- **Location**: `crates/renderer/src/vulkan/groundcover.rs:534-537` (allocation), `:1741-1756` (`gpu_records_match_their_std430_layout`, arithmetic only), `crates/renderer/shaders/include/groundcover_scene.glsl:65-80` (struct), `crates/renderer/src/vulkan/scene_buffer/shader_contract_tests.rs:1703` (`("GroundCoverBlade", ShaderLocal)`)
- **Status**: NEW · **Age**: literal `637b652647` (2026-09-06); `ShaderLocal` class `3c16c42e70` (2026-09-08)
- **Finding**: `blade_bytes = GROUNDCOVER_MAX_CHUNKS × GROUNDCOVER_MAX_BLADES_PER_CHUNK × 16`. The `16` is the stride of `GroundCoverBlade { uint packedXZ; float baseY; uint seedSpecies; float dGround; }`, written by `groundcover_scatter.comp` (`gcBlades[chunkIdx * pc.bladesPerChunk + slot]`) and read by `groundcover_blade.vert`. It has no Rust mirror. `MirrorClass::ShaderLocal` only asserts that no Rust struct exists, so nothing compares the size. The sibling Cell/Chunk/Species records are all `size_of`-pinned; the blade record is the one exception. The struct's own doc (`groundcover_scene.glsl:57-64`) weighs extra terms that "would roughly double this record".
- **Impact**: If the record grows, the runtime array holds `16 MiB / stride` entries while the host dispatches `256 × 4096` slots. The high chunk slices read and write past the SSBO end: UB without `robustBufferAccess`, silently dropped with it, and not flagged by default validation. It shows up as missing or corrupt grass with an empty log. This is the lockstep-drift class in `feedback_shader_struct_sync.md`. (Verified by the orchestrator: the struct has exactly four 4-byte fields, and `grep GroundCoverBlade crates/renderer/src` finds only the contract-table row.)
- **Suggested Fix**: Add `#[repr(C)] GpuGroundCoverBlade { packed_xz: u32, base_y: f32, seed_species: u32, d_ground: f32 }`, size `blade_bytes` with `size_of`, pin it in `gpu_records_match_their_std430_layout`, and reclassify the contract entry `Guarded` so the std430 field check compares both sides.

**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-09-14.md` (HEAD `358999c40`)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shaders / block parsers / skill files / docs)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **TESTS**: A regression test (or gate) pins this specific fix
