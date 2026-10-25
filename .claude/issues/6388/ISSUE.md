# TD6-2026-10-25-02: `owned.rs` still says guards, arrest and the other crimes "are not modelled"

Labels: low, gameplay, doc-rot, tech-debt, documentation
Issue: https://github.com/matiaszanolli/ByroRedux/issues/6388
(Immutable snapshot of the issue as filed; GitHub is authoritative for state.)

- **Severity**: LOW · **Dimension**: 6 (stub-status doc; Dim 3 overlap) · **Effort**: trivial · **Age**: line written e63fa457d 2026-10-23, made false by 99051a4eb (guards rush) / e161c45f3 (PayFine/GoToJail) / 39ffea9a8 (assault + murder), all 2026-10-24
- **Location**: `crates/core/src/ecs/components/owned.rs:19-22`
- **What**: "... a theft seen by an NPC adds its bounty (`byroredux/src/crime.rs`); guards, arrest and the other crimes are not modelled." Today `crime.rs` models assault and murder, `systems/guard.rs` + `make_guard_rush_system` rush a wanted player, and `scripting/src/jail.rs` implements PayFine/GoToJail/serve-sentence/evidence-chest (Oblivion; Skyrim/FO4 jail is explicitly "parsed and unused" per `docs/engine/charal-skyrim-ruleset.md:811`). The doc steers readers away from a working subsystem. `byroredux/src/crime.rs:40-44` carries the accurate "Not modelled yet" list (sneak/stealth roll, disposition drop, hostile spells, creatures-as-victims, Shivering Isles).
- **Fix**: replace the clause with a pointer: "bounty, witnesses, guards and Oblivion jail live in `byroredux/src/crime.rs` and `scripting/src/{jail,guards,crime_knowledge}.rs`; see that module's *Not modelled yet* list".


#### Dimension 7 — Magic Numbers (5)

---
Source: `docs/audits/AUDIT_TECH_DEBT_2026-10-25.md` (HEAD c7463c04d), finding `TD6-2026-10-25-02`. Fix with: `/fix-issue <this issue>`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files / crates
- [ ] **TESTS**: A regression test or source-scan guard pins this specific fix (or the finding is doc-only and `_audit-validate.sh` / `cargo test` stays green)
