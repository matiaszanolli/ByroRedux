# CHAR-D3-08: CharacterLevel and Perks are save-exempt as "ESM re-derivable" — true only while no progression runtime exists

- **Issue**: [#2947](https://github.com/matiaszanolli/ByroRedux/issues/2947)
- **Finding ID**: `CHAR-D3-08`
- **Labels**: `low,legacy-compat,tech-debt,bug`
- **Source report**: [`docs/audits/AUDIT_CHARACTER_2026-08-15.md`](../../../docs/audits/AUDIT_CHARACTER_2026-08-15.md)
- **Run**: `/audit-character` (first audit of this subsystem), 2026-08-15, HEAD `c25f61e6`

> Immutable snapshot of the issue *as filed* (TD10-001 / #1156). GitHub is
> authoritative for current state — query `gh issue view 2947 --json state`.

---

- **Severity**: LOW
- **Dimension**: Leveling & Progression
- **Game**: all
- **Location**: `byroredux/src/save_io/round_trip_tests.rs:735-758` (`REDERIVED_NOT_SAVED`); the stamped state at `byroredux/src/npc_spawn.rs:110-144`; `crates/core/src/character/components.rs:12-27`
- **Status**: NEW
- **Source**: `docs/engine/charal.md` §4.2 — "`pub struct CharacterLevel { level: u16, xp: f32 /* progress toward next */ }` … The per-game leveling strategy (§5 `LevelingModel`) advances it."
- **Description**: The save registry deliberately excludes `CharacterLevel`, `Background` and `Perks`, classified as "Re-derived from static ESM `NPC_` data. Most entries are write-once." That is currently accurate: `spawn_npc_entity` writes `level` from `NPC_` and `xp: 0`, and `Perks` verbatim from `PRKR`. But `CharacterLevel.xp` is defined by CHARAL as *progress toward the next level* — accumulated runtime state that is by construction **not** derivable from static ESM data — and `Perks` will hold level-up-granted ranks the same way. The exemption is therefore a snapshot of "no leveling runtime exists yet" recorded as if it were a property of the components, with no tripwire that fires when the first XP award lands.
- **Evidence**:
  ```rust
  // byroredux/src/save_io/round_trip_tests.rs:749-756
  const REDERIVED_NOT_SAVED: &[&str] = &[
      "FactionRanks", "CharacterLevel", "Background", "Perks", ...
  ```
  ```rust
  // byroredux/src/npc_spawn.rs:119-122 — xp is a literal 0 today, which is what makes the claim true
  CharacterLevel { level: npc.level.max(0) as u16, xp: 0 },
  ```
- **Impact**: Zero today. On the day leveling ships, every save silently discards accumulated XP and every perk rank earned after spawn, and the round-trip guard passes because the component is on the allow-list. Same class as #1834 / #1835 and `AUDIT_ECS_2026-08-13`'s `FactionReputation` finding, but for progression state rather than reputation.
- **Related**: #1834, #1835 (both CLOSED — the `ActorValues` / `PerkList` save gaps), ECS-2026-08-13-04; `/audit-save` owns the registry itself
- **Suggested Fix**: Narrow the exemption comment to say *why* it holds ("`xp` is always 0 and `Perks` is `PRKR`-verbatim until a leveling runtime exists") and add an assertion that `CharacterLevel::xp == 0` at snapshot time, so the exemption breaks loudly rather than silently the moment progression starts writing.

---

## Completeness Checks
- [ ] **SIBLING**: The same pattern is checked in the other per-game ruleset builders (`fallout.rs` / `tes.rs` / `skyrim.rs`), not just the one cited
- [ ] **SOURCE**: Any changed constant cites the capture document line it comes from (`docs/engine/charal-*-ruleset.md`) — never a guessed value
- [ ] **CHARAL-BOUNDARY**: The per-game seam stays *data in the tables*; no consumer gains a branch on game identity
- [ ] **TESTS**: A regression test pins this specific fix (`cargo test -p byroredux-core character`)

---

*Filed by `/audit-publish` from [`docs/audits/AUDIT_CHARACTER_2026-08-15.md`](docs/audits/AUDIT_CHARACTER_2026-08-15.md) — `/audit-character`, 2026-08-15, HEAD `c25f61e6`. First audit of this subsystem. Verified CONFIRMED against current code at publish time.*
