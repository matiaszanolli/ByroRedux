# REN-D9-2026-09-20-02: the PickedUp render-skip is a two-site lockstep decision with no guard — only its NpcAppearanceHidden sibling is tested

- **ID**: REN-D9-2026-09-20-02
- **Labels**: low,renderer,test-gap,bug
- **Filed from**: docs/audits/AUDIT_RENDERER_2026-09-20.md

**Severity**: LOW · **Dimension**: Skinning
**Source**: docs/audits/AUDIT_RENDERER_2026-09-20.md (REN-D9-2026-09-20-02)

**Location**: `byroredux/src/render/skinned.rs` (palette skip) vs `byroredux/src/render/static_meshes.rs` (draw skip) — landed by 479163836

**Description**
The P3 PickedUp optimization must skip both the palette dispatch and the draw; divergence renders picked-up items in bind pose. The sibling NpcAppearanceHidden skip has a lockstep test; PickedUp has none.

**Evidence**
Audit D9, 2026-09-20.

**Impact**
A future edit to one site silently breaks the other.

**Suggested Fix**
Mirror the NpcAppearanceHidden lockstep guard for PickedUp.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
