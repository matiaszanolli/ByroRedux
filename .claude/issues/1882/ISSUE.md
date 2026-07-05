**Severity**: MEDIUM · **Dimension**: Stream Position · **Source**: `docs/audits/AUDIT_NIF_2026-07-05.md` (NIF-D1-002)
**Game Affected**: Starfield only (Starfield-introduced block; bsver ≥ 172).
**Status**: NEW
**Location**: `crates/nif/src/blocks/node.rs` (`BsWeakReferenceNode::parse`, dispatched in `crates/nif/src/blocks/mod.rs`)

## Description
`BsWeakReferenceNode::parse` reads the `NiNode` base then the weak-ref / water-ref lists and returns, stopping a constant **2 bytes short of `block_size`** on 6894 instances (8 short on 13), which `block_size` reconciliation absorbs. The parser's own doc comment claims it "reads and discards the trailing data so block alignment is maintained" — the residual drift means that contract is not met. The nifly reference matches the top-level list layout (numWeakRefs → weakRefs[] → unkInt1 → numWaterRefs → waterRefs[]) and the per-ref `formID` gate (`Stream()>=173` == `bsver::SF_FORM_ID`), so the missing 2 bytes are neither the list layout nor the formID gate. The constant +2 (independent of ref counts, present even with empty lists) points to either a 2-byte trailing/pad field or a NiNode-level Starfield field — `version.rs` documents that `bsver >= SF_FORM_ID` content "carries a form_id field in some blocks (e.g. NiNode)", yet `NiNode::parse` reads no such field.

## Evidence
- `node.rs` `BsWeakReferenceNode::parse` returns immediately after the water-ref loop; no trailing read, no `block_size` awareness.
- Doc comment claims "reads and discards the trailing data so block alignment is maintained" — contradicted by the +2 residual.
- `version.rs` `SF_FORM_ID` comment ("this field in some blocks (e.g. NiNode)") vs `NiNode::parse` having no bsver≥173 branch.
- Drift histogram (MeshesPatch.ba2): `BSWeakReferenceNode drift=+2 ×6894`, `drift=+8 ×13`.

## Impact
2 bytes/node dropped on effectively every Starfield packin/composite-LOD reference node (6907 in one patch archive alone). Discard-only today (LOD-streaming payload unconsumed pending M35+), so no visible artifact, but an unmodelled field that will bite once the weak-ref/water-ref data is consumed. If the culprit is a NiNode-level `bsver≥173` field, the correct fix in `NiNode::parse` would protect every Starfield NiNode subclass.

## Suggested Fix
Byte-dump one Starfield `BSWeakReferenceNode` (compare `block_size` vs consumed against on-disk bytes) to localize the 2-byte field. If NiNode-level, add the `bsver >= SF_FORM_ID` branch in `NiNode::parse` (protects all subclasses); if block-local, read it in `BsWeakReferenceNode::parse`. Add a `dispatch_tests` case asserting exact consumption.

## Related
#754 (BSWeakReferenceNode introduction), `bsver::SF_FORM_ID`.

## Completeness Checks
- [ ] **SIBLING**: If the field is NiNode-level, verify every Starfield NiNode subclass (`BSGeometry` parents, `BSOrderedNode`, …) now consumes it
- [ ] **TESTS**: A `dispatch_tests` case asserts `consumed == block_size` for a real Starfield `BSWeakReferenceNode`
