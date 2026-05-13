# Issue #992

**URL**: https://github.com/matiaszanolli/ByroRedux/issues/992
**Title**: REN-MESH-ID-32: Flip MESH_ID_FORMAT from R16_UINT to R32_UINT — MAX_INSTANCES past 0x7FFF (Solitude / Whiterun draw / Diamond City scale)
**Labels**: enhancement, renderer, medium, vulkan
**Parent**: 67da5af (R16_UINT ceiling bump)

---

**Severity**: MEDIUM (latent — fires only on cells past 32767 instances, which today is Solitude / Whiterun draw distance / Diamond City / dense FO4 settlements). Once it fires, every shadow / reflection / SVGF disocclusion query against the wrapped instance silently misroutes — visible as cross-instance ghosting + flicker.

**Domain**: renderer + 4 shaders

**Source**: deferred from \`67da5af\` ("Bump MAX_INSTANCES + MAX_INDIRECT_DRAWS to 0x7FFF") — that commit pinned the cap at the maximum the R16_UINT path can address. This issue is the architectural follow-up.

## Why this needs to land

The mesh_id G-buffer attachment is currently \`R16_UINT\`. \`triangle.frag:980\` writes \`(instance_index + 1) & 0x7FFFu\` into the low 15 bits and reserves bit 15 (\`0x8000\`) for the \`ALPHA_BLEND_NO_HISTORY\` flag. With 15 effective bits and meshId=0 reserved for "sky / no instance", the maximum addressable instance index is **32766** (yielding meshId 32767). Anything past that wraps to meshId 0 and collides with the sky sentinel.

The runtime \`debug_assert!\` in \`crates/renderer/src/vulkan/context/draw.rs:1255\` catches it in debug builds. A new compile-time pin \`max_instances_stays_within_r16_uint_mesh_id_ceiling\` (scene_buffer.rs) prevents bumping \`MAX_INSTANCES\` past 0x7FFF without also doing this work. Both fire intentionally to force this issue.

## Scope

**Touches 5 files:**

1. \`crates/renderer/src/vulkan/gbuffer.rs\` — \`MESH_ID_FORMAT: vk::Format = vk::Format::R16_UINT\` → \`R32_UINT\`. The image-view + clear-color + format reflection downstream pick up the change automatically.

2. \`crates/renderer/shaders/triangle.frag\` — \`outMeshID\` already declared as \`uint\` so the type is fine. Encoding changes:
   ```glsl
   // Before (R16_UINT, bit 15 = alpha-blend):
   uint meshIdBase = (uint(fragInstanceIndex) + 1u) & 0x7FFFu;
   outMeshID = meshIdBase | (alphaBlendFrag ? 0x8000u : 0u);

   // After (R32_UINT, bit 31 = alpha-blend):
   uint meshIdBase = (uint(fragInstanceIndex) + 1u) & 0x7FFFFFFFu;
   outMeshID = meshIdBase | (alphaBlendFrag ? 0x80000000u : 0u);
   ```

3. \`crates/renderer/shaders/taa.comp\` — disocclusion read updates from \`0x7FFF\`/\`0x8000\` masks to \`0x7FFFFFFF\`/\`0x80000000\`. Lines 143, 152.

4. \`crates/renderer/shaders/svgf_temporal.comp\` — same mask update. Lines 93, 142.

5. \`crates/renderer/shaders/caustic_splat.comp\` — same mask update. Lines 122, 130. Also the sampler comment at line 30 (\"R16_UINT, id + 1\" → \"R32_UINT, id + 1\").

Plus the scene_buffer.rs caps: bump \`MAX_INSTANCES\` from \`0x7FFF\` to e.g. \`0x3FFFFFFF\` (1G — effectively unbounded for any plausible scene). Update the doc comment + relax the \`max_instances_stays_within_r16_uint_mesh_id_ceiling\` test to a 31-bit ceiling instead.

## Memory cost

GBuffer mesh_id attachment doubles. At common resolutions:
- 1080p: 1920 × 1080 × 4 B = 8.3 MB (was 4.15 MB) → +4.15 MB per attachment × 2 in-flight = +8.3 MB
- 1440p: 2560 × 1440 × 4 B = 14.7 MB (was 7.4 MB) → +7.4 MB × 2 = +14.7 MB
- 4K: 3840 × 2160 × 4 B = 33.2 MB (was 16.6 MB) → +16.6 MB × 2 = +33.2 MB

Trivial on the 6 GB RT-minimum target.

## Completeness Checks

- [ ] **SHADER LOCKSTEP**: All 4 shaders must use the same masks. After the GLSL edit, recompile SPIR-V (\`glslangValidator -V file.frag -o file.frag.spv\` per CLAUDE.md). The mask drift is the regression trap — if \`triangle.frag\` writes with the new encoding but \`svgf_temporal.comp\` reads with the old, disocclusion silently fails on every instance past 32767.
- [ ] **TESTS**: Update \`max_instances_stays_within_r16_uint_mesh_id_ceiling\` to pin the new ceiling (e.g. \`0x3FFFFFFF\`). The debug_assert in \`draw.rs\` should drop to match.
- [ ] **GBUFFER FORMAT TEST**: Add a regression test pinning \`MESH_ID_FORMAT == R32_UINT\` so a future "save VRAM by going back to R16" attempt fails loudly.
- [ ] **BENCH**: Re-bench a known interior + exterior (Prospector Saloon, Whiterun exterior) to confirm no measurable per-frame cost from the wider G-buffer. The format change is fill-rate-bound; on the 12 GB RTX 4070 Ti baseline the cost should be < 0.1 ms.
- [ ] **REAL-DATA VALIDATION**: Load Solitude / Whiterun draw / Diamond City and confirm \`Instance SSBO overflow\` no longer warns. The exact target is the cell whose REFR count exceeded 32767 (file the trigger cell in the closure comment so future audits can re-validate).
- [ ] **DOC**: Bump \`MAX_INSTANCES\` doc comment in \`scene_buffer.rs\` to reflect the new ceiling and remove the "Past-ceiling follow-up" paragraph (the follow-up is done at that point).

## Related

- \`67da5af\` — the bump to 0x7FFF that surfaced this as the next-step ceiling.
- \`fbba53e\` — the prior bump to 16384 + Option B sort prefix.
- \`4031614\` — Option B (raster-first sort + instance_map cap) — when this issue lands, the instance_map cap parameter doesn't need rework (it just gets a larger \`MAX_INSTANCES\`).
- \`#957\` REN-D8-NEW-13 — instance_custom_index 24-bit overflow (TLAS side, separate cap). The 24-bit TLAS field is independent of the G-buffer mesh_id encoding but lives in the same address-space conceptually — once the mesh_id is 31 bits, the TLAS becomes the next narrowest field for an unrelated scaling pass.

