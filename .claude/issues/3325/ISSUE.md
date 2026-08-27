# FNV-2026-08-26-D4-02

**Issue**: #3325
**Filed**: 2026-08-26 from `docs/audits/AUDIT_FNV_2026-08-26.md`

---

**Severity**: MEDIUM
**Dimension**: 4 — ESM Record Parser
**Status**: NEW
**Source**: `docs/audits/AUDIT_FNV_2026-08-26.md` (audit HEAD `d6e16c90`)


**File**: `crates/plugin/src/esm/records/actor/mod.rs:1511-1551` (`parse_fact`
sub-record match — arms exist for `DATA`, `XNAM`, `MNAM` only);
`FactionRecord` struct at `:584-594` has no reputation field.

**Premise verified**: `grep -rn "WMI1" crates/ byroredux/ docs/` returns **zero
hits** across the entire repository. `RepuRecord`/`index.reputations` is
populated (`dispatch_misc_gameplay_b.rs:135`) and asserted (`>= 10`,
`parse_real_esm.rs:755`) but has no consumer beyond the count assertion.

**Evidence** — census over FalloutNV.esm, resolving every `WMI1` payload against
a whole-file FormID→record-type map:

```
FACT WMI1 count: 46   target record types: {'REPU': 46}
REFR WMI1 count: 36   target record types: {'REPU': 36}
  VRRCKarlFaction        0x17b5b6 -> 000F43DD  REPU (RepNVCaesarsLegion)
  PrivateKowalskiFaction 0x179162 -> 000F43DE  REPU (RepNVNCR)
  BoomerChildFaction     0x1630bf -> 000FFAE8  REPU (RepNVBoomer)
  vTopsPerformerFaction  0x16a265 -> 00118F61  REPU (RepNVTheStrip)
```

**82 of 82 WMI1 payloads resolve to a real REPU record — a 100% hit rate**, which
byte-proves the sub-record is the FACT/REFR→REPU link and not opaque data. The 13
REPU records are FNV's signature faction-reputation set (RepNVNCR,
RepNVCaesarsLegion, RepNVBoomer, RepNVGoodsprings, RepNVFreeside, RepNVNovac,
RepNVPrimm, RepNVFollowers, RepNVBrotherhood, RepNVGreatKhans, RepNVTheStrip,
RepNVWhiteGloveSociety, RepNVPowderGanger).

**Impact**: This is the most FNV-specific authoring in the whole file — reputation
replaces FO3's global karma and gates vendor prices, faction-armor disguise
reactions, quest branches, and hostile/idolized NPC greetings. Without the
FACT→REPU edge nothing can map an NPC's faction to the reputation meter it moves,
so the 13 parsed REPU records are unreachable: the reputation subsystem cannot be
built on top of the current index no matter what runtime lands.

**Fix sketch**: add `b"WMI1" if sub.data.len() >= 4 => record.reputation =
Some(remap_fid(...))` to `parse_fact` plus a `pub reputation: Option<u32>` on
`FactionRecord`; mirror it as `reputation_ref` on `PlacedRef` in
`cell/walkers.rs` for the 36 REFR-scoped overrides. Pin with a floor assertion in
`parse_rate_fnv_esm` (`>= 46` FACT bindings, all resolving into
`index.reputations`).

---

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
