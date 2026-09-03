# #2633 — SF-D3-05: duplicate field names silently last-wins in CDB reader where Gibbed reference hard-fails

**Severity**: LOW · **Dimension**: 3 (CDB Material Database)
**Location**: `crates/sfmaterial/src/reader.rs::read_user_class`

## Fix

Verified the premise: `read_user_class`'s three `fields.insert(...)` call
sites (the issue's cited line numbers had drifted from intervening
commits, but the sites themselves — non-diff field read, diff field
read, chunk-field read — still exist unchanged in shape) all used
`BTreeMap::insert`'s silent overwrite-on-collision behavior. A `CLAS`
declaring the same field name twice, or a `DIFF` naming the same field
index twice, silently kept the second value — the Gibbed reference
(`Dictionary.Add`) throws on exactly this.

Applied the issue's own suggested fix, choosing "a real `Err`" over
`debug_assert!` — this is untrusted file-format input (a hostile or
corrupted CDB), and a `debug_assert!` vanishes in release builds,
leaving exactly the "silent wrong value" risk the issue is about intact
for the build that matters. Added `Error::DuplicateFieldName {
class_name, field_name }` and a shared `insert_field` helper (used at
all three sites) that checks for a pre-existing key before inserting.

## SIBLING (issue's own checklist item)

Checked the one other `.insert()` call in this file —
`state.class_by_name_offset.insert(class.name_offset, idx)` — but this
is a structurally different map (class-name-offset → class index,
populated once per declared `TYPE` entry) guarding against a different
anomaly (two distinct classes sharing a name-offset), not the field-name
collision this issue is about. No evidence Gibbed's reference treats
that map the same way as `Dictionary.Add`-backed field storage, so left
out of scope per the no-guessing policy rather than assumed.

## TESTS (issue's own checklist item — "a duplicate-field-name fixture
asserts the new error/assert behavior")

`insert_field_rejects_a_duplicate_field_name` — a direct unit test of
the new helper (no full synthetic CDB byte-stream needed, since
`insert_field` is a small, pure, directly-testable function): two
distinct field names insert cleanly, then re-inserting the first name
must return `Error::DuplicateFieldName` with the right class/field
names, and the original value must survive untouched (no partial
overwrite on the rejected insert).

**Reintroduce-and-revert verification**: temporarily restored the bare
`fields.insert(field_name, value); Ok(())` (dropping the duplicate
check) — confirmed the new test failed
(`"re-declaring an already-present field name must fail"`). Restored
the fix and reran — all 17 tests in `byroredux-sfmaterial`'s `reader`
module pass again.

## Verification

- `cargo check -p byroredux-sfmaterial --tests`: clean, zero warnings.
- `cargo test -q -p byroredux-sfmaterial`: 17 + 6 + 1 tests passing, 0
  failing (+1 new; the vanilla-corpus test stays `#[ignore]`d, needs
  real Starfield game data on disk).
- `cargo test -q --no-fail-fast` (full workspace): **7182 passing, 0
  failing**.
