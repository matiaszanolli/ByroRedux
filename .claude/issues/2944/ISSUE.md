# CHAR-D3-05: Perks::set_rank neither rejects nor clamps out-of-range ranks though PerkRecord::num_ranks is parsed

- **Issue**: [#2944](https://github.com/matiaszanolli/ByroRedux/issues/2944)
- **Finding ID**: `CHAR-D3-05`
- **Labels**: `low,legacy-compat,bug`
- **Source report**: [`docs/audits/AUDIT_CHARACTER_2026-08-15.md`](../../../docs/audits/AUDIT_CHARACTER_2026-08-15.md)
- **Run**: `/audit-character` (first audit of this subsystem), 2026-08-15, HEAD `c25f61e6`

> Immutable snapshot of the issue *as filed* (TD10-001 / #1156). GitHub is
> authoritative for current state — query `gh issue view 2944 --json state`.

---

- **Severity**: LOW
- **Dimension**: Leveling & Progression
- **Game**: FO4 (the only game whose `PRKR` ranks reach the component today)
- **Location**: `crates/core/src/character/components.rs:57-69` (`set_rank`); writer at `byroredux/src/npc_spawn.rs:132-144`; the available max-rank data at `crates/plugin/src/esm/records/misc/magic.rs:241-244` (`PerkRecord::num_ranks`)
- **Status**: NEW
- **Source**: `docs/engine/charal-fo4-ruleset.md` § *Perk chart — COMPLETE* — "Each perk has **1–5 ranks** … a perk is takeable iff `SPECIAL ≥ Val ∧ character_level ≥ rank_gate ∧ owns(prev_rank)` … Rank counts range 2–5 … This is the *gating* half the `Perks` component validates against"
- **Description**: The capture document states that `Perks` is where the gating half is validated. It validates nothing: `set_rank` writes any `u8` unconditionally, and the spawn path copies the `PRKR` rank byte straight through with no bound. The checklist's required behavior — a rank beyond the perk's declared max is *rejected*, not silently clamped — is not implemented in either form. The data needed to enforce it is already parsed: `PerkRecord::num_ranks` is decoded from the PERK `DATA` sub-record for both the FO3/FNV and Skyrim layouts. Two smaller hygiene gaps ride along: `set_rank(id, 0)` inserts an entry indistinguishable from "not owned" (`rank()` returns `0` for both), and there is no removal API, so the `PERK` lifecycle documented in project memory `perk_system` ("automatically removed when the perk is removed") has no ECS expression.
- **Evidence**:
  ```rust
  // crates/core/src/character/components.rs:59-69 — no bound, no Result, no max
  pub fn set_rank(&mut self, perk_form_id: u32, rank: u8) {
      if let Some(p) = self.entries.iter_mut().find(|p| p.perk_form_id == perk_form_id) {
          p.rank = rank;
      } else { self.entries.push(PerkRank { perk_form_id, rank }); }
  }
  ```
  ```rust
  // crates/plugin/src/esm/records/misc/magic.rs:241-244 — the max the component never sees
  /// DATA num_ranks (count of multi-rank steps). 1 for most perks;
  /// 3–5 for Skyrim skill-tree perks with progressive ranks.
  pub num_ranks: u8,
  ```
- **Impact**: Currently bounded by the fact that authored `PRKR` ranks are well-formed and nothing reads `Perks` ranks yet (see CHAR-D3-01). It becomes real the moment a level-up path or the perk entry-point pipeline calls `set_rank`: an out-of-range rank would select entries that do not exist on the `PERK` record, with no error and no clamp.
- **Related**: CHAR-D3-01 (same component, no reader); project memory `perk_system` / `perk_entry_points`
- **Suggested Fix**: Give `set_rank` a fallible sibling that takes the perk's `num_ranks` and rejects `rank == 0 || rank > num_ranks`, and add a `remove` to mirror `PerkList::remove`. If the max cannot be plumbed at the call site, at least make rank `0` a documented no-op rather than a stored ghost entry.

## Completeness Checks
- [ ] **SIBLING**: The same pattern is checked in the other per-game ruleset builders (`fallout.rs` / `tes.rs` / `skyrim.rs`), not just the one cited
- [ ] **SOURCE**: Any changed constant cites the capture document line it comes from (`docs/engine/charal-*-ruleset.md`) — never a guessed value
- [ ] **CHARAL-BOUNDARY**: The per-game seam stays *data in the tables*; no consumer gains a branch on game identity
- [ ] **TESTS**: A regression test pins this specific fix (`cargo test -p byroredux-core character`)

---

*Filed by `/audit-publish` from [`docs/audits/AUDIT_CHARACTER_2026-08-15.md`](docs/audits/AUDIT_CHARACTER_2026-08-15.md) — `/audit-character`, 2026-08-15, HEAD `c25f61e6`. First audit of this subsystem. Verified CONFIRMED against current code at publish time.*
