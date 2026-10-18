# SKY-D5-2026-10-17-01: Every Skyrim bhkRagdollConstraint/bhkLimitedHingeConstraint parses 1 byte short (motor byte) and WARNs per NIF load

GitHub: https://github.com/matiaszanolli/ByroRedux/issues/5960 (snapshot as filed 2026-10-17; GitHub is authoritative for current state)

**Source**: `docs/audits/AUDIT_SKYRIM_2026-10-17.md` (report ID SKY-5-01) · **Severity**: LOW · **Dimension**: 5 (corpus gates) / NIF constraint parsing · **Game**: Skyrim SE + LE

## Description
The FO3+/Skyrim constraint CInfo parsers deliberately stop before the trailing `bhkConstraintMotorCInfo` and rely on the block-size table to realign (`crates/nif/src/blocks/collision/constraints.rs:156-162` and `:775-784`). On Skyrim the motor type byte is the single missing byte on every instance, so the parse-end summary WARN (`crates/nif/src/lib.rs:812-820`) fires for every actor NIF that carries a ragdoll.

## Evidence
`nif_stats --drift-histogram` on SE `Skyrim - Meshes0.bsa`: 1,462 events (769 `bhkLimitedHingeConstraint` + 693 `bhkRagdollConstraint`), all `drift=+1`. `Skyrim - Meshes1.bsa`: 113 events (54 + 59), all `drift=+1`. The totals 823 + 752 equal the `bhkLimitedHingeConstraint` / `bhkRagdollConstraint` counts in `crates/nif/tests/data/per_block_baselines/skyrim_se.tsv`, so 100% of Skyrim constraint blocks drift by exactly one byte. Live Forelhost01 run (`--master Skyrim.esm --master Update.esm --esm Dawnguard.esm`): the same WARN fired 5 times (17 blocks per Draugr / Character skeleton: `bhkRagdollConstraint=9, bhkLimitedHingeConstraint=8`).

## Impact
No data is lost: block-size realignment is exact and the decoded prefix is what PHYSAL consumes. But the corpus rate gate does not count drift, so a "100% clean" Skyrim corpus coexists with 1,575 expected drift events, and a genuine new drift in these two types would be invisible among them. The `/audit-skyrim` Dim 5 expectation of "0 realignment WARNs" does not hold for Skyrim.

## Suggested fix
Consume the motor type byte (and its payload when not `NONE`) so the histogram reads zero and a real regression becomes visible; or demote the aggregated WARN to debug for these two types and gate the drift histogram in the Skyrim baselines.

Related: closed #1605 (constraint decoders hidden from the drift detector), #565 (drift aggregation), #939 (`--drift-histogram`).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in the other FO3+/FNV constraint types and the `BhkBreakableConstraint` prefix tables
- [ ] **TESTS**: A regression test pins the Skyrim drift histogram for these two types at zero (or at the documented count)
