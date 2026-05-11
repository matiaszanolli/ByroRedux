# NIF-D5-NEW-03: BSDistantObjectInstancedNode (FO76) + BSDistantObjectLargeRefExtraData (SSE) not dispatched

**Severity**: MEDIUM
**Source audit**: `docs/audits/AUDIT_NIF_2026-05-10.md` (Dim 5)

## Game Affected

- `BSDistantObjectInstancedNode` — FO76 (`#F76#` per nif.xml)
- `BSDistantObjectLargeRefExtraData` — Skyrim SE (`#SSE#`)

## Location

`crates/nif/src/blocks/mod.rs` — both absent. `BSMultiBoundNode` parent IS dispatched (line 176), so the inheritance chain is broken at the subclass.

## Why it's a bug

- `BSDistantObjectInstancedNode` is the canonical container for FO76 foliage/rock cluster instancing in worldspace LOD. Inherits `BSMultiBoundNode`, adds `Num Instances` + array of `BSDistantObjectInstance`.
- `BSDistantObjectLargeRefExtraData` rides on every SSE large-ref worldspace object that participates in the precombined-LOD scheduling. Inherits `NiExtraData`, adds a single `bool Large Ref` flag.

## Impact

Silent `NiUnknown` skip (block_size present). 

- **FO76**: distant LOD instances drop, only the multi-bound shell renders → "ghost foliage" in distant terrain.
- **SSE**: large-ref flag lost, large refs miss precombined-LOD scheduling — renderer treats them as plain refs and re-uploads geometry per-cell instead of from the precombined pool.

## Fix

Two small parsers in `node.rs` / `extra_data.rs`:
- SSE LargeRef: one bool. Pair with a `LargeRefMarker` ECS component (or existing `LargeRef` marker if one exists).
- FO76 InstancedNode: parse `BSDistantObjectInstance` array per nif.xml compound def (4×u32 + transform). Emit per-instance transforms into the import scene.

## Completeness Checks

- [ ] **SIBLING**: Verify nif.xml `BSDistantObjectInstance` compound matches Rust parser
- [ ] **TESTS**: Fixture parse for both block types; FO76 corpus assertion
