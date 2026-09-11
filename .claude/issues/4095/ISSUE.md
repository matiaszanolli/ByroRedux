# CHAR-2026-09-11-D1-03: `probe_combat_fixture` re-derives actor level as `npc.level.max(1)`, the exact divergence #3081/#3171 rejected twice

GitHub: https://github.com/matiaszanolli/ByroRedux/issues/4095
Filed: 2026-09-11 from `docs/audits/AUDIT_CHARACTER_2026-09-11.md` (HEAD `8151cded`)

> Immutable snapshot of the issue as filed (TD10-001/#1156). GitHub is
> authoritative for current state: `gh issue view 4095 --json state`.

---

Reported by `/audit-character` — see `docs/audits/AUDIT_CHARACTER_2026-09-11.md` (HEAD `8151cded`).

- **Severity**: LOW
- **Dimension**: Ruleset Seam & CHARAL Doctrine (the #3171 regression guard, dev-tool side)
- **Game**: FO3 / FNV
- **Location**: `crates/plugin/examples/probe_combat_fixture.rs:64`
- **Source**: `crates/plugin/src/esm/records/actor/mod.rs:61-102` — the
  `effective_actor_level` docstring, which states the rule and names `.max(1)` as
  the rejected form.

## Description

Not a fourth *definition* of `effective_actor_level` — the
  function still has exactly one — but it is an inline re-derivation of the same
  decision with the rejected clamp, in a tool whose entire job is to pick combat
  fixtures. It reads `npc.level` raw, so it never consults `ACBS_PC_LEVEL_MULT`
  (`0x0080`): for the 268 FNV records where that bit is set, `level` is a
  fixed-point multiplier (round steps up to 2000), not a level. That value is
  then fed straight into `resolve_inherited_inventory(npc, actor_level, …)` and
  `expand_leveled_form_id(…, actor_level, …)` at lines 65 and 92 — the two
  level-gated filters the docstring explicitly warns about ("feeding that raw
  into a leveled-list filter makes every entry eligible, so the actor always
  draws the top tier").

## Impact

Dev-tool only (nothing in `examples/` ships), so no gameplay impact
  — but the probe's *output* is what a human uses to choose the vertical-slice
  combat fixture, and for PC-level-mult creatures it reports the top tier of
  every leveled list. It is also the one remaining place a future reader can copy
  the wrong rule from.

## Related

#3081, #3171, #2955.

## Suggested Fix

Replace with
  `byroredux_plugin::esm::records::effective_actor_level(npc)` (already `pub`
  and re-exported at `records/mod.rs:48`) and delete the local clamp.

## Completeness Checks

- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix