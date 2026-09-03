# #3712 — NIF-2026-08-30-D3-01: Oblivion's eight DLC archives (1,580 NIFs, 16.4%) are guarded by no parse gate

**Severity**: MEDIUM · **Dimension**: Block Dispatch Coverage
**Location**: `crates/nif/tests/common/mod.rs`, `parse_real_nifs.rs`, `block_coverage_baselines.rs`

## Premise, re-verified against real data

Re-ran the issue's own full-corpus sweep against this machine's real
Oblivion GOTY install: **9,612 NIFs across 9 archives, 8,032 gated (base
only) / 1,580 ungated (8 DLC archives), 16.4%** — exact match to the
issue's evidence table.

## Fix

- **`Game::optional_mesh_archives`** — populated Oblivion's arm with all
  eight vanilla DLC archives (`Knights.bsa`, the six small plugin
  archives, `DLCShiveringIsles - Meshes.bsa`). Kept out of `mesh_archives`
  (not promoted to the required, all-or-nothing tier) per that fn's own
  rule — a base-game install must not lose the whole gate.
- **`parse_rate_oblivion`** — needed **zero code changes**: `run_game`
  already generically extends its required-tier archives with
  `open_optional_mesh_archives(game)` (the #3369 plumbing), so populating
  the list alone widened this gate from 8,032 to the full 9,612 NIFs.
  Confirmed by running it: `9612/9612 NIFs: clean 100.00%`.
- **`oblivion_block_count_parity`** — this one genuinely needed code
  changes, because `open_optional_mesh_archives`'s own doc explicitly
  warns it's "only safe for rate-based gates — never for the count-keyed
  baseline harnesses" (a single monolithic count comparison would make a
  base-game-only CI host's absent DLC read as a "parsed count dropped"
  regression). Widened to open both tiers, but re-keyed `parsed` and the
  truncating-file set **per archive** rather than as one global sum: an
  absent optional archive contributes nothing and is never compared,
  while a present one is checked against its own independently-tracked
  baseline slice. This is safe specifically because — unlike Skyrim SE's
  Creation Club tier — Oblivion's DLC content is static vanilla content
  (GOTY/Deluxe own the set or don't; it never rotates per account), so a
  once-captured per-archive count stays reproducible for the life of that
  install.
- **Baseline regenerated**: `oblivion_truncations.tsv` now carries one
  `archive_parsed\t<name>\t<count>` line per archive instead of one
  aggregate `parsed\t<n>` line. Regenerated against the real corpus: **all
  9,612 NIFs parse whole, 0 truncating** across every archive including
  the DLC — the same clean result the issue's own evidence table showed.

## SIBLING (issue's own checklist item — "`Skyrim - Animations.bsa` (44 NIFs) is in neither list")

Added it to `Game::SkyrimSE`'s `optional_mesh_archives` alongside the
existing five Creation Club entries — checked in the same pass as
requested.

## TESTS (issue's own checklist item — "the widened gate must actually open the DLC archives when present")

- Extended the existing `archive_tiers_are_disjoint_and_skyrim_optional_is_populated`
  test (needs no game data — pure logic on the const lists, runs on every
  CI host) to also pin Oblivion's 8 DLC entries and the new
  `Skyrim - Animations.bsa` entry, mirroring the exact "if someone
  simplifies this back to an empty list, X NIFs silently fall out of the
  gate" pattern #3369 established for Skyrim SE.
- Verified the per-archive-scoped design's core safety property directly:
  pointed `BYROREDUX_OBLIVION_DATA` at a scratch directory containing
  only a symlinked base archive (simulating a non-GOTY install) and
  confirmed `oblivion_block_count_parity` still passes cleanly, checking
  only the 1 present archive — the exact scenario the per-archive keying
  exists to protect against.
- Verified the guard actually catches a regression (this session's
  established quality bar): hand-edited the checked-in baseline to
  inflate `Knights.bsa`'s count from 75 to 100, reran — it failed with
  the exact expected message (`parsed NIF count dropped 100 -> 75`),
  then restored the correct baseline and confirmed a clean pass again.

## Verification

- `cargo check -p byroredux-nif --tests`: clean.
- `cargo test -q -p byroredux-nif`: 1,221 lib tests + all integration
  suites passing, 0 failing.
- `cargo test -q --no-fail-fast` (full workspace): **7086 passing, 0
  failing** (unchanged — this fix extended an existing test's body rather
  than adding a new `#[test]` function, and the corpus-dependent tests
  stay `#[ignore]`d as before, though all were run manually against real
  data above).
