# #7118 — ESM-D8-01: Starfield `LCTN` `ACPR`/`LCPR` (16 B) and `ACSR`/`LCSR` (20 B) walked at FO4's 12/16-byte stride; fabricated location refs, 37,800 warn lines

Labels: low,esm-plugin,bug,game:starfield
Filed from: docs/audits/AUDIT_ESM_2026-11-20.md (finding ESM-D8-01)

Source: `docs/audits/AUDIT_ESM_2026-11-20.md` (`/audit-esm`, HEAD 47ae285c6 / f3713bf5e) · ID **ESM-D8-01**

- **Severity**: LOW
- **Dimension**: Real-Data Validation
- **Record / Sub-record**: `LCTN` `ACPR`, `LCPR` (persist location refs), `ACSR`, `LCSR` (special refs); Starfield only
- **Location**: `crates/plugin/src/esm/records/misc/location.rs:62-112`
- **Status**: NEW (not in the 120 open issues; the orchestrator's ESM-01 covers only DOOR)
- **Description**: the decoder walks `ACPR`/`LCPR` in 12-byte steps (ref, cell/world, grid y i16 + x i16) and `ACSR`/`LCSR` in 16-byte steps (ref type, ref, cell, grid). xEdit `wbDefinitionsSF1.pas:13400-13470` appends a `wbUnknown(4)` to both structs: `ACPR`/`LCPR` are 16 bytes per entry, `ACSR`/`LCSR` are 20. FO4 and FO76 have the shorter layout, so the code is right for them and wrong only for Starfield (which dispatches through the same arms).
- **Evidence**: `Starfield.esm` `LCPR` lengths are 517/517 multiples of 16, and 269+35 of them are not multiples of 12; `LCSR` 714/714 are multiples of 20 while only 135 of 714 are multiples of 16. FO76 and FO4 `LCPR` are 100% multiples of 12 and `LCSR` 100% multiples of 16, so the shared decoder is right for them. Backtrace attribution on the real load path: `location.rs:106` (`ACSR` ref type and ref) 33,735 warns, `location.rs:68` 4,065 warns. Values such as `ffffffff` and `7fff7fff` are the next entry's grid/unknown words read as FormIDs.
- **Impact**: `LocationRecord::editor_location_refs` and `ref_type_refs` hold fabricated references for Starfield locations (6,017 `LCTN`; `crates/scripting/src/location.rs` reads them for `GetInCurrentLocFormList` / location aliases). Practically no Starfield script consumer runs yet, so the visible effect is 37,800 of the 62,576 warns per load. It also adds to ESM-01's "buries the genuine out-of-range warnings" impact.
- **Related**: orchestrator ESM-01 (`DOOR`), ESM-D8-02, ESM-D8-03 (same warn source).
- **Suggested fix**: make the stride a per-`GameKind` constant (Starfield 16 / 20, others 12 / 16) and gate the trailing `wbUnknown(4)`. Add a real-data pin that Starfield `LCPR`/`LCSR` payload lengths are exact multiples of the stride.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related decoders / other games' arms (per-game layout splits, other callers of the same helper)
- [ ] **TESTS**: A regression test pins this specific fix (synthetic fixture in the default lane; real-data pins stay in the ignored lane)
