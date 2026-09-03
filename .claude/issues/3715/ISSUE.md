# #3715 — ESM-2026-08-30-D3-02: the #3400/#3401 remap source guard is a hardcoded 8-parser allowlist, and 11 more embedded-FormID reads sit outside it

**Severity**: MEDIUM · **Dimension**: FormID & Load Order
**Location**: `crates/plugin/src/esm/records/items.rs`, `misc/equipment.rs`, `misc/magic.rs`, `misc/world.rs`, dispatch files, `tests.rs` (guard)

## Fix

Verified each of the 11 cited sites against current source before
touching anything (line numbers in the issue were stale after intervening
edits, as expected). Found the sites split into two real categories:

**Signature already took `remap`, the parameter just wasn't applied at
these specific reads** (`parse_ammo`, `parse_weap`, `parse_perk` — all
three already accept `remap: &Option<FormIdRemap>` for other fields):
- `items.rs` AMMO `parse_ammo`: `projectile_form` (Skyrim/FO76/Starfield
  `DATA`, FO3/FNV `DAT2`, FO4 `DNAM`) and `casing_form` (`DAT2`) — 4 raw
  reads.
- `items.rs` WEAP `parse_weap`: `skill_form` (`ETYP`) — 1 raw read.
- `misc/magic.rs` PERK `parse_perk`: `quest_form_id` / `spell_form_id`
  (`DATA`, inside the `PRKE`/`DATA`/`PRKF` entry block) — 2 raw reads.
  `remap` was applied to this record's condition list via `push_ctda`
  but never to these two fields.

**Signature didn't take `remap` at all** (matching the issue's own
"Verified at HEAD" note, still current for these three):
- `misc/equipment.rs` COBJ `parse_cobj`: `created_form` / `workbench_form`
  (`CNAM`/`BNAM`).
- `misc/magic.rs` MGEF `parse_mgef`: `light_form_id` (`DATA`).
- `misc/world.rs` ECZN `parse_eczn`: `owner_form` (`DATA`).

Added the parameter to these three, wrapped all 9 raw reads across the
six parsers in `remap_fid(..., remap)`, and threaded `remap` through
every call site (`dispatch_misc_gameplay_a.rs` ECZN,
`dispatch_misc_gameplay_b.rs` COBJ/MGEF — all three already had `remap`
in scope from `reader.get_form_id_remap()`, either shared across the
dispatch function or fetched per-arm matching each file's own existing
convention).

Also normalized `parse_perk`'s remap parameter from the fully-qualified
`&Option<crate::esm::reader::FormIdRemap>` to the short `&Option<
FormIdRemap>` (added the import) — every other parser in the crate uses
the short form, and the guard's own source-scan (below) matches on the
exact short-form text.

## SIBLING (issue's own checklist item — "the inverted guard covers
`cell/` as well as `records/`")

Not applicable in the direction implied: this fix took the "extend the
allowlist" fallback rather than inverting the guard (see Guard section
below), so there is no new inverted guard to extend into `cell/`.
Verified instead that `cell/`'s own remap discipline needs no source-scan
guard at all: `cell/helpers.rs::read_form_id(reader: &EsmReader, ...)`
takes the *reader* (not a bare data slice), so the remap is baked into
the one function every caller must go through — a Rust type-level
guarantee (#3314), not a heuristic. `records/`'s `remap_fid(raw, remap)`
free function is the weaker pattern that needed a guard in the first
place; `cell/`'s doesn't have the same escape hatch.

## Guard fix (issue's own suggested fix, second option)

Took the issue's own explicitly-offered fallback rather than the harder
"invert the guard" option: extended the hardcoded allowlist with the six
parsers above (`parse_weap`, `parse_ammo`, `parse_cobj`, `parse_perk`,
`parse_mgef`, `parse_eczn`), so the guard's source-scan now actually
inspects them. Inverting the guard (source-scanning `records/**.rs` for
any FormID-shaped read outside `remap_fid(`) would need a much less
false-positive-prone heuristic than this issue's own text sketches, and
the marginal safety over a maintained allowlist didn't justify the risk
for a MEDIUM-severity, currently-latent finding.

## TESTS (issue's own checklist item — "a regression test proves the new
guard *fails* when a remap parameter is removed from an arbitrary
parser")

Extracted the guard's per-entry check into a pure, panic-free helper,
`parser_signature_takes_remap(source, parser) -> Result<bool, String>`,
and added `parser_signature_takes_remap_detects_a_missing_remap_param` —
calls the helper directly against a synthetic "before the fix" signature
(no remap parameter) and a synthetic "after the fix" one, proving the
detection logic itself would have caught the exact regression this issue
describes, not just that it currently passes against already-fixed
sources.

Field-level remap tests, one per site, following this crate's existing
`FormIdRemap::regular(2, vec![0])` self-reference-vs-master-reference
pattern:
- `items.rs`: `skyrim_ammo_projectile_form_is_remapped`,
  `fo3_ammo_dat2_projectile_and_casing_form_are_remapped`,
  `fo4_ammo_dnam_projectile_form_is_remapped`,
  `weap_etyp_skill_form_is_remapped`.
- `misc/equipment.rs`: `cobj_cnam_and_bnam_are_remapped_into_global_space`
  (added to the existing `#3714` `remap_tests` module).
- `misc/magic.rs`: `parse_mgef_light_form_id_is_remapped`,
  `parse_perk_quest_and_ability_entries_are_remapped`.
- `misc/world.rs`: `parse_eczn_owner_form_is_remapped`.

**Reintroduce-and-revert verification, two levels**:
1. Field level — each of the 8 new tests exercises a genuinely new code
   path (the fix just landed), so there's no meaningful "revert" for
   these individually; correctness is pinned by the assertion itself
   (self-reference remaps, master reference doesn't).
2. Guard level — temporarily inserted a single space into `parse_cobj`'s
   real signature (`remap : &Option<FormIdRemap>`, still valid Rust,
   still compiles) so the exact substring the guard checks for no longer
   matches — confirmed `record_parsers_with_embedded_form_ids_take_a_remap`
   failed with the expected message. Restored the fix and reran — all 40
   tests in `esm::records::tests::` pass again.

## Verification

- `cargo check -p byroredux-plugin --tests`: clean (one pre-existing,
  unrelated `unused_mut` warning in `grup_walker.rs:469` predates this
  fix, out of scope).
- `cargo test -p byroredux-plugin --lib esm::records::`: 594 tests
  passing, 0 failing (+9 new: 8 field-remap tests + 1 checker-logic
  test).
- `cargo test -q -p byroredux-plugin`: 912 tests passing, 0 failing.
- `cargo test -q --no-fail-fast` (full workspace): **7141 passing, 0
  failing**.
