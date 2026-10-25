# TD2-2026-10-25-03: the player-entity read has an accessor in scripting (#5813) but not in the binary; 66 production sites hand-spell it

Labels: low, gameplay, tech-debt, bug
Issue: https://github.com/matiaszanolli/ByroRedux/issues/6362
(Immutable snapshot of the issue as filed; GitHub is authoritative for state.)

- **Dimension**: 2
- **Location**: `try_resource::<PlayerEntity>().and_then(|p| p.0)` at 66 production sites across 34 files in `byroredux/src` (worst: `inventory.rs` 10, `combat.rs` 6, `systems/character.rs` 6, `systems/npc_dialogue.rs` 4, `commands/gameplay.rs` 3, `commands/view.rs` 3, `crime.rs` 2, `systems/vats.rs` 3); three local one-off wrappers `barter.rs:218 player()`, `crates/scripting/src/jail.rs:184 player_of()` (reads the *other* resource), `crates/scripting/src/obscript_more.rs:163 player()`. 9 of the 66 landed since 4a3e89d56.
- **Age**: #5813 (closed) added `PapyrusPlayerEntity::get` (`crates/scripting/src/papyrus_demo/mod.rs:83`) for "62 production sites"; its doc says the bin's `PlayerEntity` is a second, NOT-synchronised resource that "in FlyCam differs". The sweep stopped at the crate boundary.
- **Effort**: small (mechanical)
- **Fix**: `impl PlayerEntity { pub fn get(world: &World) -> Option<EntityId> }` at `byroredux/src/systems/character.rs:44`; sed the 66 sites; delete `barter::player`. Add a `workspace_hygiene_tests` scan for `PlayerEntity>().and_then` like the Papyrus one. Bonus: the two-resource split then has exactly two accessors, which is where a future FlyCam divergence must be reasoned about once (today `crime.rs` reads `PlayerEntity` while `jail.rs`/`guards.rs` read `PapyrusPlayerEntity` for the same player in one crime→jail flow).

---
Source: `docs/audits/AUDIT_TECH_DEBT_2026-10-25.md` (HEAD c7463c04d), finding `TD2-2026-10-25-03`. Fix with: `/fix-issue <this issue>`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files / crates
- [ ] **TESTS**: A regression test or source-scan guard pins this specific fix (or the finding is doc-only and `_audit-validate.sh` / `cargo test` stays green)
