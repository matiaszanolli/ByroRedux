# TD2-2026-10-25-01: `papyrus_provider::owner_form_id` is a copy of `entity_global_form_id` that omits the player special-case; eight more sites hand-spell `pool.resolve(id).map(|pair| pair.local.0)`

Labels: medium, scripting, tech-debt, bug
Issue: https://github.com/matiaszanolli/ByroRedux/issues/6351
(Immutable snapshot of the issue as filed; GitHub is authoritative for state.)

- **Severity**: MEDIUM (promotion: duplicated logic where the copy lacks the fix the original carries)
- **Dimension**: 2
- **Location**: `crates/scripting/src/papyrus_provider/execute.rs:24-31` (copy); original `crates/scripting/src/condition/eval.rs:220-229`; inline FormId→local spellings: `byroredux/src/systems/vats.rs:353` (new, 7eb4de261), `byroredux/src/systems/trap_hits.rs:95`, `byroredux/src/cell_loader/spawn.rs:488,569`, `byroredux/src/cell_loader/stream_snapshot.rs:291`, `crates/physics/src/sync.rs` (2)
- **Age**: `owner_form_id` 2f4b590412 (2026-10-19) — eight days after #5588 (badc… 2026-10-11) consolidated "byte-identical private fns in two crates plus an inline third copy" into `entity_global_form_id`. The #5588 guard (`entity_global_form_id_has_one_definition`, `systems/eat_sleep.rs:624`) greps three files for the literal `fn entity_global_form_id(`, so a re-copy under another name passes.
- **Effort**: small
- **What/why**: `entity_global_form_id` first maps the player entity to `PLAYER_REFERENCE` (0x14) because the player's pool entry is local 1 (its doc: "a new writer that forgot keyed the player under the wrong id"). `owner_form_id` is the same `FormIdComponent` → `FormIdPool::resolve` walk without that arm, so a player-owned script keys under local 1. The pool→local step is separately spelled 8 more times.
- **Fix**: (1) delete `owner_form_id`, call `crate::condition::entity_global_form_id` (it is `pub`, same crate). (2) Add `FormIdPool::local_of(&self, id: FormId) -> Option<u32>` in `crates/core/src/form_id.rs:196` next to `resolve`; route the 9 call sites (incl. `entity_global_form_id` itself) through it. (3) Widen the #5588 guard to scan `papyrus_provider/execute.rs` and `systems/vats.rs`, or key it on the `component.0).map(|pair| pair.local.0)` body rather than the fn name.

---
Source: `docs/audits/AUDIT_TECH_DEBT_2026-10-25.md` (HEAD c7463c04d), finding `TD2-2026-10-25-01`. Fix with: `/fix-issue <this issue>`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files / crates
- [ ] **TESTS**: A regression test or source-scan guard pins this specific fix (or the finding is doc-only and `_audit-validate.sh` / `cargo test` stays green)
