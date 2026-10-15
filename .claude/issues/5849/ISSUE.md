# TD8-2026-10-15-01: The blocking clippy gate is red again (12 lints in 2 crates) and leaves `byroredux-renderer` (lib) and the `byroredux` bin unlinted; the bin carries 11 hidden lints

Labels: low,renderer,scripting,tech-debt,bug

Filed from `docs/audits/AUDIT_TECH_DEBT_2026-10-15.md` (finding `TD8-2026-10-15-01`).

- **Severity**: LOW
- **Dimension**: 8 - Dead Code & Backwards-Compat Cruft (clippy gate)
- **Location**: renderer build script: `crates/renderer/src/shader_constants_data.rs:683` (`GROUNDCOVER_MODEL_MAX_RECORDS % 32 == 0`, `manual_is_multiple_of`). `byroredux-scripting` lib (11): `fragment/package.rs:87` (`multiple_bound_locations`, `E: Copy` + `where E: Clone`), `animation_graph.rs:721` (`nonminimal_bool`), `animation_modifiers.rs:125` (`only_used_in_recursion`, `events`), `animation_modifiers.rs:174` (`collapsible_match`), `encounter_level.rs:24` (`doc_lazy_continuation`), `fragment/reference.rs:657` (`nonminimal_bool`), `fragment/systems.rs:13` (`type_complexity`), `fragment/vm.rs:2811` + `:2821` (`unnecessary_map_or`), `fragment/vm.rs:5270` (`collapsible_if`), `obscript_quests.rs:200` (`drop_non_drop`, `drop(host)`). Hidden by the red gate, in the `byroredux` bin (11, seen only with `-D` removed): `app_frame.rs:955`, `cell_loader/graph_nodes.rs:224`, `cell_loader/spawn.rs:1468` (three `too_many_arguments` 8/7), `cell_loader/graph_nodes.rs:228` (`only_used_in_recursion`, `_needed`), `asset_provider/object_graph.rs:140` + `asset_provider/script.rs:192` (`type_complexity`), `cell_loader/terrain.rs:1124` (`redundant_locals`), `components/projectile_muzzle.rs:9-11` (3x `doc_lazy_continuation`), `systems/graph_locomotion.rs:145` (`map_clone`)
- **Status**: NEW (the 10-12 lints, #5621/#5744, are all cleared; this is a fresh wave — same class as closed #5621, not a re-open of the same sites)
- **Age**: every site is from the last 48 h (blame): scripting lints from `1a12cef0e`, `6e6b1da5d`, `ef0bc42fb`, `9c8e0161a`, `d5c59fcb3`, `f67d82b3c` (10-13) and `49529db7c`, `e2a234c6e` (10-14); renderer lint `c14dd2b39` (10-14); bin lints `aabd99a05`, `6db3805a5`, `caa81f212`, `f6613fd6a`, `017968468`, `592f31b87`, `f6a64fe20`, `746ba8dfd` (10-13/14)
- **Effort**: small
- **Description**: `cargo clippy --workspace --keep-going -- -D warnings` fails `byroredux-renderer` (build script — the renderer lib is then skipped) and `byroredux-scripting` (lib, 11 errors). `byroredux` depends on both, so the highest-churn crate (the bin) is never linted by CI. The renderer's separate `undocumented_unsafe_blocks` re-lint step (`-p byroredux-renderer --no-deps -- -D clippy::undocumented_unsafe_blocks`) is unaffected, so the #4595 guard is still armed. 12 of 12 gate errors and 11 of 11 hidden bin lints were introduced after the 10-12 audit; the previous wave was cleared in the same 72 h window, so this is a re-accrual pattern, not a stuck backlog.
- **Evidence**: `/tmp/audit/tech-debt/d8_clippy_gate.txt` (gate), `d8_clippy_nod.txt` (no `-D`); reverse-dependency check via `cargo metadata`: only `byroredux-renderer`, `byroredux-scripting`, `byroredux` are in the unlinted closure.
- **Impact**: CI `Test + Check + Clippy` is red on every push until fixed; the bin stays lint-blind meanwhile.
- **Related**: #5621 (closed, same class), #5622 (open), TD8-2026-10-15-02, TD8-2026-10-12-01.
- **Suggested Fix**: apply the rustc suggestions (4 are machine-applicable: the two `map_or` -> `is_none_or`, the `collapsible_if`, `is_multiple_of`); `drop(host)` -> let the borrow end by scope; `populate_fragments` fold `E: Copy` into the `where` clause; give `graph_nodes.rs:224` / `spawn.rs:1468` / `app_frame.rs:955` a parameter struct or a reasoned `#[allow]`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files/docs
- [ ] **TESTS**: A regression test or guard pins this specific fix (or the existing guard is named)
