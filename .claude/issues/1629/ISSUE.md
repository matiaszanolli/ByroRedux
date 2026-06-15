# TD6-001: RACE DATA parser applies TES4 36-byte layout to Skyrim's 128/164-byte DATA

_Filed as #1629 from `docs/audits/AUDIT_TECH_DEBT_2026-06-14.md`. Immutable snapshot as-filed; GitHub is authoritative for live state._

**Severity**: LOW · **Dimension**: Stub/Placeholder · **Effort**: medium
**Source audit**: `docs/audits/AUDIT_TECH_DEBT_2026-06-14.md` (TD6-001)
**Status**: NEW — genuine bounded gap (documented but **length-gated, not game-gated**, so it mis-parses rather than skipping)

## Description
The `b"DATA" if sub.data.len() >= 36` arm in `crates/plugin/src/esm/records/actor.rs:789` (RACE `DATA`) has no `GameKind` check. A Skyrim RACE DATA (128 / 164 B) satisfies `len >= 36` and is decoded with the TES4-era 36-byte layout → garbage skill bonuses / height / weight / flags. The comment honestly says "TES5 DATA is 128 / 164 bytes with a different layout — not yet wired here," but the guard is length-based, so it produces *plausible-but-wrong* values instead of leaving defaults. Reachable via `--master Skyrim.esm` and the `m41-equip.sh` smoke test.

## Evidence
`actor.rs:786-789`:
```
// TES5 DATA is 128 / 164 bytes with a different layout —
// not yet wired here.
b"DATA" if sub.data.len() >= 36 => {
```
No `is_skyrim` / `GameKind` branch precedes it.

## Impact
Bounded today — `RaceRecord` `skill_bonuses` / `base_height` / `base_weight` are not yet consumed by rendering/equip, so no visible symptom. Becomes a live foot-gun the moment a consumer reads those fields: it returns wrong data, not "unknown."

## Suggested Fix
Gate the arm to `Oblivion | Fallout3NV` (e.g. `b"DATA" if !is_skyrim && sub.data.len() >= 36`); for Skyrim either parse the TES5 layout or leave the fields at defaults so a future consumer sees "unknown," not "garbage."

## Completeness Checks
- [ ] **SIBLING**: Other length-gated record arms with cross-game layout drift (cf. #1550 `parse_ctda`, #1579 SF XCLL) carry a `GameKind` guard where the layout differs by game
- [ ] **TESTS**: A regression test feeds a 128/164-byte Skyrim RACE DATA and asserts no garbage TES4-layout decode
