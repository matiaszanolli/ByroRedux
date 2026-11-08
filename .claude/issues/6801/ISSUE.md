# ESM-D6-06: Starfield English tables omit ids the other languages carry; 85 item names surface as `<lstring 0x…>` placeholders

GitHub: https://github.com/matiaszanolli/ByroRedux/issues/6801
Labels: low, esm-plugin, bug, game:starfield
Source: docs/audits/AUDIT_ESM_2026-11-08.md

_Filed from `docs/audits/AUDIT_ESM_2026-11-08.md` (audit HEAD 3500d2737)._

- **Severity**: LOW / INFO (a decision, not a decoder defect)
- **Dimension**: 6
- **Location**: records/common.rs:211 (`resolve_lstring(id).unwrap_or_else(|| format!("<lstring 0x{:08X}>", id))`)
- **Status**: NEW
- **Description**: with all three Starfield `_en` tables loaded, 85 `full_name`s in Starfield.esm do not resolve (e.g. `Spacesuit_UCPilot_Backpack_01` -> `<lstring 0x00030541>`, `Spacesuit_Navigator_Backpack` -> `<lstring 0x0003000B>`).
- **Evidence**: `Starfield_en.STRINGS` has 48875 entries, every other language 48963 (88 more). Ids 0x0003000B / 0x00030541 / 0x00033C5A / 0x00006154 are absent in `en` and present in `de/fr/es/ja/pl/ptbr/zhhans` as generic labels ("Einstieg"/"Entrée"/"Entry", "Standard"/"Default"). So the English table deliberately (or by build quirk) carries no text for them. This was measured; what the original engine displays for an absent id was not.
- **Impact**: the placeholder text leaks into any name display, and, as #4072 argued for id 0, it defeats `.is_empty()` checks that fall back to the editor ID. The "zero tables resolved" diagnostic (#4073) is the only reason to keep a placeholder for a fully-missing table set; for an id absent from tables that did load, an empty string is arguably the correct value.
- **Suggested fix**: needs a decision: return `String::new()` for an id that is missing from at least one successfully loaded table, keep the placeholder only when no table was loaded. If the original engine behaviour is unknown, leave as is and record the 88-id gap in the Starfield notes.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related decoders / other games' arms
- [ ] **LOCK_ORDER**: n/a unless a RwLock scope changes
- [ ] **TESTS**: A regression test pins this specific fix (real-data pin where an install exists)
