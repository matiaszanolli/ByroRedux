# Creatures (CREA, FO3/FNV) derive no ActorValues — needs a sourced CREA DATA layout

- **Issue**: [#3390](https://github.com/matiaszanolli/ByroRedux/issues/3390)
- **Split from**: #3383 (the mis-feed half, fixed in `afff03c8`)
- **Labels**: `medium,character,esm-plugin,game:fnv,game:fo3,bug"

> Immutable snapshot of the issue **as filed** (TD10-001 / #1156). GitHub is authoritative
> for current state — query `gh issue view 3390 --json state`.

---

Split out from #3383, which fixed only the mis-feed half.

## What #3383 did
`CREA.CNAM` is no longer stored as `class_form_id` (`afff03c8`), so ~990 creature entities stop carrying an unrelated FormID in a field documented as a class. The module docstring on `derive_npc_actor_values` now states the gap explicitly.

## What remains
**The entire FO3/FNV bestiary derives an empty actor-value set.** `CREA` records are parsed into the same `NpcRecord` shape as `NPC_` (#442/#2567) and route through the identical spawn tail — `spawn_placement_root` calls `stamp_actor_values` *before* `prepare_runtime_state` branches on `npc.is_creature`. On FO3/FNV that lands in the `ClassAutoCalc` arm, whose class lookup can never hit, because creature stats are not class-derived at all.

Census (independent walk of both masters, resolving each `CREA.CNAM` against the plugin's own CLAS and IPDS FormID sets):

```
FNV: CREA=1578  CLAS=74  IPDS=60
     CNAM→CLAS    0      CNAM→IPDS  793     no CNAM  785
     control: NPC_=3816, CNAM→CLAS 3816 (100%)

FO3: CREA=533   CLAS=53  IPDS=41
     CNAM→CLAS    0      CNAM→IPDS  197     no CNAM  336
     control: NPC_=1647, CNAM→CLAS 1647 (100%)
```

## Consequences
- No `ActorValues`, therefore no `ActorVitals` (only inserted when derived pairs contain the Health AVIF).
- `resolve_actor_root` (`byroredux/src/combat.rs`) ends with `.filter(|a| world.get::<ActorVitals>(*a).is_some())`, so a melee ray landing on a creature's bone collider records `"first obstruction is not an actor"` and emits no `HitEvent` — creatures are untargetable and unkillable by the P2 melee slice.
- Every `GetActorValue` CTDA against a creature is a structural `0.0`, indistinguishable from a genuine zero.
- Nothing for the save-delta path to track.

## Why it is not fixed here
A creature arm must read `CREA`'s own `DATA` subrecord. That field layout is not present in this repo or in `/mnt/data/src/reference/` (`openmw` is Morrowind's incompatible `CREA`), and per [[feedback_no_guessing]] the offsets must come from the xEdit `wbDefinitionsFNV.pas` / fopdoc `CREA` definition rather than be inferred from the bytes.

## Acceptance
1. A sourced `CREA.DATA` layout, cited in the code comment as the other record schemas are.
2. A creature arm in `derive_npc_actor_values` (and a matching `NpcStatModel` variant, which does not exist today).
3. Wire-level tests in the style of `crates/plugin/src/esm/records/actor/tests.rs`.
4. A guard pinning that a placed FNV creature ends up with `ActorVitals`.

Found by /audit-character (CHAR-2026-08-27-D5-03) in the streaming-deep suite.
