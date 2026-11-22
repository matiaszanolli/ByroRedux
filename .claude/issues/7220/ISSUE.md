# #7220 CHAR-2026-11-21-L-23: M65 status lags in the captures, `sneak-detection.md` and its index row; the `mod.rs` components bullet under-lists

**Labels**: low,character,documentation,doc-rot,game:skyrim,game:fo4,game:fo76,game:starfield

From `docs/audits/AUDIT_CHARACTER_2026-11-21.md` (`/audit-character`, HEAD `a06d5f9db`).

- **Severity**: LOW
- **Dimension**: Coverage & Doctrine
- **Game**: skyrim, fo4, fo76, starfield
- **Location**:
  - `docs/engine/charal-skyrim-ruleset.md:211-215` ("Not wired into `stealth.rs`"; the header marks the Visual/skill halves PENDING).
  - `docs/engine/charal-fo4-ruleset.md:48` and `:201-204` (sneak detection a "DEAD END").
  - `docs/engine/charal-fo76-ruleset.md:119-121` (points at FO4's dead end).
  - `docs/engine/charal-starfield-ruleset.md` (silent on detection).
  - `docs/engine/sneak-detection.md:95-96` ("`SightOnly` (Starfield, per Q2)") and `:3` (status leads with "DESIGN (Phase 0 …)").
  - `docs/engine/index.md:24` ("DESIGN (Phase 0, 2026-11-15)").
  - `crates/core/src/character/mod.rs` docstring `components` bullet.
- **Status**: NEW (merges CHAR-D5-06, CHAR-D5-07 and the Skyrim-stealth half of CHAR-D2-08)
- **Description**: "Code ahead of capture is doc rot too."
  - `StealthKind` has seven variants: `Skyrim`, `Fallout4` and `Starfield` are built rows (`stealth/{skyrim,fallout4,starfield}.rs`) consumed by `detection/tick.rs`. Only FO76 and the unsupported `NONE` profile answer from `SightOnly`.
  - The PRD shipped 2026-11-19, but `sneak-detection.md` and its index row still lead with Phase 0 design.
  - The FO4 "dead end" is true of the documented wiki source. The built FO4/Starfield rows are executable-derived (the `stealth.rs` header says so). The capture should state both facts; whether to keep, re-source or relabel those rows is the owner's open call under the 2026-11-20 no-exe-decode ruling, and this audit recommends no decode.
  - The `mod.rs` `components` bullet lists seven types but omits `CharacterIdentity`, `DismemberRecord`, `SkillUseLevel` and `PendingLevel`. `mod_docstring_indexes_every_sub_module` checks module names only.
- **Impact**: a reader of the Skyrim or FO4 capture will scope work that is already built, or treat FO4 detection as impossible; the index row misstates the doc's currency.
- **Related**: L-22 (the matching `profile.rs` `stealth` field doc), #6908 (M65 tracking).
- **Suggested Fix**: add "Status: built as `StealthKind::X` (M65); provenance per `sneak-detection.md`" to the four captures; replace the `kind` line with the seven-variant list; mark the status line and the index row "BUILT (PRD SHIPPED 2026-11-19)"; add the four component names to the docstring bullet.

## Completeness Checks
- [ ] **SIBLING**: Same stale claim grepped across `docs/engine/charal*.md`, `docs/feature-matrix.md`, `ROADMAP.md` and code comments
- [ ] **TESTS**: Where a source-text pin is suggested, it is added in the same commit
