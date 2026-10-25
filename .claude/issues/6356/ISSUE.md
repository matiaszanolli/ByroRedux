# TD1-2026-10-25-02: `byroredux/src/inventory.rs` 1956 prod LOC (+93 in 7 commits since 10-21, 44 lines from the line) — split before the next crime/loot feature crosses it

Labels: low, gameplay, inventory, tech-debt, bug
Issue: https://github.com/matiaszanolli/ByroRedux/issues/6356
(Immutable snapshot of the issue as filed; GitHub is authoritative for state.)

- **Severity**: LOW · **Dimension**: 1 · **Effort**: medium
- **Age**: 7 commits since 4a3e89d56: `fcd1cce4a`, `4cfcee4b9` (crime faction), `e161c45f3` (PayFine/GoToJail), `e63fa457d` (theft bounty), `e9b7bcead` (radiation gauge), `2682aa969` (#6229), `3e4bfab51`. Flat in the window before that (1863). 4659 lines total: ~2680 are inline tests, so any edit also pays the test-module diff.
- **Location**: `byroredux/src/inventory.rs` (1980 prod; `mod tests` L1981-4659)
- **Structure read**: three jobs. (a) catalog/templates/vitals: `InventoryCatalog`, `PlayerInventoryTemplate`, `PlayerVitals`, `install_catalog` L601, `build_player_character_template` L462, `build_player_template*` L742-816, `attach_to_player` L871 (L22-960). (b) loot, ownership and pickup: `is_loot_source` L962, `stamp_cell_ownership` L996, `theft_owner` L1047, `transfer_loot` L1122 (~190 lines), `container_loot_system` L1316, `PickedUp`/`PlacedItemCount`, `pickup_loot`, `take_placed_item` L1416 (L962-1500). (c) UI-driven inventory mutation: `snapshot` L1505, `consume_item` L1577 (~137), `apply_action` L1714, `toggle_equip` L1771, `reconcile_*` L1894-1980.
- **Split axis**: `inventory/{mod.rs (add_item, queue_*, drain_pending, re-exports), catalog.rs (a), loot.rs (b, the crime/ownership growth point), actions.rs (c)}`, tests as siblings per job.
- **Hazard**: `byroredux/src/npc_spawn/tests.rs:3373` does `include_str!("../inventory.rs")` and counts call names at :3401 — it must read a `concat!` of the new files or the census goes vacuous (feedback_file_split_include_str). Hub-churn caveat: after the split `git log -- inventory.rs` shows only the hub, use `inventory/` pathspec.

---
Source: `docs/audits/AUDIT_TECH_DEBT_2026-10-25.md` (HEAD c7463c04d), finding `TD1-2026-10-25-02`. Fix with: `/fix-issue <this issue>`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files / crates
- [ ] **TESTS**: A regression test or source-scan guard pins this specific fix (or the finding is doc-only and `_audit-validate.sh` / `cargo test` stays green)
