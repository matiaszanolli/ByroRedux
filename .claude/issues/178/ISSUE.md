# Issue #178 — ECS: SkinnedMesh component + bone-palette upload path

**Labels:** ecs, renderer, high
**State:** OPEN

Follow-up to #151 (importer now populates ImportedSkin) and #177 (BSTriShape
vertex weights extracted). This issue adds the ECS component, scene assembly
wiring, GPU bone palette, vertex format extension, and shader skinning path.

**Decisions (confirmed by user):**
1. Unify vertex format (single path, non-skinned uses identity palette entry)
2. One big SSBO indexed by base offset per skinned mesh
3. MAX_BONES_PER_MESH = 128
4. Two-commit split: (A) ECS + scene assembly + CPU palette, (B) renderer + shader
5. Bone weights stored as f32 on GPU
