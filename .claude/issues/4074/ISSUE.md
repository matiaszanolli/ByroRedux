# #4074 — ESM-2026-09-09-D7-04

`main_body_bit`'s FO76 and Starfield arms are still unsourced — the one inference in the file the project holds up as its citation-discipline example

Filed 2026-09-09 by `/audit-publish` from `docs/audits/AUDIT_ESM_2026-09-09.md`.
Snapshot of the issue **as filed** — GitHub is authoritative for current state
(`gh issue view 4074 --json state`).

---

- **Severity**: MEDIUM
- **Dimension**: ESM→ECS Handoff
- **Record / Sub-record**: `ARMO` / `BOD2` (`BMDT` on pre-Skyrim)
- **Location**: `crates/plugin/src/equip.rs:24-30` (bit-layout table), `:69-81`
  (`main_body_bit`), `:637-686` (tests)
- **Status**: NEW — re-verified against **code**, not against issue absence.
  Sourced from `AUDIT_ESM_2026-08-13.md` ESM-D7-06 (pre-2026-06-07, so per protocol
  the code is the authority). `/tmp/audit/esm/issues.json` also has no matching
  issue in any state, so it was never filed.
- **Description**: the module header states its authority precisely — xEdit
  *wbDefinitions{TES4,FNV,TES5,FO4}.pas* at tag `dev-4.1.6`, commit valid 2026-05-07 —
  and every one of the four tests cites a specific `.pas` line. `GameKind::Fallout76`
  and `GameKind::Starfield` are then routed to bit 3 on the strength of *"FO76
  inherits FO4's layout per Bethesda's typical incremental reuse pattern"*, with
  Starfield not mentioned in the comment at all and neither game present as a column
  in the module's own bit-layout table.
- **Evidence**:
  ```rust
  // crates/plugin/src/equip.rs:76-80
          // FO4 reorganised the layout — bit 2 became "FaceGen Head"
          // and bit 3 became BODY. FO76 inherits FO4's layout per
          // Bethesda's typical incremental reuse pattern.
          GameKind::Fallout4 | GameKind::Fallout76 | GameKind::Starfield => Some(3),
  ```
  ```
  // crates/plugin/src/equip.rs:24  — the table has four columns, not six
  //! | bit | Oblivion (BMDT u16) | FO3 / FNV (BMDT low u16) | Skyrim+ (BOD2 u32) | FO4 (BOD2 u32) |
  ```
  Test coverage: `fnv_upper_body_bit_is_2` / `oblivion_upper_body_bit_is_2` /
  `skyrim_body_bit_is_2` / `fo4_body_bit_is_3` each assert a positive case with a
  `.pas` line citation (`equip.rs:639,650,657,665`). FO76 and Starfield appear only
  in `empty_flags_never_cover_body` (`equip.rs:673-686`), which asserts
  `!armor_covers_main_body(game, 0)` — true for *any* bit choice, so it pins nothing.
- **Impact**: `armor_covers_main_body` decides whether the base body NIF is skipped
  when an armour is equipped. If the FO76/Starfield bit is wrong the failure is
  visual and silent in both directions — a naked torso under armour that no longer
  suppresses the body, or a doubled torso that z-fights and doubles the skinned bone
  palette (the exact cost the function's own doc names). Per the project's no-guessing
  rule an unsourced constant with no positive test is itself the reportable item,
  independent of whether the value happens to be right.
- **Related**: `AUDIT_ESM_2026-08-13.md` ESM-D7-06; the *No Guessing Policy* memory.
- **Suggested Fix**: cite `wbDefinitionsFO76.pas` / `wbDefinitionsSF1.pas` at
  `dev-4.1.6` (or whatever tag carries them) for both arms, add the two missing
  columns to the header table, and add a positive test per game with the line
  citation — matching what the other four arms already carry. If xEdit has no
  Starfield biped table, say so in the comment and mark the arm provisional rather
  than asserting inheritance.

---
