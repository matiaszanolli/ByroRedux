# #5045 — LC-D3-01: DIAL DATA byte 0 is decoded as "dialogue type" on every game, but on Skyrim, FO4 and Starfield byte 0 is Topic Flags and the category is byte 1

**Labels**: medium, bug, legacy-compat, esm-plugin, dialogue, game:skyrim, game:fo4, game:starfield

**Source**: `docs/audits/AUDIT_LEGACY_COMPAT_2026-09-29.md` — finding `LC-D3-01`

**Severity**: MEDIUM

**Dimension**: 3 — Cross-game translation pattern (Pattern C: one wire structure, a per-era layout)

**Location**:
- `crates/plugin/src/esm/records/misc/dialogue.rs:199` (the decode).
- `:21-28` (the field doc).
- `:636-650` (the test `parse_dial_captures_dialogue_type_byte`).

**Status in report**: NEW. It is not in any report dated today. `AUDIT_GAMEPLAY_2026-09-29.md:258` names it only as a routing note to `/audit-esm`, and `AUDIT_ESM_2026-09-29.md` does not carry it. It is not tracked in open or closed issues; `#1307` and `#1313` added the byte-0 read for Oblivion only.

## Description

- `parse_dial` stores `DATA[0]` into `DialRecord::dial_type`.
- The field doc asserts that "FO3+ widen it (type byte + flags) but byte 0 is the type in every game, so the byte-0 read is cross-game safe".
- That is true for the classic layouts (1 or 2 bytes). It is false for the 4-byte Skyrim, FO4 and Starfield layout, which is `flags u8, category u8, subtype u16`.
- On those three games `dial_type` holds a flags bitfield, and the topic category (Topic / Favor / Scene / Combat / Favors / Detection / Service / Misc) is never captured.
- The unit test pins the false premise with a synthetic 4-byte "FO3" `DATA` (`[5, 0x01, 0, 0]`). Real FO3 `DATA` is 2 bytes.

## Evidence

Raw census of DIAL `DATA`, this run:

| Master | `DATA` length | byte 0 values | byte 1 values |
|---|---|---|---|
| Oblivion.esm | 1 (3,817) | 0..6 (type) | — |
| Fallout3.esm | 2 (6,369), 1 (12) | 0..7 (type) | 0/1/2 (flags) |
| FalloutNV.esm | 2 (18,205), 1 (10) | 0..7 (type) | 0/1/2 (flags) |
| Skyrim.esm | 4 (15,037) | {0: 15,018, 1: 19} (flags) | 0..7 (category; 2 = 7,426, 0 = 6,535) |
| Fallout4.esm | 4 (35,443) | {0, 1, 4, 6} (flags) | 0..7 (category; 2 = 31,330) |
| Starfield.esm | 4 (68,154) | {0, 1, 2, 4, 6} (flags) | 0..7 (category; 2 = 63,367) |

```rust
// dialogue.rs:197-199
// DATA byte 0 = dialogue type, cross-game safe (Oblivion: 1 byte;
// FO3+: wider, byte 0 still the type). #1307 / OBL-D3-...-03.
b"DATA" if !sub.data.is_empty() => out.dial_type = sub.data[0],
```

## Impact

- **Today:** nothing reads `dial_type` at runtime. The readers are `parse_real_esm.rs` printlns and `npc_dialogue` test fixtures that set it to 0.
- **Next consumer:** **GAME-D2-2026-09-29-01** (MEDIUM) needs a category filter (Topic vs Scene / Combat / Misc), because Eltrys "owns" 117 MS01 DIALs, including 5 Scene and 4 Misc DIALs. A fix that filters on `dial_type` would read flags on 3 of 6 games. On Skyrim it would treat all 15,018 zero-flag DIALs as "Topic", which silently re-admits exactly the scene and bark topics it means to exclude.
- **Shape:** the category is a translatable input dropped on three games, with a wrong value stored under a field documented as cross-game safe.

## Related

- GAME-D2-2026-09-29-01 (the consumer).
- ESM-2026-09-29-D2-03 (sibling DIAL doc rot).
- `#1307` and `#1313` (closed; they introduced the byte-0 read from Oblivion evidence only).
- `#4469` (a different record: INFO `DATA` on FO3/FNV).
- Depth is owned by `/audit-esm`.

Dialogue-landing cluster (`ab31cfefe` / `766e1746e`), filed as distinct issues: ECS-2026-09-29-D1-01 (#5025), ECS-2026-09-29-D1-02 (#5032), ECS-2026-09-29-D5-01 (#5035), ECS-2026-09-29-D7-01 (#5038), SCR-D3-2026-09-29-01 (#5041), ESM-2026-09-29-D2-03 (#5048). The per-frame Talk-candidate scan on the same code (ECS-2026-09-29-D6-01 = PERF-D1-2026-09-29-01) is tracked under open #3475.

## Suggested Fix

- Decode by layout generation. For `DATA` length ≤ 2, set type = byte 0 and flags = byte 1. For length 4, set flags = byte 0, category = byte 1 and subtype = `u16 @ 2`.
- Expose a single category value whose enum mapping is per game.
- Correct the field doc, and replace the synthetic 4-byte "FO3" test with one real-shaped case per era.

Validated at HEAD 9fcfdc3fc: `parse_dial` (`crates/plugin/src/esm/records/misc/dialogue.rs`) still does `b"DATA" if !sub.data.is_empty() => out.dial_type = sub.data[0]`; the field doc still says byte 0 is the type in every game; `parse_dial_captures_dialogue_type_byte` still pins a synthetic 4-byte "FO3" DATA `[5, 0x01, 0, 0]`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (INFO `DATA` per-era layout, see #4469)
- [ ] **CANONICAL-BOUNDARY**: the per-game category mapping is resolved at the parser boundary, not by consumers
- [ ] **TESTS**: A regression test pins this specific fix (one real-shaped DATA case per era)
