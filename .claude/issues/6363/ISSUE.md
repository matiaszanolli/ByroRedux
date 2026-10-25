# TD2-2026-10-25-04: "how many of base form X does this inventory hold" is hand-spelled at seven production sites (three added by the crime/jail work) plus four test copies

Labels: low, gameplay, inventory, tech-debt, bug
Issue: https://github.com/matiaszanolli/ByroRedux/issues/6363
(Immutable snapshot of the issue as filed; GitHub is authoritative for state.)

- **Dimension**: 2
- **Location**: `byroredux/src/barter.rs:309-318` (`count_of`), `crates/scripting/src/guards.rs:104` (`player_gold`) and `:115` (`can_pay_bounty`), `crates/scripting/src/jail.rs:196`, `crates/scripting/src/fragment/vm/natives/favor.rs:59`, `crates/scripting/src/fragment/vm/natives/reference.rs:155`, `crates/scripting/src/obscript_more/tail.rs:577`; test copies `obscript_effects.rs:2095`, `jail.rs:351`, `obscript_objects.rs:1365`, `reference_state.rs:1599`. `crates/core/src/ecs/components/inventory.rs` has `new/push/add/get/len` and no count accessor.
- **Effort**: trivial
- **Fix**: add `Inventory::count_of(&self, base_form_id: u32) -> u32` (saturating `u32` sum) in `inventory.rs:100-150`; one-liners at each site (`player_gold` widens to `i64`). Delete `barter::count_of` and the four test helpers.

---
Source: `docs/audits/AUDIT_TECH_DEBT_2026-10-25.md` (HEAD c7463c04d), finding `TD2-2026-10-25-04`. Fix with: `/fix-issue <this issue>`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files / crates
- [ ] **TESTS**: A regression test or source-scan guard pins this specific fix (or the finding is doc-only and `_audit-validate.sh` / `cargo test` stays green)
