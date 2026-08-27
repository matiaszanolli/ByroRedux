# CHAR-2026-08-27-D5-02: The FO4 stored actor-value arm has the same un-resolved-template gap, and `charal.md`'s recorded open item mis-scopes it

- **Issue**: [#3382](https://github.com/matiaszanolli/ByroRedux/issues/3382)
- **Finding ID**: `CHAR-2026-08-27-D5-02`
- **Source report**: `docs/audits/AUDIT_CHARACTER_2026-08-27.md`
- **Audit suite preset**: streaming-deep (2026-08-27)
- **Labels**: `medium,character,esm-plugin,game:fo4,bug`

> Immutable snapshot of the issue **as filed** (TD10-001 / #1156). GitHub is authoritative
> for current state — query `gh issue view 3382 --json state`.

---

- **Severity**: MEDIUM
- **Dimension**: Population Boundary
- **Game**: fo4
- **Location**: `crates/plugin/src/esm/records/actor_value_derive.rs:167-176`
  (`derive_npc_actor_values`, the `NpcStatModel::Stored` arm) and `:208-224`
  (`derive_stored_actor_values`)
- **Status**: NEW
- **Source**: `docs/engine/charal-fo4-ruleset.md:519-529` — the explicit three-step
  inheritance chain: `RACE.PRPS` base → **`TPLT` + ACBS Template Flags ("Use Stats")** →
  `NPC_.PRPS` own overrides. Counts measured this session from `Fallout4.esm`.
- **Description**: identical structural gap to D5-01 on the other un-resolved arm.
  `derive_stored_actor_values(npc, index)` reads `npc.actor_value_props` (`PRPS`) and
  `npc.calculated_health` / `npc.calculated_action_points` (`DNAM`) straight off the shell
  record, with no `resolve_inherited_stats` call, while `stamp_character_components` on the
  same entity resolves `Use Stats` for `CharacterLevel` and `Background.class_form_id`.
  FO4 `template_flags` is parsed (`actor/mod.rs:919-927`, ACBS offset 14) and is used by
  the equip path, so the data is present and consumed elsewhere.

  There is a second, documentation-side half. `docs/engine/charal.md:568-570` records the
  remaining FO4 gap as *"the **`RACE`/template inheritance fallback** for NPCs that author
  no `PRPS` pairs of their own"*, and `:602-606` repeats it. That framing is falsified by
  the data: **0 of 3,015** vanilla FO4 `NPC_` records lack `PRPS`. The real gap is not a
  fallback for PRPS-less NPCs — it is a precedence question for the 1,222 shells that
  author a `PRPS` set differing from their `Use Stats` template's. A future reader working
  from `charal.md` would look for a population that does not exist and conclude the item is
  moot.
- **Evidence**: independent walk of `Fallout4.esm` (same script family; FO4 ACBS
  `template_flags` @ 14, `PRPS` as `(u32, f32)` pairs, `DNAM` as two leading `u16`):

  ```
  fo4: NPC_ total=3015  with TPLT=2289  UseStats=1972
  fo4: UseStats resolvable=1952
       PRPS pair-set differs from template = 1222
       DNAM (Calculated Health, Action Points) differs from template = 1201
       shell DNAM empty while template has one = 37
  fo4: NPC_ with zero PRPS = 0 / 3015
  fo4: NPC_ with DNAM Calculated Health == 0 = 330 / 3015
  ```

  The 330 with `calculated_health == 0` matter because `derive_stored_actor_values` pushes
  the Health pair only `if baked > 0`, so those actors get no Health AV, hence no
  `ActorVitals`, hence `combat.rs`'s `resolve_actor_root` filters them out entirely — 37 of
  them have a template that carries a real baked Health.
- **Impact**: 1,222 of 3,015 vanilla FO4 actors (40.5 %) are seeded with a SPECIAL /
  actor-value set that is not the one the template chain specifies, and 1,201 with the
  wrong baked Health/Action Points. FO4 `Health` and `ActionPoints` derived rows are
  `player_only()` by design (`fallout.rs:130-144`), so there is no `CharacterRuleset`
  fallback to mask a wrong `DNAM` — the stored value is the only value. Additionally the
  documented open item points at an empty population, so the gap reads as closed.
- **Related**: D5-01 (same root, other arm); #2956; `docs/engine/charal.md:568-570` and
  `:602-606` (the mis-scoped note).
- **Suggested Fix**: the same hoist as D5-01 fixes the code half. Separately, correct
  `charal.md` §8 item 3 and §9 to describe the gap as *template precedence for NPCs whose
  own `PRPS`/`DNAM` disagree with their `Use Stats` source* rather than a fallback for
  NPCs authoring no `PRPS`, and record the 0/3,015 measurement so the framing cannot drift
  back.

---
## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test pins this specific fix

---
_Filed by `/audit-publish` from `docs/audits/AUDIT_CHARACTER_2026-08-27.md` (audit-suite preset: streaming-deep). Finding ID: `CHAR-2026-08-27-D5-02`._
