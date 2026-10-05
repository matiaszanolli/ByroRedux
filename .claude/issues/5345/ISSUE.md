# #5345: LC-D3-01 (2026-10-05): FO76 is left out of the LSCR "modern" family, so 356 FO76 model load screens and their 4,064 parsed TRNS transforms are dropped

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/5345
- **Labels**: low,legacy-compat,esm-plugin,bug,game:fo76
- **Source**: `docs/audits/AUDIT_LEGACY_COMPAT_2026-10-05.md` (LC-D3-01)

_From `docs/audits/AUDIT_LEGACY_COMPAT_2026-10-05.md` (2026-10-05 comprehensive audit suite, HEAD a2c24b16e)._

- **Severity**: LOW
- **Dimension**: 3 — Cross-game translation pattern (Pattern B mis-dispatch: the wire format matches FO4, but the game enum excludes FO76)
- **Location**: `crates/plugin/src/esm/records/load_screen.rs:58-62` (the `legacy` / `modern` sets), `:75-76` and `:106-108` (the `TNAM` / `ZNAM` gate), `:93` (`ICON`), `:103`, `:129`; and the module doc at `:4-5`.
- **Status**: NEW. It landed with `e60911864` (LSCR model cover). No open or closed issue matches, and no report dated 2026-10-02..05 mentions FO76 LSCR.
- **Description**:
  - `parse_lscr` partitions games into `legacy` (Oblivion, FO3NV) and `modern` (Skyrim, FO4, Starfield). It gates `TNAM` / `ZNAM` on FO4 or Starfield.
  - `GameKind::Fallout76` is in neither set, so for FO76 every presentation sub-record falls through to `_ => {}`: `NNAM`, `TNAM`, `ONAM`, `ZNAM` and `MOD2`.
  - xEdit `wbDefinitionsFO76.pas` `wbRecord(LSCR)` is the FO4 shape: `NNAM` → STAT/SCOL, `TNAM` → TRNS, `ONAM` s16×2, `ZNAM` f32×2, `MOD2`. It adds `BNAM` 'Background Image' (string) and `LSST`.
  - The module doc lists TES4/FO3/FNV/TES5/FO4/SF1 as references and does not mention FO76, so the exclusion is an omission, not a decision.
  - The `TRNS` dispatch arm (`dispatch_misc_stub.rs:146`) *is* game-agnostic, so FO76's transforms are parsed into `load_screen_transforms` with nothing pointing at them.
- **Evidence**: Census of `SeventySix.esm`: 474 LSCR, of which 356 have `NNAM` + `TNAM` + `ONAM`, 439 have `BNAM` and 14 have `ZNAM`; and 4,064 TRNS (36 B ×3,933, 28 B ×131).
  ```rust
  // load_screen.rs:58-62
  let legacy = matches!(game, GameKind::Oblivion | GameKind::Fallout3NV);
  let modern = matches!(
      game,
      GameKind::Skyrim | GameKind::Fallout4 | GameKind::Starfield
  );
  ```
- **Impact**:
  - Every FO76 LSCR decodes with `model == 0`, so `load_screen_verdict` rejects all 474 and no FO76 load cover can ever be selected.
  - It is cosmetic, and FO76 is not a playable target (`game-compatibility.md` "Cell loading: not yet started"). That is why this is LOW.
  - The shape is the one the survey warns about: a per-game enum set that silently omits a game whose wire format already discriminates itself.
- **Related**: `e60911864`, `#5229`; `docs/engine/per-game-translation-survey.md` §5 Pattern B.
- **Suggested Fix**:
  - Add `GameKind::Fallout76` to `modern` and to the `TNAM` / `ZNAM` gate. Optionally decode `BNAM` into `icon` as FO76's image path.
  - Add `wbDefinitionsFO76.pas` to the module doc and an FO76 arm to the LSCR test.
  - If FO76 is meant to stay excluded, say so in the doc instead.

**Publish note (validation)**: the parse-side gap is confirmed. `parse_lscr`'s `legacy` / `modern` sets omit `GameKind::Fallout76`. However, `load_screen_verdict` in the same file already has an explicit `GameKind::Fallout76 | GameKind::Starfield => Rejected(MissingArtwork)` arm, with the comment "FO76 shares FO4's LSCR shape but is outside the engine's covered fixture set". So the *presentation* exclusion is a deliberate decision. Only the parse-side drop and the module doc's silence on FO76 are unrecorded. Either fix is valid: decode FO76 at parse while the verdict stays rejected, or document the exclusion at `parse_lscr` and in the module doc.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
