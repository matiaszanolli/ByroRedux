# TD3-2026-10-25-02: Recurrence of the #6268/#6064/#5809 class: eight doc blocks spliced onto the wrong item in the last 4 days (the #5809/#6064/#6268 class recurred; three landed after the #6268 sweep)

Labels: low, doc-rot, tech-debt, documentation
Issue: https://github.com/matiaszanolli/ByroRedux/issues/6373
(Immutable snapshot of the issue as filed; GitHub is authoritative for state.)

- **Severity**: LOW (contract sits on the wrong item; the new item is undocumented). Severity table has no row for this class.
- **Dimension**: 3
- **Effort**: small (8 doc moves)
- **Detector**: scanned every added run of lines in `git diff -U4 4a3e89d56..HEAD -- '*.rs'` that starts directly under a context `///` line and contains an item.
- **Sites** (each: the doc that belongs to item B now heads item A, and B has none):

| # | Doc belongs to (now undocumented) | Doc now sits on | Introduced |
|---|---|---|---|
| 1 | `scene.rs:200` `mark_scene_actor_bindings_dirty` ("Notify the alias resolver that the live candidate set changed", `:190`) | `scene_form_ids` (`:193`), whose own doc is glued under it | `43772fff8` 2026-10-22 |
| 2 | `obscript_effects.rs:179` `form_arg` ("The form a reference argument resolved to; `None` for an absent or null one", `:160`) | `adjust_standing` (`:164`) | `0539e07f8` 2026-10-21 |
| 3 | `context/post_passes.rs:83` `volumetric_sun` (the `portal_sun_radiance` / `cloud_transmittance` doc, `~:45-66`) | `caustic_sun_transmittance` (`:75`) | `7180d3109` (#6245) 2026-10-21 |
| 4 | `streaming/pre_parse.rs` `pre_parse_cell` (~`:697`; doc "Pre-parse one exterior cell's unique, uncached static NIFs...", `:652-664`) | `bake_absorbed_refs` (`:677`) | `88149ae26` (#6097) 2026-10-24 |
| 5 | `cell_loader/load_order.rs:368` `plugin_for_form_id` (the "ESL anywhere but last shifts every later regular plugin's position" doc, `~:352-362`) | `load_order_position_of` (`:363`) | `9634aa362` (#6088) 2026-10-25 |
| 6 | `load_order.rs:120` `all_regular` ("Build an order in which every plugin is a regular master...") | `has_slot_table` (`:115`) | `9634aa362` 2026-10-25 |
| 7 | `cell_loader/references/synth_child.rs:357` `spawn_synth_child` ("Dispatch one synthetic child placement... Split verbatim out of `load_references` (#2058)", `:304`) | `spawn_logical_synth_child` | `c82fcc407` (#6160 part) 2026-10-22 |
| 8 | `plugin/.../misc/dialogue.rs:1459` `parse_info_tclf_and_name_are_not_dropped` (the "#3614 TCLF/NAME" doc, `:1414`) | `parse_info_splits_begin_and_end_result_scripts_at_next` (`:1423`) | `6db47b0e4` 2026-10-22 |
- **Impact**: rustdoc and readers get the wrong contract on production items (sites 1-5, 7) and lose the contract for the right one. Sites 4, 5 and 6 landed 10-24/25, after `82f5d8919` (#6268, 10-23), because that sweep and the guard cover only the renderer crate.
- **Fix**: Move each doc block onto its item (rows 1, 3, 4 and 5 are pairs: the item that keeps its own doc is correct, the first-listed doc must move to the undocumented item). Then generalise `no_item_is_spliced_into_another_items_doc_block` (renderer-only today) into a shared helper in `crates/core/src/source_scan.rs` and run it over `byroredux`, `scripting`, `plugin` from `byroredux/src/workspace_hygiene_tests.rs`. Cheap heuristic: a `///` block whose first backticked identifier is an item name defined within 40 lines below but is not the item that follows.

---
Source: `docs/audits/AUDIT_TECH_DEBT_2026-10-25.md` (HEAD c7463c04d), finding `TD3-2026-10-25-02`. Fix with: `/fix-issue <this issue>`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files / crates
- [ ] **TESTS**: A regression test or source-scan guard pins this specific fix (or the finding is doc-only and `_audit-validate.sh` / `cargo test` stays green)
