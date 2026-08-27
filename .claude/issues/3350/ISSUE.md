# FNV-2026-08-26-D9-04

**Issue**: #3350
**Filed**: 2026-08-26 from `docs/audits/AUDIT_FNV_2026-08-26.md`

---

**Severity**: LOW
**Dimension**: 9 — AI Packages & Procedures
**Status**: NEW
**Source**: `docs/audits/AUDIT_FNV_2026-08-26.md` (audit HEAD `d6e16c90`)


**File**: `crates/plugin/src/esm/records/condition.rs:264-270`

**Premise verified**: `parse_ctda` returns `None` for any payload shorter
than 24 bytes, with only a `log::debug!`. Its comment asserts the
minimum is *"24 bytes (Oblivion)"*. FalloutNV.esm disagrees.

**Evidence** — full-file CTDA size histogram over every record type:
```
global CTDA size histogram: [(28, 67880), (20, 123), (24, 2)]
  IDLE  [(28, 1698), (20, 98), (24, 2)]
  PACK  [(28, 2968), (20, 24)]
  QUST  [(28, 1500), (20, 1)]
```
The 24 PACK-side 20-byte CTDAs belong to two real Patrol packages,
`0x26d86 mvsRaiderTowerPatrolA` and `0x26d88 mvsRaiderTowerPatrolB`.
Raw record dump of `0x26d88` (sub-record size field is `0x0014` = 20,
verbatim in the file, not a walker artefact):
```
b'PKDT' 8  000080000d000000       # procedure byte @4 = 0x0d = 13 = Patrol
b'PSDT' 8  ffff00ff00000000       # time = 0xFF = -1 (any), duration 0
b'CTDA' 20 60000000 00000040 1200 0000 00000000 00000000
b'CTDA' 20 81000000 00008040 1200 0000 00000000 00000000   … ×12
b'PKPT' 1  01
```
The layout is the canonical prefix minus the FO3+ `run_on`/`reference`
tail: type `0x60`/`0x81` (the `0x01` OR bit set on alternating rows),
f32 comparand stepping 2.0 → 4.0 → 6.0 → 8.0 …, function index `u16` @8
= `0x0012` = 18, params zero. A twelve-leaf OR-chain of hour-of-day
windows for a tower patrol.

**Impact**: today, invisible. Function 18 is outside the 19-entry M47.1
catalog, so even if the conditions were decoded the fail-open rule would
pass the package anyway; and with the CTDAs dropped the list is empty,
which also passes. The bug is latent: the moment function 18 enters
`ConditionFunction::from_index`, these two packages will still be
unconditionally active because their conditions never reached
`PackRecord.conditions` at all — a wrong result that no test would catch,
arriving via a change in an unrelated file. The same reject drops 98
IDLE-record and 1 QUST-record conditions on the reference title.

**Fix sketch**: lower the guard to `data.len() < 20` and gate the
`param_1`/`param_2` reads at offsets 12..20 on the available length
(they are already inside the 20-byte form); extend the
`matches!(data.len(), 24 | 28 | 32)` "unexpected length" debug set to
include 20. Note this is a `crates/plugin` change that also touches
Dimension 2's territory — flagged here because PACK condition gating is
this dimension's checklist item 1.

---

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
