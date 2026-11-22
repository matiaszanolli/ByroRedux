# #7214 CHAR-2026-11-21-L-17: Rule rows sourced only in module docs and PRDs: Oblivion block, fall and bow, Skyrim archery, FO3/FNV crippled-limb effects; the Acrobatics capture still calls fall "not a CHARAL build target"

**Labels**: low,character,documentation,doc-rot,game:oblivion,game:skyrim,game:fo3,game:fnv

From `docs/audits/AUDIT_CHARACTER_2026-11-21.md` (`/audit-character`, HEAD `a06d5f9db`).

- **Severity**: LOW
- **Dimension**: Coverage & Doctrine (seam sourcing)
- **Game**: oblivion, skyrim, fo3, fnv
- **Location**: `crates/core/src/character/{block,bow,fall,limb_effects}.rs` module headers; `docs/engine/charal-oblivion-ruleset.md:354-382` (Acrobatics); no Block or Bow section in `charal-oblivion-ruleset.md`, no Archery section in `charal-skyrim-ruleset.md`, no crippled-limb section in `charal-fnv-fo3-ruleset.md`
- **Status**: NEW (merges CHAR-D1-05, the capture half of CHAR-D2-08, and item 6 of Dim 3's CHAR-D3-05)
- **Source**: the rows' own cited sources, all re-checked against the local wiki dumps and matching (Constant table rows 54-61 and the Dim 3 limb rows):
  - UESP *Oblivion:Block*: shield 1% per level, capped at 75%; weapon 0.5%; bare hands 0.25%; skill capped at 100.
  - CS wiki `fJumpFall*`: 80 / 0.2 / 0 / 1.25 / −0.01.
  - CS wiki `fArrowBowTimer*`, `fArrowSpeedMult`, `fArrowWeakSpeed`.
  - UESP *Skyrim:Archery*: +0.5% per point, tilt 2 / 2.5, range 4096.
  - GECK *Movement Settings* / *Gun Spread Formula* / *Weapon Damage Formula*: 0.85 / 0.75, 0.5 / −0.5, 0.2 / 0.8. The page's "10%" prose disagrees with its own formula, which `limb_effects.rs` discloses.
- **Description**: The skill makes `charal-*-ruleset.md` the authority for every constant, but these rows' numbers live only in module headers:
  - **Block**: also in `skill-use.md:144`.
  - **Bow**: in `docs/prd/player-archery.md`.
  - **Limb effects**: in `docs/prd/vats.md` §5.3.

  The Acrobatics capture records only `1.25 − Acro/10000`, omits the `fJumpFall*` defaults the built row encodes, and still says the formulas are "documented for completeness, not a CHARAL build target", which the built `fall` row now contradicts. No value is claimed wrong. The rows are cited, not invented, so this is a gap in the documents of record, not a guessed number.
- **Impact**: a deep `/audit-character` run cannot verify these rows from the capture documents.
- **Suggested Fix**: add Block, Bow and Fall subsections to `charal-oblivion-ruleset.md`, an Archery subsection to `charal-skyrim-ruleset.md` and a "Crippled-limb effects" subsection beside VATS in `charal-fnv-fo3-ruleset.md`, each quoting the source line; update the Acrobatics note.

## Completeness Checks
- [ ] **SIBLING**: Same stale claim grepped across `docs/engine/charal*.md`, `docs/feature-matrix.md`, `ROADMAP.md` and code comments
- [ ] **TESTS**: Where a source-text pin is suggested, it is added in the same commit
