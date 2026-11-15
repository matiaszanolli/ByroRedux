# #6965: TD2-2026-11-15-12: "First inventory row holding base form X" is hand-spelled five times next to `Inventory::count_of`

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/6965
- **Labels**: low,gameplay,inventory,tech-debt,bug
- **Source**: docs/audits/AUDIT_TECH_DEBT_2026-11-15.md

From the tech-debt audit `docs/audits/AUDIT_TECH_DEBT_2026-11-15.md` (HEAD 995ed089b).

- **Dimension**: 2 · **Severity**: LOW · **Effort**: trivial · **Labels**: `tech-debt` · **Existing**: none (same family as #6363 / `count_of`)
- **Age**: d467b074c (2026-10-30) for `queue_drop_by_form_id`; the others older
- **Location**: `byroredux/src/inventory/mod.rs:149` (`queue_consume_by_form_id`), `:163` (`queue_drop_by_form_id`), `:199` (`drain_script_weapon_equips`); `crates/scripting/src/obscript_effects/inventory.rs:157`; `byroredux/src/systems/projectiles.rs:200` (`inventory.items.iter().any(|s| s.base_form_id == *ammo && s.count > 0)`, which is `count_of(*ammo) > 0`)
- **Description**: four sites spell `items.iter().position(|s| s.base_form_id == X && s.count > 0)` and one spells the held test. `queue_consume_by_form_id` and `queue_drop_by_form_id` are also identical for 12 lines except the `InventoryAction` variant and the verb in two strings.
- **Suggested Fix**: `Inventory::first_row_of(&self, base_form_id: u32) -> Option<usize>` in `crates/core/src/ecs/components/inventory.rs` beside `count_of` (`:144`), `count_of(..) > 0` at `projectiles.rs`, and one private `queue_by_form_id(world, form_id, make: impl FnOnce(u32, u32) -> InventoryAction, verb)` for the consume/drop pair.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test (or an extended guard) pins this specific fix
