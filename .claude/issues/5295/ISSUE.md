# #5295: ESM-2026-10-05-D2-01: The #4469 INFO `DATA` decode documents the wrong layout: `data_flags` is Next Speaker + Flags 1, Goodbye is not `0x80`, Flags 2 is dropped, and Skyrim INFOs do author `DATA`

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/5295
- **Labels**: low,esm-plugin,bug,dialogue,game:fo3,game:fnv
- **Source**: `docs/audits/AUDIT_ESM_2026-10-05.md` (ESM-2026-10-05-D2-01)

_From `docs/audits/AUDIT_ESM_2026-10-05.md` (2026-10-05 comprehensive audit suite, HEAD a2c24b16e)._

- **Severity**: LOW. No consumer reads it yet; it becomes a dialogue bug when one lands.
- **Dimension**: Sub-Record Byte Accounting
- **Record / Sub-record**: `INFO` / `DATA`
- **Location**: `crates/plugin/src/esm/records/misc/dialogue.rs:253-265` (`InfoRecord::info_type` / `data_flags` docs), `:575-580` (`parse_info` arm)
- **Status**: NEW. It is a residual of closed #4469.
- **Description**: The field is documented as "FO3 / FNV: `Flags 1`, with bit `0x80` = Goodbye … Skyrim+ INFOs author no `DATA`". All three parts are wrong:
  - **The u16 spans two fields.** xEdit FO3/FNV INFO `DATA` is `Type u8, Next Speaker u8 (wbNextSpeaker, Common:8537), Flags 1 u8, Flags 2 u8`, `SetOptionalFrom(3)` (`wbDefinitionsFO3.pas` INFO). `data_flags = u16(bytes 1..2)` is therefore Next Speaker in the low byte and Flags 1 in the high byte.
  - **The bit is wrong.** Goodbye is Flags 1 bit 0, which is `0x0100` of the u16. `0x80` is bit 7 of Next Speaker, whose values are only 0..2, so it is never set. The cited `wbINFOAfterLoad` (`FO3.pas:2206`) tests `DATA\Flags 1 and $80`, which is *Speech Challenge* (DNAM retention), not Goodbye.
  - **Flags 2 (byte 3) is never stored.** It carries Say Once a Day and Always Darken.
  - **Skyrim does author `DATA`.** TES5 INFO defines `DATA` as `Quest Dialogue Tab u16, Response Flags u16, Reset Days f32`. On those records `info_type` gets the tab's low byte, and `data_flags` gets the tab's high byte plus the low byte of the response flags.
- **Evidence**: real-byte census (`scripts/info_data.py`):

  | master | INFOs with `DATA` | `DATA` length | byte 1 values | byte 2 bits |
  |---|---|---|---|---|
  | Oblivion | 19,278/19,278 | 3 B ×19,276 | {0, 1, 2} | — |
  | FO3 | 22,327 | 4 B ×21,693, 3 B ×634 | {0: 22,128, 1: 199} (Next Speaker) | bit 0 (Goodbye) ×4,716, bit 7 (Speech Challenge) ×263 |
  | FNV | 23,247 | 4 B ×23,247 | — | bit 0 ×8,275 |
  | Skyrim | 924/31,465 | 8 B | — | — |
  | FO4 | 0 | — | — | — |

  `grep` finds no reader of `InfoRecord.data_flags` / `info_type` outside `dialogue.rs`.
- **Impact**: None today. The first consumer that follows the doc and tests `data_flags & 0x80` for Goodbye will never see it on FO3/FNV (4,716 + 8,275 Goodbye lines missed). If it decodes the high byte instead, it reads Speech Challenge. Skyrim's 924 legacy `DATA` INFOs would feed a garbage type and flags.
- **Related**: #4469 (closed), #5224 (the DIAL-side twin, decoded correctly), `/audit-scripting` P4 dialogue.
- **Suggested Fix**: Split the decode into `next_speaker: u8`, `flags1: u8`, `flags2: Option<u8>`, with the FO3/FNV and Oblivion meanings cited, or thread `GameKind` into `parse_info`. Skip or separately type the Skyrim 8-byte `DATA`. Pin the fix with a test on a real FO3 Goodbye INFO.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
