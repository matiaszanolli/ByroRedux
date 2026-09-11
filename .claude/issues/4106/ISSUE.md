# CHAR-2026-09-11-D5-04: no test falsifies `derive_skyrim_actor_values`'s per-pool independence — the `Option<f32>` contract is asserted in prose only

GitHub: https://github.com/matiaszanolli/ByroRedux/issues/4106
Filed: 2026-09-11 from `docs/audits/AUDIT_CHARACTER_2026-09-11.md` (HEAD `8151cded`)

> Immutable snapshot of the issue as filed (TD10-001/#1156). GitHub is
> authoritative for current state: `gh issue view 4106 --json state`.

---

Reported by `/audit-character` — see `docs/audits/AUDIT_CHARACTER_2026-09-11.md` (HEAD `8151cded`).

- **Severity**: LOW
- **Dimension**: 5 — Population boundary
- **Game**: Skyrim
- **Location**: `crates/plugin/src/esm/records/actor_value_derive.rs:1061-1087` (`skyrim_health_skips_when_race_health_is_missing_or_invalid`), `:1030-1059` (`skyrim_pools_are_race_starts_plus_signed_npc_offsets`)
- **Source**: `crates/plugin/src/esm/records/actor/mod.rs:564-570` — the three fields are `Option<f32>` precisely so partial race data degrades one pool at a time; `actor_value_derive.rs:261-262` states the contract ("Each resolves independently through its authored `AVIF`").

## Description

the behaviour is correct (check 7), but nothing pins it. The all-pools test uses a race authoring all three; the skip test only ever removes **Health**, and both of its cases assert `is_empty()` — i.e. they exercise the *race-level* all-or-nothing gate, not per-pool independence. Neither direction the contract names is covered: (a) a race with `starting_magicka: None` but Health and Stamina present must still emit two pools; (b) a load order missing only the Magicka `AVIF` must still emit Health and Stamina. A regression to an all-or-nothing gate — the shape this code *used* to have — would keep all 26 tests green.

## Evidence

`grep -n "starting_magicka: None" actor_value_derive.rs` matches only inside `skyrim_race_follows_use_traits_while_offsets_follow_use_stats:757-766`, where that race belongs to the template and is deliberately **not** the one the assertions resolve to — so its `None` is never on the output path.

## Impact

test-coverage gap only; no shipped wrong value.

## Related

#3480 / `0dcb5cf0`; the parallel FO4 sentinel invariant *is* pinned in both directions (`fo4_absent_baked_stats_fall_back_to_the_shell`, `fo4_authored_template_baked_stats_still_win_over_the_shell`).

## Suggested Fix

two short tests beside the existing ones — one race with `starting_magicka: None` asserting exactly `[Health, Stamina]`, one index omitting the Magicka `AVIF` asserting the same — each verified to fail if the loop's `continue` is turned into a `return Vec::new()`.

## Completeness Checks

- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix