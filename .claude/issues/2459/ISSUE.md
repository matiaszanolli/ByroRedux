# SUBSYS-04: NiAVObject DISABLE_SORTING is captured into SceneFlags but never reaches the alpha draw sort

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2459
**Finding ID**: SUBSYS-04 (source: `docs/audits/AUDIT_LEGACY_COMPAT_2026-08-07.md`)

**Severity**: LOW
**Dimension**: 7 — Subsystem coverage vs legacy
**Location**: `crates/core/src/ecs/components/scene_flags.rs:41-44,81-84`; `byroredux/src/render/mod.rs:309-360`
**Status**: NEW

## Description
`SceneFlags::DISABLE_SORTING` (0x0040) and its accessor are attached at both import paths but have zero consumers — `draw_sort_key` has no sorting-disabled lane, so authored "keep children in file order" intent is overridden by the global back-to-front alpha sort.

## Evidence
Confirmed directly: `DISABLE_SORTING` constant and its accessor exist (`scene_flags.rs:41,82`) with a self-test at `:126`; grep for it (or `disable_sorting`) outside that file and its own test returns zero hits in `byroredux/src/render/` or `crates/renderer/src/`.

## Impact
Transparent geometry authored with explicit draw-order (layered glass, multi-card foliage/hair, nested effect planes) gets depth-sorted instead of file-ordered. Scored LOW because the flag's legacy runtime semantics could not be verified against the (unmounted) Gamebryo source, and no concrete misrendering has been attributed to it yet.

## Suggested Fix
Verify the flag's meaning against Gamebryo's `NiAlphaAccumulator`/`NiAVObject` headers when reachable, and measure real-corpus incidence via a `byro-dbg` scan; if ~0, close as a documented intentional skip (`NiFogProperty`-style) so future audits don't re-file it.

## Completeness Checks
- [ ] **TESTS**: If wired up, a regression test confirms file-order draw for a `DISABLE_SORTING`-flagged subtree
