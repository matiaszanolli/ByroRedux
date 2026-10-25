# TD2-2026-10-25-13: `ItemKind::Weapon { 14 fields }` is built literally in ten test fixtures; the VATS work touched four of them for one new field

Labels: low, gameplay, inventory, test-gap, tech-debt, bug
Issue: https://github.com/matiaszanolli/ByroRedux/issues/6372
(Immutable snapshot of the issue as filed; GitHub is authoritative for state.)

- **Dimension**: 2 (test fixtures)
- **Location**: `byroredux/src/systems/use_weapon_package.rs:123-140`, `systems/projectiles.rs:~425`, `systems/vats.rs` (×3), `npc_spawn/loot_appearance.rs:~1855`, `npc_spawn/tests.rs`, `inventory.rs` (×2), `crates/plugin/src/esm/records/tests.rs`, `crates/plugin/tests/parse_real_esm.rs`. 7 literals added since 4a3e89d56; ff97cdd93 (#6350, WEAP DNAM → `ap_cost`/`vats_base_to_hit`/`skill_form`/`vats`) edited `loot_appearance.rs`, `npc_spawn/tests.rs`, `projectiles.rs`, `use_weapon_package.rs` for one-line field adds.
- **Effort**: small
- **Fix**: give the variant a named payload (`ItemKind::Weapon(WeaponData)` with `#[derive(Default)]`) or add `ItemKind::weapon_stub(damage, clip_size)` in `crates/plugin/src/esm/records/items.rs` under `#[cfg(any(test, feature = "test-fixtures"))]`; fixtures then spell only the fields they pin.


#### Dimension 3 — Stale Documentation (10)

---
Source: `docs/audits/AUDIT_TECH_DEBT_2026-10-25.md` (HEAD c7463c04d), finding `TD2-2026-10-25-13`. Fix with: `/fix-issue <this issue>`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files / crates
- [ ] **TESTS**: A regression test or source-scan guard pins this specific fix (or the finding is doc-only and `_audit-validate.sh` / `cargo test` stays green)
