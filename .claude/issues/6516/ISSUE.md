# SKY-D4-01: The FACT CRVA short-form gate is 14 bytes, but the format's optional cut is at 12 and 16; two vanilla CRVAs are dropped and every 16-byte CRVA's steal multiplier is discarded

Labels: low, esm-plugin, bug, game:skyrim, legacy-compat
Source: docs/audits/AUDIT_SKYRIM_2026-10-27.md

**Source**: `docs/audits/AUDIT_SKYRIM_2026-10-27.md` finding `SKY-D4-01` (Dimension 4, TES5 record coverage) · Severity LOW · Status NEW (4cfcee4b9, 2026-10-24) · Game: Skyrim. Contradicts AUDIT_ESM_2026-10-27's "verified clean" note that `CRVA` is a 14-byte minimum.

**Location**: `crates/plugin/src/esm/records/actor/faction.rs:5-11` (`CRVA_MIN_LEN = 14`, `CRVA_TAIL_LEN = 20`), `:334-355` (the `b"CRVA"` arm); test `actor/tests.rs` (`crva[..14]`, ~:2693).

## Description
The FACT `CRVA` short-form gate is 14 bytes, but the format's optional cut is at 12 and 16. Layout:

- `u8 arrest`, `u8 attack-on-sight`, then `u16` murder, assault, trespass, pickpocket and unknown. That is **12** bytes.
- `f32 steal mult` brings it to **16**.
- `u16 escape` and `u16 werewolf` bring it to **20**.

UESP *Skyrim Mod:Mod File Format/FACT* gives `struct[12/16/20]`. `SetOptionalFrom(7)` makes element 7 (steal mult) the first optional element, which is 12 bytes in, not 14: elements 0–1 are u8. The parser's own doc ("end after the `unknown` word (14 bytes)") is off by two. Records at 12 bytes fail the `>= 14` gate, so `values` is `None`. Records at 16 bytes take `tail = len >= 20` false, so `steal_multiplier = 0.0` whatever is authored.

## Evidence
Current code:
```rust
const CRVA_MIN_LEN: usize = 14;
const CRVA_TAIL_LEN: usize = 20;
...
b"CRVA" if sub.data.len() >= CRVA_MIN_LEN => { ...
    let tail = sub.data.len() >= CRVA_TAIL_LEN;
    steal_multiplier: if tail { r.f32_or_default() } else { 0.0 },
```
CRVA size census from the audit:

| Plugin | 20 B | 16 B | 12 B |
|---|---|---|---|
| `Skyrim.esm` | 969 | 112 | 2 |
| `Update.esm`, `Dawnguard.esm`, `HearthFires.esm`, `Dragonborn.esm` | all | 0 | 0 |

The 12-byte records are `MS06CultistFaction` and `MS06PotemaFaction`.

## Impact
No vanilla runtime effect: all 112 16-byte multipliers are authored 0.0, and none of the 114 short-form factions sets TRACK_CRIME (0x40). The only consumer (`crime_faction.rs:114`) reads fines, not arrest or attack-on-sight. A mod that authors an older-version 16-byte CRVA loses its steal multiplier.

## Related
AUDIT_TECH_DEBT_2026-10-25 (named the constants only), #6393 (closed, length-gate naming), #6427.

## Suggested Fix
Gate at 12 bytes. Read `steal_multiplier` at >= 16 and escape/werewolf at >= 20. Pin the test at 12 and 16.

## Completeness Checks
- [ ] **SIBLING**: FO4 FACT `CRVA` handling (shared parser path) checked for the same cut points
- [ ] **TESTS**: A regression test pins 12-byte and 16-byte CRVA decoding, plus the existing 20-byte case

