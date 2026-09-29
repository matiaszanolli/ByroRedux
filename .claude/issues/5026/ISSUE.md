# REN-D5-2026-09-29-01: memory-budget.md has no egui entry, and #4986 added a host-RAM mirror of every egui image

**Labels**: low,documentation,doc-rot,renderer,memory

**Source**: `docs/audits/AUDIT_RENDERER_2026-09-29.md`
**Severity**: LOW
**Dimension**: Memory/Lifecycle
**Location**: `EguiPass::image_mirrors` (`crates/renderer/src/vulkan/egui_pass.rs`); `docs/engine/memory-budget.md`.

## Description
`promote_partial_deltas` (#4986) keeps a CPU `Color32` copy of each egui-managed image, mainly the font atlas, which grows with glyph coverage. The copy lives as long as the texture. Every partial atlas delta becomes a full-image re-upload. `memory-budget.md` has no "egui" entry at all — the GPU atlas has no row either. The audit rule is that every resource owner added since the baseline gets a ledger row.

## Impact
Ledger completeness; typically a few MiB of host RAM, equal to the atlas.

## Related
#4986.

## Suggested Fix
Add one row: atlas GPU image, an equal-sized CPU mirror, and a full re-upload on atlas growth, citing #4986.

Validated at HEAD 9fcfdc3fc: `image_mirrors: FxHashMap<TextureId, Arc<egui::ColorImage>>` exists in `egui_pass.rs`; `grep egui docs/engine/memory-budget.md` returns nothing.

## Completeness Checks
- [ ] **SIBLING**: other resource owners added since the last ledger refresh
- [ ] **TESTS**: if ledger rows are pinned against constants, add the egui row to that pin
