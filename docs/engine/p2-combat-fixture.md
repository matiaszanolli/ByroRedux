# P2 Combat Fixture

**Status:** fixture frozen 2026-08-10; combat core passing 2026-08-16

This fixture is the single real-data target for the playable vertical slice's
first combat loop. It deliberately avoids leveled-actor placement and weapon
family breadth so implementation failures stay attributable to engine state,
not content selection.

The combat-core checkpoint now passes via
[`p2-melee-core.sh`](../smoke-tests/p2-melee-core.sh): vanilla Health derives
to 50, the player ray resolves a skeleton bone back to this placement root,
seven bound unarmed attacks emit seven canonical `HitEvent`s, and zero Health
produces one `Dead` transition plus an 18-body ragdoll. The full P2 closure
below remains open for authored response animation/sound, corpse loot, and
save/reload continuity.

## Frozen actor and weapon family

| Field | Value |
|---|---|
| Plugin | `Skyrim.esm` |
| Interior CELL | `BleakFallsBarrow01` (`000371DE`) |
| Placed reference | `000380B4` |
| Base NPC | `000E9895` (`EncDraugr01AmbushMelee2HHeadM06`) |
| Authored position (Z-up) | `(9015.6, -4724.6, -2053.7)` |
| Level | `1` |
| Factions | `00000013 CreatureFaction`, `0002430D DraugrFaction` |
| Default outfit | `0001F85E` |
| Death-item list | `0003AD7F` |
| Weapon family | Draugr two-handed melee |
| Concrete weapon leaves | `0001CB64` Draugr Battleaxe (18), `000236A5` Draugr Greatsword (17) |

The placement points directly at an `NPC_`, not an `LVLN`. The current cell
loader therefore spawns it through the production Skyrim pre-baked-FaceGen
path. Both concrete weapon leaves use the same two-handed animation family;
P2 may choose one deterministically without broadening its animation or timing
contract. The smoke will position the player near this actor and explicitly
start the encounter, so the initial ambush presentation is not a prerequisite
for the combat gate.

The data contract can be rechecked without Vulkan or archive loading:

```bash
cargo run -p byroredux-plugin --example probe_combat_fixture -- \
  "$BYROREDUX_SKYRIM_DATA/Skyrim.esm" BleakFallsBarrow01
```

The probe must report CELL `000371DE`, direct NPC reference `000380B4`, both
factions, death item `0003AD7F`, and the two concrete weapon leaves above.

A release-build live preflight on 2026-08-10 also resolved the actor by its
editor ID after the full cell load. Its root landed at renderer Y-up position
`(9015.58, -2053.70, 4724.62)`, matching the authored transform. Inspection
showed ten inventory rows, including four copies each of the Battleaxe and
Greatsword leaves, while all 32 `EquipmentSlots` occupants were empty. That
confirms the production spawn path and makes the current leveled expansion /
weapon-selection ambiguity observable rather than hypothetical.

## Pre-implementation runtime surface (2026-08-10 trace)

| Surface | Ready now | P2 gap exposed by the fixture |
|---|---|---|
| Actor identity | Direct `NPC_` placement creates a root with `Name`, `FactionRanks`, `CharacterLevel`, and `Background`; cell finalization adds the placed reference's `FormIdComponent` and `SceneAliasCandidate`. | No hostile-target or combat-state consumer exists. Current PACK behavior is ambient only: sandbox, wander, travel, follow, escort, guard, or patrol. |
| Health | `ActorValues` has composed values plus `apply_damage`/restore APIs. | `derive_npc_actor_values` returns an empty set for Skyrim, so this actor currently receives no `ActorValues`. Skyrim ACBS health/stat offsets need a typed parse and a deliberate base-value policy before damage can be real. |
| Inventory/equipment | Outfit and inherited inventory resolve through leveled lists into concrete `Inventory` rows; the generic armor path can occupy `EquipmentSlots` and attach skinned meshes. The live fixture contains both expected weapon leaves. | The fixture's live root has empty `EquipmentSlots`, and each weapon leaf is duplicated four times by current expansion. Weapons have no hand slot, chosen state, mesh attachment, or attack timing. P2 must freeze one deterministic selection rule and remove the multi-pick duplication from the equipped result. |
| Animation | The Skyrim skeleton and skin are attached, ragdoll bones are keyframed, and the actor root receives a `HavokAnimationTarget`. | General Skyrim NPC idle/locomotion/attack/hit/death playback is absent. The installed HKX catalog is intentionally limited to MQ101 cart idles; Skyrim actors do not receive the KF idle pool. |
| Physics/hit ownership | `PhysicsWorld::cast_ray` returns the hit rigid body and can exclude the player. Skeleton collision bodies and a parsed `RagdollTemplate` already exist. | Ray hits identify a bone body, while canonical reference identity lives on the placement root. `PhysicsSourceForm` covers static placement colliders, not actor bones. P2 needs one bone/descendant-to-actor-root ownership path. The `RagdollTemplate` is attached to the skeleton root, but `activate_ragdoll` expects the entity passed to it to own the template. |
| Hit/damage/death | Canonical `HitEvent` is registered as a transient ECS marker; end-of-frame cleanup handles it. | There is no production `HitEvent` producer or consumer, no health-to-death transition, and no combat-AI disable step. Existing `apply_damage` uses positive accumulated damage, so the consumer contract is already clear once Health exists. |
| Loot/persistence | NPC `Inventory` and the authored death-item FormID are parsed; inventory state is already an ECS component. | No dead/corpse interaction, death-item materialization, transfer UI, looted marker, or dead/looted change-form coverage exists. |

This trace also corrects an easy false assumption: Skyrim NPC spawn does call
the shared actor-value stamping function, but the derivation dispatcher only
produces values for FO3/FNV and FO4-style stored properties. Merely wiring
`HitEvent` to `apply_damage` would therefore create a combat path that silently
cannot damage this fixture.

## Implementation slices

1. **Actor readiness:** parse/derive Skyrim Health for this base NPC; add a
   canonical actor-root ownership link usable by ray hits and ragdoll
   activation; choose one concrete two-handed weapon and attach its state.
2. **Cause and effect:** add Attack/Block actions, emit one `HitEvent` from the
   normal action path, consume it into Health damage, and transition once to a
   dead state that removes ambient/combat participation.
3. **Readable response:** play one two-handed attack plus hit/death response,
   emit one spatial sound family, and activate the existing ragdoll path at
   death.
4. **Loot and continuity:** make the corpse interactable, materialize its death
   item, transfer inventory through the native UI path, and persist dead/looted
   state across save → exit → reload.

## Closure gate

The P2 smoke must use physical Attack input rather than a damage console
command and assert the complete chain:

```text
Attack edge -> actor-owned ray hit -> one HitEvent -> Health decreases
            -> zero Health once -> dead/AI-disabled -> death response
            -> corpse activation -> inventory transfer -> reload continuity
```

The smoke also asserts the frozen reference/base FormIDs and weapon family at
preflight. Unit tests may construct the individual state transitions, but P2
does not close until the chain passes against this real `Skyrim.esm` actor.
