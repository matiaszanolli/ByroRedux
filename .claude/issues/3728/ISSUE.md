# #3728 — ESM-2026-08-30-D5-01: a REFR's persistent/temporary/visible-distant group membership is discarded at parse time

**Severity**: LOW · **Location**: `crates/plugin/src/esm/cell/walkers.rs` (`parse_cell_group_inner`, `parse_refr_group_inner`), `crates/plugin/src/esm/cell/wrld.rs` (`parse_wrld_children_inner`), `crates/plugin/src/esm/cell/mod.rs` (`PlacedRef`)
**Source**: `docs/audits/AUDIT_ESM_2026-08-30.md` (ESM-2026-08-30-D5-01)

Not a duplicate of #3542 (record-*type* dropping) — this is about group-
*type* membership (6=temporary / 8=persistent / 9=visible-distant) of
records that already parse successfully. `parse_cell_group_inner` and
`parse_wrld_children_inner` both matched `6 | 8 | 9` with one shared arm and
passed the same `&mut refs` for all three; `PlacedRef` had no field to carry
it. No consumer is wrong today (the engine's only "persistent" concept is
the worldspace-level persistent CELL), but the split is exactly what a
future streaming system (what to keep resident across cell transitions) and
a save system (what to distinguish on restore) will need — and it was
discarded unrecoverably at parse time.

## Fix implemented

Added `pub group_type: u8` to `PlacedRef` (documented: `6`/`8`/`9`, `0xFF` on
legacy fixtures that predate the field). Threaded a `group_type: u8`
parameter through `parse_refr_group`/`parse_refr_group_inner` the same way
`depth` already threads — fixed across the whole recursive call tree (any
further-nested group inside a 6/8/9 body is still scoped to that same
membership, so it doesn't increment like `depth`). Set on the `PlacedRef`
struct literal at its one construction site.

**SIBLING** (issue's own checklist item): both call sites —
`parse_cell_group_inner` (`walkers.rs`) and `parse_wrld_children_inner`
(`wrld.rs`) — now pass their own `sub_group.group_type as u8` into
`parse_refr_group`, not just one of the two.

**TESTS** (issue's own checklist item):
`persistent_and_temporary_refrs_carry_distinct_group_type` (`tests/cell.rs`)
builds one interior CELL with two children groups — a type-6 (temporary) and
a type-8 (persistent), each holding one REFR — and asserts the resulting
`PlacedRef`s carry `group_type` 6 and 8 respectively, not a shared/collapsed
value.

Signature change touched every `parse_refr_group` call site and every
`PlacedRef` struct literal in the workspace (mechanical parameter/field
threading, matching the existing `depth`-threading pattern exactly — no
semantic ambiguity): `records/grup_walker.rs` (1 test call site),
`cell/tests/refr.rs` (12 call sites), `cell/tests/cell_for_refr.rs` and
`cell/tests/merge.rs` (1 struct literal each), plus two more `PlacedRef`
struct literals in the `byroredux` binary crate
(`cell_loader/references/synth_child_tests.rs`,
`cell_loader/refr_texture_overlay_tests.rs`) that `cargo check -p
byroredux-plugin` alone didn't surface — caught by the final full-workspace
build.

Full workspace: `cargo test --no-fail-fast` 7056 passing, 0 failing (+1 new
test).
