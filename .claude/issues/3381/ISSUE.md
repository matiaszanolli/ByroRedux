# CHAR-2026-08-27-D5-01: The Skyrim actor-value arm reads the shell `NPC_`, never the `TPLT` source — and contradicts `Background` on the same entity

- **Issue**: [#3381](https://github.com/matiaszanolli/ByroRedux/issues/3381)
- **Finding ID**: `CHAR-2026-08-27-D5-01`
- **Source report**: `docs/audits/AUDIT_CHARACTER_2026-08-27.md`
- **Audit suite preset**: streaming-deep (2026-08-27)
- **Labels**: `medium,character,esm-plugin,game:skyrim,bug`

> Immutable snapshot of the issue **as filed** (TD10-001 / #1156). GitHub is authoritative
> for current state — query `gh issue view 3381 --json state`.

---

- **Severity**: MEDIUM
- **Dimension**: Population Boundary
- **Game**: skyrim
- **Location**: `crates/plugin/src/esm/records/actor_value_derive.rs:167-176`
  (`derive_npc_actor_values`) and `:180-201` (`derive_skyrim_actor_values`), against
  `byroredux/src/npc_spawn.rs:150-171` (`stamp_character_components`)
- **Status**: NEW
- **Source**: `docs/engine/charal-fo4-ruleset.md:524-526` — *"**`TPLT` + ACBS Template
  Flags** — if 'Use Stats' is set, inherit SPECIAL / level / etc. from the template
  `NPC_`/`LVLN`"* — the inheritance chain the FO3/FNV arm already implements via
  `crates/plugin/src/equip.rs:257-278` (`resolve_inherited_stats` /
  `resolve_inherited_traits`, #2956). `docs/engine/charal-skyrim-ruleset.md:603-605`
  supplies the composition shape (*"race base … + per-NPC fixed adjustment"*) — i.e. both
  operands of the Skyrim formula are exactly the two fields the template flags govern.
  Population counts measured this session from `Skyrim.esm`.
- **Description**: `derive_npc_actor_values` is a four-way match on `NpcStatModel`. The
  `ClassAutoCalc` arm resolves `TPLT` first:

  ```rust
  NpcStatModel::ClassAutoCalc { health } => {
      let stats_npc =
          crate::equip::resolve_inherited_stats(npc, effective_actor_level(npc), index);
      derive_autocalc_actor_values(stats_npc, index, index.character_rules, health)
  }
  ```

  The `RaceBaseOffsets` (Skyrim) arm does not:

  ```rust
  NpcStatModel::RaceBaseOffsets => derive_skyrim_actor_values(npc, index),
  ```

  and `derive_skyrim_actor_values` then reads **both** operands off the shell:
  `index.races.get(&npc.race_form_id)` for the race base, and
  `npc.health_offset` / `npc.magicka_offset` / `npc.stamina_offset` for the per-NPC
  adjustment. `RNAM` is `Use Traits` data; the three ACBS offsets are `Use Stats` data.
  Both flags are parsed for Skyrim (`actor/mod.rs:939-949`, `template_flags` at ACBS
  offset 18) and both are honoured elsewhere in the same spawn tail.

  The result is not just "possibly the wrong number" — it is an internal contradiction.
  `stamp_character_components`, twenty lines away in `npc_spawn.rs`, writes:

  ```rust
  let traits_npc = resolve_inherited_traits(npc, shell_level, index);
  …
  Background { race_form_id: traits_npc.race_form_id, … }
  ```

  So on 887 Skyrim actors the entity's `Background` declares one race while its
  `ActorValues` Health/Magicka/Stamina were computed from a different one. A third site,
  `build_npc_equip_state` (`npc_spawn.rs:788`), uses the shell's `npc.race_form_id` again
  for the `RACE.WNAM` default skin — three sites, two conventions, no test pinning either.
- **Evidence**: independent walk of `Skyrim.esm` (`/tmp/audit/character/tplt2.py`,
  TES5 24-byte record headers, zlib-inflating compressed records, ACBS offsets read at the
  same byte positions the Rust parser uses; `RACE.DATA` H/M/S read as `f32` @ 36/40/44,
  matching `actor/mod.rs:1324-1335`):

  ```
  skyrim: NPC_ total=5118  with TPLT=3651  UseStats=3182  UseTraits=2053
  skyrim: UseTraits resolvable=1874, own RNAM differs from template=887,
          and their RACE.DATA (H,M,S) triple differs=534
  skyrim: UseStats  resolvable=2970, own (H,M,S) offsets differ from template=671
  skyrim: FINAL computed (H,M,S) differs from what the code produces = 875 / 5118
  ```

  The TPLT walk used in that script mirrors `resolve_inherited_record`'s own contract
  (flag-gated chain, depth cap 6, `LVLN` highest-eligible pick).
- **Impact**: 875 of 5,118 vanilla Skyrim actors (17.1 %) are seeded with the wrong
  Health / Magicka / Stamina. Because Health is what `stamp_actor_values` keys
  `ActorVitals` on, this is also the number combat, drowning damage
  (`systems/water.rs`), and every `GetActorValue` CTDA read against. Silent — no log line,
  no failing test, and the two contradicting components both look plausible in isolation.
  The affected population is exactly the templated `Enc*` encounter actors, i.e. the ones
  the player actually fights.
- **Related**: #2956 (CLOSED — established the rule for the FO3/FNV arm only); #3171
  (CLOSED — same defect class, `ActorValues` and `CharacterLevel` derived from different
  source records, 30 actors); CHAR-D5-02 of the 2026-08-15 sweep (the FO3/FNV original of
  this finding).
- **Suggested Fix**: hoist the template resolution above the match in
  `derive_npc_actor_values` — resolve `stats_npc` (`Use Stats`) and `traits_npc`
  (`Use Traits`) once and pass both down, so `derive_skyrim_actor_values` takes its race
  from the traits source and its three offsets from the stats source. That also removes the
  duplicated chain walk `stamp_character_components` performs separately. Pin it with a
  test asserting that `Background.race_form_id` and the race whose `RACE.DATA` fed
  `ActorValues` are the same FormID for a templated actor — the invariant that is currently
  unrepresented.

---
## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test pins this specific fix

---
_Filed by `/audit-publish` from `docs/audits/AUDIT_CHARACTER_2026-08-27.md` (audit-suite preset: streaming-deep). Finding ID: `CHAR-2026-08-27-D5-01`._
