# SAVE-01: Load path performs no referential-integrity re-validation

**Labels**: medium, ecs, bug
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/1844
**Source**: docs/audits/AUDIT_SAVE_2026-07-02.md

**Severity**: MEDIUM
**Dimension**: Validation Gates
**Data-Loss Class**: none (defense-in-depth)
**Location**: `crates/save/src/driver.rs:77-118` (`restore_world` / `restore_resources` / `apply_deltas`); `byroredux/src/save_io.rs:566-689` (`execute_pending_save_loads`)

## Description
`validate_world` + `validate_form_ids` run only on the SAVE path (`SaveCommand::execute`). `decode` validates the *container* (magic/version/schema/CRC) but the load drivers never re-run the referential gate on the decoded data. A save written by an OLDER engine (before a given validation rule existed), or a file hand-edited to keep a valid CRC, loads a referentially broken world unchecked — re-introducing the very slow-corruption tail the format's thesis exists to prevent. The thesis is symmetric (persist no inconsistent state ⇒ *ingest* no inconsistent state); only half is enforced.

## Evidence
`execute_pending_save_loads` goes `restore_resources` → `build_form_id_remap` → `apply_deltas` → `apply_player_pose` with no `validate_world` call anywhere on the drain. `restore_world` (`driver.rs:77`) likewise clears + repopulates with no post-load check.

## Impact
A corrupt-but-CRC-valid save (older-engine save, or manual edit) loads a broken world silently. Blast radius bounded to the loaded cell; not a write-side corruption, so no compounding tail on disk — but the in-memory world is inconsistent with no diagnostic.

## Related
The `/audit-save` SKILL's Dim-4 "validation runs on SAVE only" checklist item.

## Suggested Fix
After `apply_deltas` (live) and after `restore_world` (loose), run `validate_world` and log the issues at WARN (do not abort — a load can't fall back to the previous world cleanly, but a diagnostic is the minimum).

## Completeness Checks
- [ ] **SIBLING**: Same gate-symmetry check applied to both the live (`apply_deltas`) and loose (`restore_world`) load paths, not just one
- [ ] **TESTS**: A regression test pins that a referentially-broken-but-CRC-valid save trips a WARN (or the chosen diagnostic) on load
