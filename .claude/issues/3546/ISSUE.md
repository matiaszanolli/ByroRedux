# #3546: SK-D4-02: FileHeader::light_master's doc implies ESL decode is mod-only — a stock AE install ships 3 ESL-flagged plugins, one base-game

**Source**: `docs/audits/AUDIT_SKYRIM_2026-08-30.md` — Dimension 4 (Multi-Master Load Order)
**Severity**: LOW
**Location**: `crates/plugin/src/esm/reader.rs` — `FileHeader::light_master` doc comment

## Description

The `light_master` doc states: *"No vanilla Skyrim SE / FO4 / Starfield master is
ESL-flagged; this is for third-party ESL mods and ESL-flagged CC content."* That is true of
**masters**, but it reads as "the ESL decode path is mod-only", which a stock Anniversary
install falsifies.

## Evidence

Measured plugin census over the installed Skyrim SE `Data/` (2026-08-30) — 7 `.esm` +
3 `.esl`, 1,188,821 records total:

| Plugin | records | light_master |
|---|---|---|
| `_ResourcePack.esl` | 374 | **✓** |
| `ccBGSSSE037-Curios.esl` | 152 | **✓** |
| `ccQDRSSE001-SurvivalMode.esl` | 674 | **✓** |
| the 7 `.esm` files | 1,187,621 | — |

3 of 3 `.esl` report `light_master = true`; 7 of 7 `.esm` report `false`. `_ResourcePack.esl`
is **base-game content**, not CC and not third-party, and it carries a **3-entry MAST list**
(`Skyrim.esm`, `Update.esm`, `HearthFires.esm`) — so the ESL path really does have to remap
references into full-byte master slots on vanilla data.

The decode itself is correct: `allocate_global_slot` routes ESL plugins to
`GlobalSlot::Light` (12-bit sub-index in the `0xFE` space, capped at `0x0FFF`) and the rest
to `0x00..=0xFD`, both with overflow checks (#1554).

## Impact

Documentation only — the code is right. But the current wording invites a reader to treat
the ESL branch as untested-by-construction on a clean install, when in fact it is on the
vanilla critical path.

## Suggested Fix

One sentence: no vanilla *master* is ESL-flagged, but a stock AE install ships
`_ResourcePack.esl` (base game, 374 records, 3 masters) plus CC `.esl` files, so the ESL
decode path is exercised by vanilla content.

## Related

#1554 (ESL / light-master decode).

## Completeness Checks
- [ ] **TESTS**: `read_file_header_reads_localized_and_light_master_flags` already covers the flag decode — no new test required for a doc-only change
