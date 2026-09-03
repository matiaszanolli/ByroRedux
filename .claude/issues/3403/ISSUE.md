# #3403 — ESM-2026-08-27-D7-01: merge_from last-write-wins on EsmIndex::game — one failed plugin parse silently re-labels the whole load order as Fallout 3/NV

**Severity**: MEDIUM · **Dimension**: `EsmIndex` → ECS Handoff
**Location**: `crates/plugin/src/esm/records/index.rs::EsmIndex::merge_from`

## Fix

Verified the premise: `self.game = other.game;` was unconditional, and
`parse_record_indexes_in_load_order` (`byroredux/src/cell_loader/load_order.rs`)
merges `EsmIndex::default()` — `game: GameKind::default() = Fallout3NV`
— when a plugin's parse fails. A failure in the *last* plugin of a
Skyrim/FO4/Starfield load order therefore silently re-labelled the whole
merged index as Fallout3NV, behind an unrelated `log::warn!`. `game` is
a broad dispatch key (skeleton/animation paths, terrain-LOD layout,
player base form, Havok gating per the issue's own consumer list), not
a cosmetic label.

Applied the issue's own suggested one-line fix exactly: skip the `game`
merge when `other.total() == 0` — the same "a failed parse contributes
nothing" signal the existing `warn!` already implies. Did not reuse the
`character_rules` guard immediately above (the existing comment already
explains why: callers construct an index and set `game` without ever
setting `character_rules`, so gating the two together would silently
drop `game` for those callers).

## SIBLING (issue's own checklist item — "every other scalar field
`merge_from` last-write-wins on")

Scanned the whole function body for direct `self.<field> = ...`
assignments — exactly two exist: `character_rules` (already guarded,
first-non-`NONE`-wins, #3384) and `game` (now guarded here). Every other
field in `merge_from` is a collection merge (`extend`/`remove`/a
per-category `merge` table), not a scalar overwrite, so no other site
needed the same fix.

## An existing test's fixture was unrealistic, not the fix wrong

`merge_from_adopts_the_first_real_profile`
(`crates/plugin/src/esm/records/tests.rs`) broke: its `good` fixture set
`character_rules`/`game` directly on a hand-constructed `EsmIndex` with
no records inserted anywhere, making it observably empty
(`total() == 0`) despite the test's own name and narrative ("the first
*real* profile"). The issue's own suggested fix explicitly accepts this
exact tradeoff — "a genuinely empty-but-successfully-parsed plugin's own
correct game detection is also skipped" — as strictly safer than the
defect it replaces. Updated the fixture to insert a real record
(matching this issue's own new test's pattern), making the test's
premise honest instead of walking back the fix.

## TESTS (issue's own checklist item — "a regression test pins this
specific fix")

- `merge_from_skips_game_when_other_is_observably_empty` — the exact
  scenario: an already-`Skyrim`-detected merged index must not be
  overwritten by merging in a bare `EsmIndex::default()` (the
  failed-parse shape).
- `merge_from_still_applies_game_when_other_has_content` — companion:
  the guard must only exclude the observably-empty case, not every
  non-first plugin — a real DLC/patch plugin's `game` still merges
  normally.

**Reintroduce-and-revert verification**: temporarily restored the
unconditional `self.game = other.game;` — confirmed the new test failed
with exactly the bug's symptom (`left: Fallout3NV, right: Skyrim`).
Restored the fix and reran — all 21 `merge_from_` tests across both
`index.rs` and `records/tests.rs` pass again.

## Verification

- `cargo check -p byroredux-plugin --tests`: clean (the pre-existing
  unrelated `grup_walker.rs:469` `unused_mut` warning is present and
  out of scope).
- `cargo test -q -p byroredux-plugin --lib merge_from_`: 21 passing, 0
  failing (+2 new).
- `cargo test -q -p byroredux-plugin`: 916 passing, 0 failing (no other
  fixtures affected).
- `cargo test -q --no-fail-fast` (full workspace): **7181 passing, 0
  failing**.
