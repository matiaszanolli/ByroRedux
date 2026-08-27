# CHAR-2026-08-27-D5-03: Every FO3/FNV creature receives zero `ActorValues` — `CREA.CNAM` is not a class, and there is no creature arm

- **Issue**: [#3383](https://github.com/matiaszanolli/ByroRedux/issues/3383)
- **Finding ID**: `CHAR-2026-08-27-D5-03`
- **Source report**: `docs/audits/AUDIT_CHARACTER_2026-08-27.md`
- **Audit suite preset**: streaming-deep (2026-08-27)
- **Labels**: `medium,character,esm-plugin,game:fnv,game:fo3,bug`

> Immutable snapshot of the issue **as filed** (TD10-001 / #1156). GitHub is authoritative
> for current state — query `gh issue view 3383 --json state`.

---

- **Severity**: MEDIUM
- **Dimension**: Population Boundary
- **Game**: fnv, fo3
- **Location**: `crates/plugin/src/esm/records/actor_value_derive.rs:167-176`
  (`derive_npc_actor_values` — no creature arm) and `:230-236`
  (`derive_autocalc_actor_values`'s `index.classes.get(&npc.class_form_id)` gate); fed by
  `crates/plugin/src/esm/records/actor/mod.rs:812-815` (the shared `CNAM` arm) and
  `crates/plugin/src/esm/records/dispatch_actor.rs:42-53` (`CREA` parsed by `parse_npc`);
  consumed at `byroredux/src/npc_spawn.rs:90-114` (`stamp_actor_values`) and
  `byroredux/src/npc_spawn/resumable.rs:328` (`spawn_placement_root` runs **before** the
  `is_creature` early return at `:347`)
- **Status**: NEW
- **Source**: measured from `FalloutNV.esm` and `Fallout3.esm` this session — the CLAS/IPDS
  resolution census below is itself the source; no external claim about `CREA.CNAM`'s
  semantics is needed, because the decisive fact is that it resolves to a `CLAS` record
  zero times and to an `IPDS` record 990 times.
- **Description**: `CREA` records are parsed into the same `NpcRecord` shape as `NPC_`
  (`dispatch_actor.rs`, deliberately — #442/#2567), and placed creatures route through the
  identical spawn tail: `spawn_placement_root` calls `stamp_faction_ranks`,
  `stamp_actor_values`, `stamp_character_components` *before* `prepare_runtime_state`
  branches on `npc.is_creature`. So a creature is stamped with whatever
  `derive_npc_actor_values` returns.

  On FO3/FNV that lands in the `ClassAutoCalc` arm, whose first statement is:

  ```rust
  let Some(class) = index.classes.get(&npc.class_form_id) else {
      return Vec::new();
  };
  ```

  `class_form_id` is populated by the shared `CNAM` arm, which is correct for `NPC_` and
  wrong for `CREA`: on `CREA` that FormID names a different record type entirely. The
  lookup therefore misses for **100 %** of creatures, `derive_npc_actor_values` returns an
  empty `Vec`, `stamp_actor_values` early-returns on `pairs.is_empty()`, and the creature
  gets neither `ActorValues` nor `ActorVitals`.

  There is no creature arm anywhere in the dispatch, and `NpcStatModel` has no creature
  variant. The module docstring's list of empty-result cases
  (`actor_value_derive.rs:159-162`) names *"an FNV NPC whose class wasn't parsed"* — which
  reads as a rare parse failure, not as "the entire bestiary, by construction".
- **Evidence**: independent walk of both masters, resolving each `CREA`'s `CNAM` against
  the plugin's own `CLAS` and `IPDS` FormID sets:

  ```
  FNV: CREA=1578  CLAS records=74  IPDS=60
       CREA CNAM resolves to CLAS:    0
       CREA CNAM resolves to IPDS:  793
       CREA with no CNAM:           785
       NPC_=3816, NPC_ CNAM resolves to CLAS: 3816   (100 %)

  FO3: CREA=533   CLAS records=53  IPDS=41
       CREA CNAM resolves to CLAS:    0
       CREA CNAM resolves to IPDS:  197
       CREA with no CNAM:           336
       NPC_=1647, NPC_ CNAM resolves to CLAS: 1647   (100 %)
  ```

  The `NPC_` rows are the control: the field is unambiguously a class FormID there and
  unambiguously not one on `CREA`.

  Downstream consequence, traced in code: `combat.rs:305-315` (`resolve_actor_root`)
  ends with `.filter(|actor| world.get::<ActorVitals>(*actor).is_some())`, and
  `stamp_actor_values` only inserts `ActorVitals` when the derived pairs contain the Health
  AVIF. A melee ray that lands on a creature's bone collider therefore records
  `"first obstruction is not an actor"` and emits no `HitEvent`.
- **Impact**: all 1,578 FNV and 533 FO3 `CREA` base records — the entire bestiary
  (deathclaws, geckos, super mutants, robots, radroaches) — spawn with no actor values.
  Concretely: untargetable and unkillable by the P2 melee slice; every `GetActorValue` CTDA
  against a creature is a structural `0.0`, indistinguishable from a genuine zero; no
  `ActorVitals` for the save-delta path to track. A secondary effect:
  `stamp_character_components` still writes `Background { class_form_id }` for creatures,
  so 990 creature entities carry an `IPDS` FormID in a field the component documents as a
  class.
- **Related**: #3004 (CLOSED — the `NPC_` half of "actors are not damageable"; creatures
  were never in its scope); #2567 (the commit that routed creatures into this spawn tail);
  #3305 (OPEN, renderer-side creature issue — unrelated mechanism).
- **Suggested Fix**: two independent steps. (1) Short-term, stop the mis-feed: do not
  populate `class_form_id` from `CNAM` when the record came from the `CREA` group — the
  one site that knows which group it read is `dispatch_actor.rs`, the same place
  `is_creature` is set. (2) Add a creature arm to `derive_npc_actor_values` sourced from
  `CREA`'s own `DATA` subrecord, whose field layout must be taken from the xEdit / fopdoc
  `CREA` definition rather than inferred — until that decode exists, the honest interim is
  an explicit, documented "creatures are unpopulated" note in the module docstring so the
  gap stops reading as a rare parse failure.

---
## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test pins this specific fix

---
_Filed by `/audit-publish` from `docs/audits/AUDIT_CHARACTER_2026-08-27.md` (audit-suite preset: streaming-deep). Finding ID: `CHAR-2026-08-27-D5-03`._
