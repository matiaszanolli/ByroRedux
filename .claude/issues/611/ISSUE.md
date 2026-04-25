# SK-D5-02: parse_nif root_index picks wrong block when scene root is a NiNode subclass — tree LODs import 0 meshes

## Finding: SK-D5-02

- **Severity**: HIGH
- **Source**: `docs/audits/AUDIT_SKYRIM_2026-04-24.md`
- **Game Affected**: Skyrim SE (tree LODs, SpeedTree content), all games using BSTreeNode / BsValueNode / BsMultiBoundNode / BsOrderedNode / BsRangeNode / NiBillboardNode / NiSwitchNode / NiLODNode / NiSortAdjustNode as the scene root
- **Location**: [crates/nif/src/lib.rs:454-457](crates/nif/src/lib.rs#L454-L457)

## Description

The root-block selector in `parse_nif_with_options` matches the literal block-type-name `"NiNode"`:

```rust
// crates/nif/src/lib.rs:453-460
let root_index = if !blocks.is_empty() {
    blocks
        .iter()
        .position(|b| matches!(b.block_type_name(), "NiNode"))
        .or(Some(0))
} else {
    None
};
```

Every Bethesda NiNode subclass that gets its own Rust struct (and returns its own type-name from `block_type_name()`) is invisible to this predicate. The iterator skips the real root and either picks the first plain-NiNode child (typically a leaf bone container) or — if no plain NiNode exists — falls back to index 0.

The `as_ni_node()` helper at [import/walk.rs:36-68](crates/nif/src/import/walk.rs#L36) already enumerates the correct subclass list (BsOrderedNode, BsValueNode, BsMultiBoundNode, BsTreeNode, NiBillboardNode, NiSortAdjustNode, BsRangeNode). The root selector does not call it.

## Evidence (empirical)

Audit Dim 5 reproduced against vanilla `Skyrim - Meshes0.bsa`:

| File | Root block | Result |
|---|---|---|
| `landscape\trees\treepineforest01.nif` | BSTreeNode (block 0) | Picks plain NiNode at block 4 → walk descends from a leaf bone container → 0 of 4 geometry shapes imported |
| `landscape\trees\treepineforest01_lod_flat.nif` (control) | NiNode | 1 mesh OK |

`BSFadeNode` and `BSLeafAnimNode` survive only because `blocks/mod.rs:134-140` aliases them at parse time to plain `NiNode` — the dispatch table papers over the lib.rs gap rather than fixing it.

## Impact

- **HIGH**: every SpeedTree-rooted tree LOD imports 0 meshes — vanilla Skyrim landscape goes treeless.
- Same hazard for any modded NIF rooted at NiSwitchNode/NiLODNode (furniture states, weapon-state switches, LOD chains) when the root is the switch/LOD itself rather than wrapped in a plain NiNode.
- Distinct from #159 (BSTreeNode dispatch — closed) and #363 (BSTreeNode bone lists — closed). Those fixed the parser; this finding is the post-parse root-pick step.

## Suggested Fix

Replace the literal `matches!(..., "NiNode")` predicate with a call to `as_ni_node()`:

```rust
// crates/nif/src/lib.rs
use crate::import::walk::as_ni_node;  // (or hoist to a shared helper)

let root_index = blocks
    .iter()
    .position(|b| as_ni_node(b.as_ref()).is_some())
    .or_else(|| if blocks.is_empty() { None } else { Some(0) });
```

Or, in-place, extend the match arm:

```rust
matches!(b.block_type_name(),
    "NiNode" | "BSTreeNode" | "BSMultiBoundNode" | "BSValueNode"
    | "BSOrderedNode" | "BSRangeNode" | "NiBillboardNode"
    | "NiSwitchNode" | "NiLODNode" | "NiSortAdjustNode")
```

The helper-based path is more robust against future subclass additions.

## Related

- #159 (closed): BSTreeNode parser dispatch — landed but didn't fix the lib.rs root selector.
- #148 (closed): BsMultiBoundNode dispatch — same shape.
- #559 (open): NiSkinPartition global vertex buffer — separate but blocks Skyrim NPC content; together these two are the dominant Skyrim-content blockers.

## Completeness Checks

- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: Audit `import/mod.rs` for any other site that does the same literal "NiNode" check; the helper should cover them all.
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: Add a regression test using a synthetic NIF rooted at BSTreeNode → assert `parse_nif(...).root_index == Some(0)`. Add a Skyrim integration test against `treepineforest01.nif` → assert `import_nif_scene(...).meshes.len() > 0`.

_Filed from audit `docs/audits/AUDIT_SKYRIM_2026-04-24.md`._
