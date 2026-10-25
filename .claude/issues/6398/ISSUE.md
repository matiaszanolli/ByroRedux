# TD8-2026-10-25-05: pub helpers reachable only from tests are not `#[cfg(test)]`

Labels: low, tech-debt, bug
Issue: https://github.com/matiaszanolli/ByroRedux/issues/6398
(Immutable snapshot of the issue as filed; GitHub is authoritative for state.)

- **Severity**: LOW · **Effort**: trivial · **Status**: NEW (same class as closed #6192, whose `vdf.rs string_map` fix was exactly `#[cfg(test)]`)
- `crates/audio/src/lib.rs:843` `AudioWorld::is_script_music_active` (567bfe593, 2026-10-21; sole caller `byroredux/src/systems/script_requests.rs:462` inside `#[cfg(test)]`).
- `crates/plugin/src/esm/records/misc/equipment.rs:242` `first_part_name` ("the old stub's sanity-check field"; sole caller its own test :559; the stub it checked is gone).
- `crates/scripting/src/condition/catalog.rs:648` `ConditionFunction::oblivion_dialect` (73924db73, 10-24; callers only `condition/tests.rs`).
- `crates/scripting/src/crime_faction.rs:69` `CrimeFactionData::from_map` (4cfcee4b9, 10-24; doc "for a caller that stands in for a load order"; sole caller `byroredux/src/crime.rs:590` in tests).
- `crates/plugin/src/esm/records/reflection.rs:217` `weather_root_class` (callers: its unit test + `tests/parse_real_esm.rs`; plugin-crate public API, note only).
- `byroredux/src/interaction.rs:187` `bind_key` and `save_io/player.rs:68` `queued` already use `#[cfg_attr(not(test), allow(dead_code))]` — the tidy equivalent is a plain `#[cfg(test)]`.
- **Fix**: `#[cfg(test)]` on each (or delete `first_part_name`).

---
Source: `docs/audits/AUDIT_TECH_DEBT_2026-10-25.md` (HEAD c7463c04d), finding `TD8-2026-10-25-05`. Fix with: `/fix-issue <this issue>`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files / crates
- [ ] **TESTS**: A regression test or source-scan guard pins this specific fix (or the finding is doc-only and `_audit-validate.sh` / `cargo test` stays green)
