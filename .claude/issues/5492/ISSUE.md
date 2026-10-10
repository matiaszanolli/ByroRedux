# #5492: ESM-2026-10-09-D2-02: `PKDD` Dialogue Type is read from byte 0, which is the low byte of the FOV float, so #5376's Say-To decline gate never fires

**Labels**: ai, bug, dialogue, esm-plugin, game:fnv, game:fo3, medium

**Source**: `docs/audits/AUDIT_ESM_2026-10-09.md` — finding `ESM-2026-10-09-D2-02` · suite `/audit-suite --preset streaming-deep` (2026-10-09) · HEAD `3bcf6c8e8`

- **Severity**: MEDIUM. A field that a runtime gate consumes is decoded from the wrong offset. The gate's stated behaviour is dead on all vanilla data.
- **Dimension**: Sub-Record Byte Accounting
- **Record / Sub-record**: `PACK` / `PKDD`
- **Location**:
  - `crates/plugin/src/esm/records/misc/pack.rs:66-71`: field doc, typed `u8`.
  - `crates/plugin/src/esm/records/misc/pack.rs:818-829`: the arm, `out.dialogue_type = sub.data[0];`, with the comment "Byte 0 is the Dialogue Type enum".
  - Consumer: `byroredux/src/npc_spawn/ai_package.rs:283-293` (`&& package.dialogue_type == 0`).
- **Status**: NEW. This part of the closed #5376 fix does not work. The 10-08 FO3/gameplay censuses counted the right field in Python, but the Rust arm reads a different one.
- **Description**: xEdit FO3 (`wbDefinitionsFO3.pas:6579-6593`) and FNV (`wbDefinitionsFNV.pas:7228-7248`) define `PKDD` as:
  - `FOV` f32 @0
  - `Topic` FormID @4
  - `Flags` u32 @8
  - unused 4 @12
  - **`Dialogue Type` u32 @16** (0 Conversation, 1 Say To)
  - unknown 4 @20
  - `SetOptionalFrom(3)`, so 12/16-byte payloads carry no type.

  The decode takes `sub.data[0]`. That is the least significant byte of the FOV float, which is `0x00` for every round FOV (100.0 = `0x42C80000`).
- **Evidence** (`scripts/pkdd.py`, `scripts/sayto_refs.py`):

  | master | `PKDD`s | byte 0 values | `u32 @16` = Say To | Say-To packs on an `NPC_`/`CREA` `PKID` list |
  |---|---|---|---|---|
  | `Fallout3.esm` | 395 (24 B: 385) | `{0: 395}` | 19 | 17 (e.g. `CG03JonasToPlayer`, `ParadiseFallsGrouseWarnPlayerOnce`, `FFEU10Robot1Dialogue`, `ZipFollowPlayer`) |
  | `FalloutNV.esm` | 339 (24 B: 339) | `{0: 339}` | 21 | 8 (e.g. `LilyTabithaBark`, `RaulTabithaBark`) |

  `FFEU10Robot1Dialogue` authors no `CTDA` and no `PTDT`. It therefore passes the "fully modeled" and "targets player" gates trivially, and installs a full player conversation.
- **Impact**: Every Say-To dialogue package that passes the other two gates still installs the ambient force-greet bridge. Under vanilla rules it should speak one line with no menu; here it takes over the player's controls with a full menu. This is the exact case #5376 says it fixed. The unit test (`ai_package.rs:1236-1238`) sets `dialogue_type = 1` on a hand-built `PackRecord`, so it bypasses the decoder and stays green.
- **Related**: #5376 (closed), #5367, FO3-2026-10-08-D5-01 / #5429 (FO3 dialogue guards are FNV-only).
- **Suggested Fix**:
  - Read `Dialogue Type` as the `u32` at offset 16 when `sub.data.len() >= 20`, and default to Conversation (0) otherwise. Cite FO3.pas/FNV.pas.
  - Optionally decode `FOV` and `Flags` alongside it.
  - Add a parse-level test that feeds a 24-byte `PKDD` with FOV 100.0 and type 1. Add a real-data floor: FO3 19 / FNV 21 Say-To packs.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other walkers / record decoders / games / call sites)
- [ ] **TESTS**: A regression test pins this specific fix
