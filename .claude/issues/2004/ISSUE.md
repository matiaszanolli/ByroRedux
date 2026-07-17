# 2004: NIF-D1-05: NiTexturingProperty decal-slot count has no upper bound against nif.xml's fixed 4 slots

https://github.com/matiaszanolli/ByroRedux/issues/2004

Labels: medium, nif-parser, nif, bug

**Severity**: MEDIUM · **Dimension**: Stream Position Integrity
**Location**: `crates/nif/src/blocks/properties.rs:300-313`
**Status**: NEW
**Audit**: docs/audits/AUDIT_NIF_2026-07-16.md (NIF-D1-05)

## Description
nif.xml defines exactly 4 decal texture slots; the parser computes an open-ended `num_decals = texture_count.saturating_sub(8 or 6)` with no cap. `allocate_vec` bounds the allocation against remaining file bytes (OOM guard only), not against the format's known maximum of 4.

## Impact
A corrupt/malformed `texture_count` above 12 (or 10) causes the loop to read `TexDesc`s the format doesn't define, consuming bytes that belong to later fields — recoverable via `block_size` on FO3+, unrecoverable on Oblivion.

## Related
#429, #450 (adjacent version-gate fixes in the same function)

## Suggested Fix
Clamp `num_decals` to a hard maximum of 4 and treat any file claiming more as a parse error.

## Completeness Checks
- [ ] SIBLING: Same pattern checked in related files (other fixed-cardinality slot counts derived from an on-disk count field)
- [ ] TESTS: A regression test pins this specific fix (anomalous `texture_count` fixture that would otherwise overflow 4 decal slots)
