# D1-NEW-01: NiNode.culling_mode (BSVER ≥ 83) only honored on BsMultiBoundNode subclass

## Finding: D1-NEW-01

- **Severity**: LOW
- **Dimension**: Scene Graph Decomposition
- **Source**: `docs/audits/AUDIT_LEGACY_COMPAT_2026-04-24.md`
- **Game Affected**: Skyrim, FO4 (NiNode subclasses other than BsMultiBoundNode)
- **Location**: [crates/nif/src/blocks/node.rs:230-262](crates/nif/src/blocks/node.rs#L230-L262), [crates/nif/src/import/walk.rs:182-186](crates/nif/src/import/walk.rs#L182-L186)

## Description

Skyrim+ added a `culling_mode: u32` field to `NiNode` itself (BSVER ≥ 83), not just `BsMultiBoundNode`. Mode 2 (always-hidden) and mode 3 (force-culled) on a generic `NiNode` reach the parser correctly but only the `BsMultiBoundNode` downcast path at walk.rs:182-186 honors them. A plain `NiNode` with `culling_mode == 2` is recursed and rendered.

Companion to closed #355 (which only addressed BsMultiBoundNode).

## Evidence

```rust
// crates/nif/src/blocks/node.rs:254-262 — parsed for every NiNode on Skyrim+
let culling_mode = if stream.bsver() >= 83 {
    stream.read_u32()?
} else {
    0
};

// crates/nif/src/import/walk.rs:182-186 — only checked on the subclass
if let Some(mbn) = block.as_any().downcast_ref::<BsMultiBoundNode>() {
    if mbn.culling_mode == 2 || mbn.culling_mode == 3 {
        return;
    }
}
```

## Impact

Author-flagged invisible subtrees on plain NiNode parents render as visible. Most production NIFs use BsMultiBoundNode for occluders, so the in-the-wild count is small, but mod content using bare NiNode loses the hint.

## Suggested Fix

Generalize the check at walk.rs:182 — call `as_ni_node(block)` and check `node.culling_mode` regardless of subclass. Drop the BsMultiBoundNode-specific branch (or keep it as a redundant short-circuit; the generic check covers it).

## Related

- #355 (closed): BsMultiBoundNode + multi_bound_ref + culling_mode (subset).
- #148 (closed): BsMultiBoundNode dispatch.

## Completeness Checks

- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: N/A
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: Synthetic Skyrim NIF with plain NiNode + culling_mode=2 → verify subtree omitted from ImportedScene.

_Filed from audit `docs/audits/AUDIT_LEGACY_COMPAT_2026-04-24.md`._
