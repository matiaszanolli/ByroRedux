# 2007: NIF-D3-02: nif-parser.md mischaracterizes NiFogProperty as a dispatch-table gap; it is dispatched

https://github.com/matiaszanolli/ByroRedux/issues/2007

Labels: low, nif-parser, documentation

**Severity**: LOW · **Dimension**: Block Dispatch Coverage
**Location**: `docs/engine/nif-parser.md:359-360`; `crates/nif/src/blocks/mod.rs:590`
**Status**: NEW
**Audit**: docs/audits/AUDIT_NIF_2026-07-16.md (NIF-D3-02)

## Description
The doc's "Block coverage" section states `NiFogProperty` is "a deliberate non-dispatch (#1224)" — but `"NiFogProperty" => Ok(Box::new(NiFogProperty::parse(stream)?))` is a live, working dispatch arm. The actual #1224 gap is one layer downstream: the import material walker never calls `scene.get_as::<NiFogProperty>()`, so parsed fog data never reaches `MaterialInfo`.

## Evidence
`git show` of the #1224 closeout commit touches only `properties.rs` and `import/material/walker.rs`, not `blocks/mod.rs`. Checked-in baselines show `NiFogProperty` parses cleanly with zero `NiUnknown` on Oblivion/FO3.

## Impact
Low, but a future auditor trusting this doc section could file a duplicate issue against the wrong layer.

## Related
#1224 (closed, correctly scoped to the material walker — no regression).

## Suggested Fix
Reword to note `NiFogProperty` parses cleanly but is a deliberate non-consumption at the import/material-walker layer (#1224) — cross-link NIFAL/Dimension 4 rather than implying a dispatch gap.

## Completeness Checks
- [ ] TESTS: N/A (documentation-only fix; no code path to regress)
