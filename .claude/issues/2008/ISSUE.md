# 2008: NIF-D4-01: BsOrderedNodeData.alpha_sort_bound is the only node-level bound field left in raw Gamebryo Z-up space

https://github.com/matiaszanolli/ByroRedux/issues/2008

Labels: low, nif-parser, import-pipeline, bug

**Severity**: LOW · **Dimension**: Geometry Extraction & Import Handoff
**Location**: `crates/nif/src/import/walk/mod.rs:1567-1575` (`extract_bs_ordered_node`), `crates/nif/src/import/types.rs:252-261`, `crates/nif/src/blocks/node.rs:132-166`
**Status**: NEW
**Audit**: docs/audits/AUDIT_NIF_2026-07-16.md (NIF-D4-01)

## Description
Every other position/bound field on `ImportedNode` and siblings is explicitly converted Z-up → Y-up and documented as such. `BsOrderedNodeData.alpha_sort_bound` (`[x, y, z, radius]`) is copied verbatim with no conversion and no handedness note in its doc comment.

## Evidence
`walk/mod.rs:1567-1575` copies `n.alpha_sort_bound` with no `zup_to_yup` call. Confirmed via grep that no consumer reads `ImportedNode.bs_ordered_node` anywhere outside the `nif` crate today — the field is currently inert.

## Impact
None today (unconsumed field). Latent risk: a future consumer wiring up alpha-sort occlusion would very plausibly compose it as Y-up like every sibling field, silently producing a wrong bound center (axis swap + sign flip; radius unaffected).

## Related
None — distinct from `#625` (added the field's presence, not its coordinate contract).

## Suggested Fix
Either convert via `zup_point_to_yup` at extraction (matching `bs_bound`'s treatment) and document as Y-up, or explicitly document it as intentionally NOT converted and why.

## Completeness Checks
- [ ] SIBLING: Same pattern checked in related files (other bound/position fields on `ImportedNode` and its siblings)
- [ ] TESTS: A regression test pins this specific fix (once a consumer exists / once the coordinate contract is documented)
