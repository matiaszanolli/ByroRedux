# #4086 — INC-2026-09-09-01

TPLT absent-sentinel fallback only consults the two chain endpoints, so an intermediate template's authored `DNAM` Health is still lost

Filed 2026-09-09 by `/audit-publish` from `docs/audits/AUDIT_INCREMENTAL_2026-09-09.md`.
Snapshot of the issue **as filed** — GitHub is authoritative for current state
(`gh issue view 4086 --json state`).

---

- **Severity**: MEDIUM
- **Dimension**: ESM / record inheritance (`/audit-esm`), CHARAL population
- **Location**: [`crates/plugin/src/esm/records/actor_value_derive.rs:305-360`](../../crates/plugin/src/esm/records/actor_value_derive.rs)
- **Changed in**: `crates/plugin/src/esm/records/actor_value_derive.rs` (commit `0dcb5cf0`)
- **Status**: NEW. Dedup: `gh issue list --state all --limit 400` refreshed 2026-09-09 —
  #3481 (the sibling this partially fixes) is **CLOSED** by this very commit; no open or
  closed issue covers the multi-hop case. `docs/audits/` scan: no prior report mentions
  `baked_or_shell` (the symbol did not exist before this commit). Not sourced from a
  pre-2026-06-07 report, so the pre-cutoff caveat does not apply; the premise below was
  verified against the code at HEAD, not against issue absence.
- **Description**: `0dcb5cf0` correctly restored the "`0` = absent" fallback for the FO4
  baked `DNAM` pair and for an empty `PRPS`. But the fallback compares only the *shell*
  against the *terminal* record of the `TPLT` chain. `resolve_inherited_record` does not
  merge per field — it recurses to the deepest record that does not itself delegate and
  returns that one — so every intermediate `NPC_` in a chain of length ≥ 2 is invisible
  to the new fallback. If the terminal record leaves `calculated_health` at the absent
  sentinel while an intermediate authors it, the value the engine should use is skipped
  and the code falls all the way back to the shell, which is very likely `0` as well.
- **Evidence**: the fallback is strictly two-ended —
  ```rust
  // actor_value_derive.rs:317
  let props = if stats.actor_value_props.is_empty() {
      &shell.actor_value_props
  } else {
      &stats.actor_value_props
  };
  // actor_value_derive.rs:354
  fn baked_or_shell(resolved: u16, shell: u16) -> u16 {
      if resolved > 0 { resolved } else { shell }
  }
  ```
  and `stats` is the *terminal* of the walk, not a per-field merge:
  ```rust
  // crates/plugin/src/equip.rs:427-435
  if npc.template_flags & flag == 0 || npc.template_form_id == 0 { return npc; }
  if let Some(base) = index.npcs.get(&npc.template_form_id) {
      return resolve_inherited_record(base, actor_level, index, flag, depth + 1);
  }
  ```
  `TPLT_MAX_DEPTH = 6` (`equip.rs:331`), so up to four intermediate records can sit
  between the two endpoints the fallback inspects.
- **Impact**: the exact symptom #3481 was filed for — `stamp_actor_values` only inserts
  `ActorVitals` when the Health key is present, so an affected actor spawns undamageable
  and unkillable. **Reachability is narrow**: `resolve_inherited_record`'s own doc
  records that "vanilla template chains are flat (Lvl* template → base NPC, one hop)",
  and for a one-hop chain the two endpoints *are* the whole chain, so vanilla FO4 is
  fully covered by the fix as written. The gap is mod content that chains a per-faction
  wrapper (the case `TPLT_MAX_DEPTH`'s doc says it exists to accommodate). I could not
  measure the real-data count: doing so needs the `crates/plugin` `--ignored` ESM-parsing
  tests, which are barred by the OOM constraint (see §5).
- **Related**: #3480, #3481 (both CLOSED by `0dcb5cf0`); #2956, #3381, #3382, #3390.
- **Suggested Fix**: make the absent-sentinel resolution walk the chain instead of
  sampling its ends — e.g. a `resolve_inherited_field(npc, flag, index, |r| r.calculated_health)`
  that returns the first record from shell to terminal whose accessor is non-sentinel,
  reusing the same depth cap and `LVLN`/`LVLC` pick. That collapses the `PRPS` half and
  both `DNAM` halves into one rule and removes the two-endpoint assumption entirely.

---
