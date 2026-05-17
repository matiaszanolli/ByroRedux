# PERF-D7-NEW-02: fixed-stride bone palette wastes ~6.5 MB/s PCIe + 272 KB GPU on Prospector (M29.5 budget reference)

**Labels**: bug, renderer, medium, memory, performance
**State**: OPEN

## Source Audit
`docs/audits/AUDIT_PERFORMANCE_2026-05-16.md` — Dimension 7 (TAA & GPU Skinning) / GPU Memory

## Severity
**MEDIUM** — below FPS-signal threshold on current bench but listed as the budget reference for **M29.5 GPU palette dispatch** (CLAUDE.md roadmap milestone). Matters more once M41 scales NPC counts.

## Location
- `byroredux/src/render.rs:429-436`
- `crates/core/src/ecs/components/skinned_mesh.rs:29` (`MAX_BONES_PER_MESH = 128`)

## Status
**NEW** at HEAD `1608e6a2`

## Description
Every skinned mesh's bone palette is zero-padded to `MAX_BONES_PER_MESH = 128` slots so per-mesh `bone_offset` arithmetic in the shader is trivially `offset + local_index`. The padding loop:

```rust
for _ in palette_scratch.len()..MAX_BONES_PER_MESH {
    bone_palette.push([[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0],
                       [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]]);
}
```

A typical FNV/Skyrim humanoid skeleton.nif has ~75-90 active bones; the remaining 38-53 slots are identity-padded every frame.

At 34 Prospector NPCs × ~50 slots × 64 B = **~109 KB/frame** wasted upload + GPU storage. × 60 fps = **~6.5 MB/s** sustained PCIe traffic.

## Impact
- ~6.5 MB/s sustained PCIe waste on Prospector
- ~272 KB GPU storage waste
- Below FPS-signal threshold today; matters more once M41 scales NPC counts

## Suggested Fix (M29.5 follow-up)
Implement variable-stride packing — store `bone_offset` AND `bone_count` per skinned mesh, shader reads:
```glsl
bones[bone_offset + min(bone_idx, bone_count - 1)]
```

## Completeness Checks
- [ ] **UNSAFE**: Vertex shader reads — bone_idx bounds check needed
- [ ] **SIBLING**: Verify M29.5 compute palette dispatch uses the same variable-stride model
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: Shader-struct-sync test must capture new SkinnedMesh layout

## Related
- M29.5 GPU palette dispatch
- M41 NPC count scaling
- `feedback_shader_struct_sync.md`
