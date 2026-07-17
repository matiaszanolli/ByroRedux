# Issues 2023, 2024, 2025, 2026

All four are from `docs/audits/AUDIT_SCRIPTING_2026-07-16.md`.

## #2023 — SCR-D5-NEW2-01 (HIGH): SetObjective{Displayed,Completed,Failed} collapse present-but-non-literal arg into `true`
**Location**: `crates/scripting/src/translate/effects.rs:227-259` (`prim_set_objective_displayed/_completed/_failed`), `bool_arg` helper at `:297-299`

`bool_arg(args, idx)` returns `Some(as_num(...)? != 0.0)`, collapsing "argument absent" and "argument
present but not a literal" (local var, `Not(...)`, copy-propagated temp) into the same `None`. All
three call sites do `bool_arg(args, 1).unwrap_or(true)`, so `Self.SetObjectiveCompleted(20,
bWasSuccessful)` silently becomes `completed: true` regardless of the real runtime value —
`QuestObjectiveState` is persisted in save data, so this corrupts quest-journal state.

**Fix**: Give `bool_arg` the same `Option<Option<bool>>` contract `rumble.rs::bool_prop` has (fixed
under #1909): `None` = genuinely absent (index out of range), `Some(None)` = present but not a
literal (decline / bail), `Some(Some(v))` = present and literal. Update call sites to
`bool_arg(args, 1)?.unwrap_or(default)`.

## #2024 — SCR-D2-NEW-01 (MEDIUM, perf/DoS): rebuild_expression's restart-to-zero scan is O(n²)
**Location**: `crates/pex/src/decompile/lift.rs:316-346` (`rebuild_expression`)

After every successful single-consumer fold, the scan index resets to `0` and re-scans from the
start — O(n²) in statement count. No instruction-count cap exists anywhere in `crates/pex`/
`crates/scripting`. Benchmarked: 65535 instructions (wire-format max) → 1.255s, quadratic scaling.
Reachable synchronously from the live cell-loader VMAD-attach path via `translate_pex`.

**Fix**: Resume the scan at `i.saturating_sub(1)` instead of `0` after a fold (fold target is always
adjacent — `count_constant_id` only looks at `scope[i+1]`), dropping the pass to O(n). Preferred over
a hard cap since it also speeds up legitimate large scripts.

## #2025 — SCR-D4-NEW2-01 (MEDIUM): a single out-of-range literal anywhere in a .psc hard-fails the whole parse
**Location**: `crates/papyrus/src/lib.rs:20-30` (`parse_expr`), `:62-72` (`parse_script`)

`parse_script` collects `lex_errors` across the whole file and returns `Err` immediately if
non-empty — before the tolerant per-item-recovering parser runs. Post-#1908 (which correctly turned
silent-`0` on out-of-range literals into a real lex error), one bad literal anywhere now yields zero
AST for the entire file instead of just the offending item. Latent today (live cell-loader path uses
`.pex` decompilation, not `parse_script`), but undermines the stated per-item-recovery contract.

**Fix**: Route lex errors through the same per-item recovery path as parse errors — convert each
`LexError` into a synthetic placeholder token (or scope lex-failure to the containing line) so
`parse_script` drops only the offending item, not the whole file.

## #2026 — SCR-D7-NEW2-01 (MEDIUM): SCOL/PKIN outer REFR's VMAD replicated onto every synthetic child
**Location**: `byroredux/src/cell_loader/references/mod.rs:365-373` (synth_refs expansion loop),
consumed at `:463-469`, `:479-487`, `:784-792`; `byroredux/src/cell_loader/refr.rs:357-450`
(`expand_pkin_placements`/`expand_scol_placements`)

The outer REFR's own VMAD is a property of the single outer REFR, but the SCOL/PKIN expansion loop
threads it unchanged into `attach_script_for_refr` for every synthetic child — so a VMAD-scripted
SCOL/PKIN REFR's behavior (including `OnCellLoadEvent`) instantiates N times instead of once. Mirrors
the deliberate/correct texture-overlay sharing (#584) but VMAD attachment is behavioral, not visual.

**Fix**: Attach the outer REFR's own `script_instance` only to the first synthetic child, passing
`None` for the remaining N-1 — mirroring how `door_pos` already special-cases "first of N" elsewhere
in the same loop.

## Domain classification
- #2023: `byroredux-scripting` (crates/scripting)
- #2024: `byroredux-pex` (crates/pex)
- #2025: `byroredux-papyrus` (crates/papyrus)
- #2026: `byroredux` (binary, byroredux/src/cell_loader)
