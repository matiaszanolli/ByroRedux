# Papyrus Script API Reference

Complete reference for Papyrus script objects, their inheritance hierarchy,
and API surfaces. This documents what ByroRedux's scripting layer must support
for legacy content compatibility.

Source: [Fallout 4 Creation Kit Wiki — Script Objects](https://falloutck.uesp.net/wiki/Category:Script_Objects)

---

## Script File Format

**Pipeline:** `.psc` (source) → `.pas` (assembly) → `.pex` (bytecode) → VM → `.log` (debug)

**Source grammar (`.psc`):**
```
ScriptName <identifier> [extends <identifier>] [Native] [Const] [DebugOnly] [Hidden]
(Import | Variable | Struct | CustomEvent | Property | Group | State | Function | Event)*
```

- **Native:** engine-backed, can define new events and native functions, no variables/auto properties
- **Const:** stateless, game can unload at will, no non-const auto props/states/variables
- **Hidden:** excluded from editor script picker
- **DebugOnly:** all functions stripped from release builds
- **Import:** `Import <script>` or `Import <namespace:script>` for unqualified access
- **Namespaces:** colon-delimited paths (`MyNamespace:MyScript:MyStruct`), maps to folder paths
- **Line continuation:** `\` at end of line
- **Comments:** `;` single-line, `;/ ... /;` multi-line, `{ ... }` doc comments (editor tooltips)

**Type system:**
- Primitives: `Bool`, `Int`, `Float`, `String`
- Object types: any script type (e.g., `Actor`, `ObjectReference`, `Quest`)
- Arrays: `Int[]`, `Actor[]` — dynamically resizable (FO4+)
- `Var` type (FO4+): holds any value, used for custom event args and reflection
- Structs (FO4+): named groups of typed fields, passed by reference

**Inheritance rules:**
- Child overrides parent by matching function signature exactly
- Unoverridden functions/events are inherited
- `parent.Func()` explicitly calls the parent's version
- States merge: child + parent states combine, child functions in same state win
- Properties are inherited but NOT overridable
- Variables are private: same name in child shadows parent completely (different types allowed)

---

## Script Object Hierarchy (101 types)

All scripts implicitly extend `ScriptObject`. The hierarchy splits into:

### Root: ScriptObject

The implicit base of every script. Defines core infrastructure:

**State machine:**
- `GetState()` / `GotoState(name)` — switch active state
- `OnBeginState(oldState)` / `OnEndState(newState)` — transition events

**Timer system (two clocks):**
- `StartTimer(interval, timerID)` / `CancelTimer(timerID)` → `OnTimer(timerID)` — real-time
- `StartTimerGameTime(interval, timerID)` / `CancelTimerGameTime(timerID)` → `OnTimerGameTime(timerID)` — game-time

**Reflection (string-based dynamic dispatch):**
- `CallFunction(name, params)` — synchronous by name, returns `Var`
- `CallFunctionNoWait(name, params)` — asynchronous by name
- `GetPropertyValue(name)` / `SetPropertyValue(name, value)` — property access by string
- `CastAs(scriptName)` — runtime cast without compile-time dependency
- `IsBoundGameObjectAvailable()` — check if backing game object is valid

**Event registration system (17 Register, 20+ Unregister):**

| Category | Registration | Event |
|---|---|---|
| Animation | `RegisterForAnimationEvent(ref, name)` | `OnAnimationEvent` |
| Custom | `RegisterForCustomEvent(source, name)` | user-defined |
| LOS (4 variants) | `RegisterForDetectionLOSGain/Lost`, `RegisterForDirectLOSGain/Lost` | `OnGainLOS`, `OnLostLOS` |
| Distance (2) | `RegisterForDistanceGreaterThan/LessThan` | `OnDistanceGreaterThan/LessThan` |
| Hit | `RegisterForHitEvent(target, aggressor, source, proj, power, sneak, bash, block, match)` | `OnHit` |
| Magic | `RegisterForMagicEffectApplyEvent(target, caster, effect, match)` | `OnMagicEffectApply` |
| Menu | `RegisterForMenuOpenCloseEvent(menuName)` | `OnMenuOpenCloseEvent` |
| Player | `RegisterForPlayerSleep/Teleport/Wait` | `OnPlayerSleep/Teleport/Wait Start/Stop` |
| Radiation | `RegisterForRadiationDamageEvent(target)` | `OnRadiationDamage` |
| Remote | `RegisterForRemoteEvent(source, eventName)` | any event on source entity |
| Tracked Stats | `RegisterForTrackedStatsEvent(stat, threshold)` | `OnTrackedStatsEvent` |
| Tutorial | `RegisterForTutorialEvent(name)` | `OnTutorialEvent` |
| Inventory | `AddInventoryEventFilter(form)` | filters `OnItemAdded/Removed` |

**F4SE additions:** `RegisterForKey(DXScanCode)`, `RegisterForControl(name)`,
`RegisterForCameraState()`, `RegisterForFurnitureEvent(filter)`,
`RegisterForExternalEvent(name, callback)` — enables cross-mod communication.

**Native events (23):** `OnInit`, `OnBeginState/OnEndState`, `OnTimer/OnTimerGameTime`,
`OnAnimationEvent`, `OnDistanceGreaterThan/LessThan`, `OnGainLOS/OnLostLOS`, `OnHit`,
`OnMagicEffectApply`, `OnMenuOpenCloseEvent`, `OnPlayerSleepStart/Stop`,
`OnPlayerTeleport`, `OnPlayerWaitStart/Stop`, `OnRadiationDamage`,
`OnTrackedStatsEvent`, `OnTutorialEvent`, `OnLooksMenuEvent`.

### Utility Scripts (extend ScriptObject, no Form backing)

Global/static function containers — no game object instance:

| Script | Purpose | ECS Mapping |
|---|---|---|
| `Debug` | Trace, notification, message box, dump stacks | Logging system |
| `Game` | GetPlayer, GetForm, fast travel, save/load, difficulty, spatial queries | World resource + spatial index |
| `Math` | Trig, abs, pow, sqrt, floor, ceiling | `f32`/`f64` methods |
| `Utility` | Wait, RandomInt/Float, GameTimeToString, INI access | Timer components + RNG + config resource |
| `Input` | Key mapping, mouse, gamepad | Input resource |
| `InputEnableLayer` | Enable/disable input layers | Layered input state |
| `UI` | Scaleform menu interaction (Get/Set/Invoke/Load) | UI bridge |
| `InstanceData` | Weapon/armor instance modification | ObjectMod system |
| `CommonArrayFunctions` | Array helpers | Rust iterators |
| `F4SE` | Script extender functions | Not applicable (native) |

### ActiveMagicEffect (extends ScriptObject, not a Form)

Represents a running magic effect instance on an actor. Temporary component,
removed when effect expires. Not a persistent game object.

### Alias Hierarchy (extends ScriptObject)

Quest alias system — indirect references resolved at runtime:

```
Alias
├── LocationAlias      — points to a Location
├── RefCollectionAlias — collection of references
└── ReferenceAlias     — points to a single ObjectReference
```

**ECS:** Component references or query results. No alias indirection needed —
entities are addressed directly.

### Form (extends ScriptObject) — 85 subtypes

The main hierarchy. Every persistent game object is a Form.

**Form base API (7 native functions):**
- `GetFormID()`, `GetGoldValue()`, `HasKeyword(kw)`, `HasKeywordInFormList(list)`
- `PlayerKnows()`, `StartObjectProfiling()`, `StopObjectProfiling()`

**F4SE additions (17):** `GetName/SetName`, `GetWeight/SetWeight`, `GetKeywords`,
`GetDescription`, `Get/SetEnchantment`, `Get/SetEquipType`, `Get/SetGoldValue`,
`Get/SetIconPath`, `Get/SetMessageIconPath`, `Get/SetWorldModelPath`, `HasWorldModel`

```
Form
├── Action
├── Activator
│   ├── Flora
│   ├── Furniture
│   └── TalkingActivator
├── ActorBase
├── ActorValue
├── Ammo
├── Armor
├── AssociationType
├── Book
├── CameraShot
├── Cell
├── Class
├── CombatStyle
├── Component (crafting)
├── Container
├── DefaultObject
├── Door
├── EffectShader
├── Enchantment
├── EncounterZone
├── EquipSlot
├── Explosion
├── Faction
├── FormList
├── GlobalVariable
├── Hazard
├── HeadPart
├── Holotape (FO4)
├── Idle
├── IdleMarker
├── ImageSpaceModifier
├── ImpactDataSet
├── Ingredient
├── InstanceNamingRules (FO4)
├── Keyword
│   └── LocationRefType
├── LeveledActor
├── LeveledItem
├── LeveledSpell
├── Light
├── Location
├── MagicEffect
├── Message
├── MiscObject
│   ├── ConstructibleObject
│   ├── Key
│   └── SoulGem
├── MusicType
├── ObjectMod (FO4)
├── ObjectReference
│   └── Actor                    ← deepest chain, largest API (~150 functions)
├── Outfit
├── OutputModel
├── Package
├── Perk
├── Potion
├── Projectile
├── Quest
├── Race
├── Scene
├── Scroll
├── ShaderParticleGeometry
├── Shout
├── Sound
├── SoundCategory
├── SoundCategorySnapshot
├── Spell
├── Static
│   └── MovableStatic
├── Terminal
├── TextureSet
├── Topic
├── TopicInfo
├── VisualEffect
├── VoiceType
├── WaterType
├── Weapon
├── Weather
├── WordOfPower
└── WorldSpace
```

**FO4-specific types:** Holotape, InstanceNamingRules, ObjectMod, Component, InstanceData.

---

## Actor Script — Full API Decomposition

Actor (extends ObjectReference extends Form) is the largest script type:
~150 native member functions, ~5 F4SE functions, ~40 events.

### ECS Component Decomposition

The Actor monolith decomposes into ~15 independent components:

**1. CombatState**
- Functions: `GetCombatState`, `GetCombatTarget`, `GetAllCombatTargets`, `StartCombat`, `StopCombat`, `StopCombatAlarm`, `IsInCombat`, `IsHostileToActor`, `SetAttackActorOnSight`, `GetFactionReaction`
- Events: `OnCombatStateChanged`, `OnKill`

**2. Equipment**
- Functions: `EquipItem`, `UnequipItem`, `UnequipItemSlot`, `UnequipAll`, `EquipSpell`, `UnequipSpell`, `IsEquipped`, `GetEquippedWeapon/Shield/Spell`, `GetEquippedItemType`, `WornHasKeyword`, `DrawWeapon`, `IsWeaponDrawn`, `MarkItemAsFavorite`
- Events: `OnItemEquipped`, `OnItemUnequipped`
- F4SE: `GetWornItem` (WornItem struct with Biped Slots), `GetWornItemMods`, `GetInstanceOwner`

**3. FactionMembership**
- Functions: `AddToFaction`, `RemoveFromFaction`, `RemoveFromAllFactions`, `GetFactionRank`, `SetFactionRank`, `ModFactionRank`, `IsInFaction`, `GetCrimeFaction`, `SetCrimeFaction`
- Relationships: `GetRelationshipRank`, `SetRelationshipRank`, `GetHighest/LowestRelationshipRank`, `HasAssociation`, `HasFamilyRelationship`, `HasParentRelationship`

**4. SpellBook**
- Functions: `AddSpell`, `RemoveSpell`, `HasSpell`, `DispelSpell`, `DispelAllSpells`, `DoCombatSpellApply`, `HasMagicEffect`, `HasMagicEffectWithKeyword`, `TrapSoul`

**5. PerkList**
- Functions: `AddPerk`, `RemovePerk`, `HasPerk`

**6. AIState**
- Functions: `EnableAI`, `IsAIEnabled`, `EvaluatePackage`, `GetCurrentPackage`, `PathToReference`
- Events: `OnPackageStart`, `OnPackageEnd`, `OnPackageChange`

**7. CompanionState**
- Functions: `AllowCompanion`, `DisallowCompanion`, `SetCompanion`, `SetAvailableToBeCompanion`, `IsPlayerTeammate`, `SetPlayerTeammate`, `SetDoingFavor`, `IsDoingFavor`, `SetCanDoCommand`, `FollowerFollow/Wait`, `FollowerSetDistance(Near/Medium/Far)`, `MakePlayerFriend`
- Events: `OnCompanionDismiss`, `OnCommandMode(Enter/Exit/GiveCommand/CompleteCommand)`

**8. VitalState**
- Functions: `Kill`, `KillEssential`, `KillSilent`, `Resurrect`, `IsDead`, `IsEssential`, `SetEssential`, `SetProtected`, `IsBleedingOut`, `Get/SetNoBleedoutRecovery`, `IsUnconscious`, `SetUnconscious`, `AllowBleedoutDialogue`, `ResetHealthAndLimbs`, `StartDeferredKill/EndDeferredKill`, `Dismember`, `IsDismembered`, `SetCriticalStage`
- Critical stages enum: None, GooStart/End, DisintegrateStart/End, FreezeStart/End
- Events: `OnDeath`, `OnDying`, `OnDeferredKill`, `OnEnterBleedout`, `OnCripple`, `OnPartialCripple`, `OnConsciousnessStateChanged`

**9. AnimationState**
- Functions: `ChangeAnimArchetype/FaceArchetype/Flavor`, `AttemptAnimationSetSwitch`, `PlayIdle/IdleAction/IdleWithTarget`, `PlaySubGraphAnimation`, `SetSubGraphFloatVariable`, `SetAnimArchetype(Confident/Depressed/Elderly/Friendly/Irritated/Neutral/Nervous)`, `SetDogAnimArchetype(Agitated/Alert/Neutral/Playful)`

**10. MovementState**
- Functions: `IsRunning`, `IsSprinting`, `IsSneaking`, `StartSneaking`, `IsOnMount`, `Dismount`, `IsBeingRidden/By`, `IsSeatOccupied`, `SetVehicle`, `SnapIntoInteraction`, `SwitchToPowerArmor`, `IsInPowerArmor`
- Events: `OnEnterSneaking`, `OnSit`, `OnGetUp`, `OnPlayerEnterVertibird`

**11. DetectionState**
- Functions: `IsDetectedBy`, `HasDetectionLOS`, `IsAlarmed`, `IsAlerted`, `SetAlert`, `GetLightLevel`, `SetNotShowOnStealthMeter`

**12. DialogueState**
- Functions: `AllowPCDialogue`, `GetDialogueTarget`, `IsTalking`, `ShowBarterMenu`, `SetOverrideVoiceType`
- Events: `OnPickpocketFailed`

**13. FlightState**
- Functions: `IsFlying`, `CanFlyHere`, `IsAllowedToFly`, `SetAllowFlying`, `GetFlyingState`, `Set/Get/ClearForcedLandingMarker`

**14. AppearanceState**
- Functions: `ChangeHeadPart`, `SetEyeTexture`, `SetAlpha`, `ClearExpressionOverride`, `SetHasCharGenSkeleton`, `SetHeadTracking`, `Set/ClearLookAt`, `Set/GetRace`, `ClearExtraArrows`, `AttachAshPile`
- F4SE: `QueueUpdate` (full rebuild)

**15. CrimeState**
- Functions: `IsArrested`, `ClearArrested`, `IsArrestingTarget`, `Is/SetBribed`, `Is/SetIntimidated`, `IsTrespassing`, `WillIntimidateSucceed`, `GetBribeAmount`, `SendAssaultAlarm`, `SendTrespassAlarm`, `SetPlayerResistingArrest`, `WouldBeStealing`, `IsOwner`, `UnlockOwnedDoorsInCell`

### Actor Events (40 total)

| Category | Events |
|---|---|
| Lifecycle | `OnDeath`, `OnDying`, `OnDeferredKill`, `OnEnterBleedout`, `OnCripple`, `OnPartialCripple` |
| Combat | `OnCombatStateChanged`, `OnKill` |
| Equipment | `OnItemEquipped`, `OnItemUnequipped` |
| AI | `OnPackageStart`, `OnPackageEnd`, `OnPackageChange` |
| Movement | `OnLocationChange`, `OnSit`, `OnGetUp`, `OnEnterSneaking` |
| Companion | `OnCompanionDismiss`, `OnCommandMode*` (4) |
| Consciousness | `OnConsciousnessStateChanged` |
| Player-specific | `OnPlayerFallLongDistance`, `OnPlayerEnterVertibird`, `OnPlayerCreateRobot`, `OnPlayerModArmorWeapon`, `OnPlayerModRobot`, `OnPlayerUseWorkBench`, `OnPlayerSwimming`, `OnDifficultyChanged`, `OnPlayerLoadGame` |
| Settlement | `OnWorkshopNPCTransfer` (FO4) |
| Escort | `OnEscortWaitStart/Stop` |

---

## ObjectMod System (FO4-specific)

Object Modification records (`OMOD`) are a data-driven property modification system:

**Structure:** Each mod is a list of `(target_field, operator, value)` tuples applied to base items.

**6 operators:** Set, Add, Mult-Add, And, Or, Remove

**5 value types:** Bool, Int, Float, Form, FormFloat (form + numeric), Enum

**Targets:** Weapon (~70 modifiable properties), Armor (~10), Actor (~6), Furniture (power armor)

**Weapon property highlights (from F4SE):**
- Damage: `iAttackDamage`, `fSecondaryDamage`, `fCriticalDamageMult`, `vdDamageTypeValues`
- Range: `fMinRange`, `fMaxRange`, `fOutOfRangeDamageMult`
- Fire rate: `fSpeed`, `fAttackDelaySec`, `fFireSeconds`, `bAutomatic`
- Recoil model: 12 floats (`fAimModelCone*`, `fAimModelRecoil*`, `fAimModelBaseStability`)
- Ammo: `iAmmoCapacity`, `poAmmo`, `pnNPCAmmoList`, `uNumProjectiles`
- Sounds: 7 sound slots (attack, loop, equip, idle, fail, unequip, fast equip)
- Zoom: `fZoomDataCameraOffset(X/Y/Z)`, `pgZoomDataImageSpace`, `eoZoomDataOverlay`

**Attach point system:** Keyword-based slots where mods attach to specific 3D model nodes.
Parent slots define sub-mod attachment points. `Collect from 3D` reads NIF node names.

**ECS mapping:** ObjectMods become modifier components. Systems apply the operator stack
to base component values during stat calculation. Priority and rank control stacking order.

---

## Game & Utility Script APIs

### Game Script — Global Game Functions

Key functional groups:
- **Spatial queries (8 variants):** `FindClosest/RandomActor/Reference/OfType/FromRef/InList`
- **Form lookup:** `GetForm(id)`, `GetFormFromFile(id, filename)`
- **Player access:** `GetPlayer()`, `GetPlayerFollowers()`, `GetPlayerLevel()`
- **SPECIAL stats:** `GetStrength/Perception/Endurance/Charisma/Intelligence/Agility/LuckAV()`
- **Input queries (12):** `Is*ControlsEnabled()` — mirrors InputEnableLayer
- **Camera:** `ForceFirstPerson/ThirdPerson`, `SetCameraTarget`, `ShakeCamera/Controller`
- **Save/Load:** `RequestSave/AutoSave`, `QuitToMainMenu`
- **Game settings:** `GetGameSetting(Float/Int/String)`, F4SE: `SetGameSetting*`
- **F4SE plugin introspection:** `GetInstalledPlugins/LightPlugins`, `GetPluginDependencies`

### Utility Script — Generic Utilities

- **Latent functions:** `Wait(seconds)`, `WaitGameTime(hours)`, `WaitMenuMode(seconds)` — canonical examples of VM stack suspension that ECS eliminates
- **Reflection:** `CallGlobalFunction/NoWait(scriptName, funcName, params)` — string-based global dispatch
- **Time:** `GetCurrentGameTime()` (game days), `GetCurrentRealTime()` (real seconds)
- **Random:** `RandomInt(min, max)`, `RandomFloat(min, max)`
- **INI:** `SetINI(Bool/Float/Int/String)` — runtime config modification
- **Performance:** Frame rate capture, memory budget tracking, stack ID inspection

---

## ECS Mapping Patterns

| Papyrus Pattern | ECS Equivalent |
|---|---|
| `Is*()` / `Set*()` pairs | Bool fields on components |
| `Get*()` functions | Component field reads or queries |
| Script inheritance (`extends`) | Component composition (attach multiple components) |
| States (`GoToState`) | Enum field on component, system checks before processing |
| Properties | Component fields, editor-configured via plugin manifests |
| Const properties | Field defaults, skipped in save serialization |
| Event registration | Marker component presence = subscribed |
| Single-shot events (LOS, distance) | One-shot system removes marker after firing |
| Filtered events (hit, magic) | Filter data in registration component |
| Remote events | Component referencing source entity + system query |
| Custom events | Event bus or component signaling |
| Timers | Timer component with pending entries, ticked per frame |
| Latent functions (Wait) | Timer component, no stack suspension |
| Reflection (CallFunction by string) | Component queries by type; legacy compat needs string→type map |
| `parent.Func()` | Shared behavior in separate system; specialization in additional system |
| Import | Rust `use` statements |
| Namespaces (`A:B:C`) | Rust module paths (`a::b::c`) |

---

## References

- [Script Objects Category](https://falloutck.uesp.net/wiki/Category:Script_Objects)
- [ScriptObject Script](https://falloutck.uesp.net/wiki/ScriptObject_Script)
- [Form Script](https://falloutck.uesp.net/wiki/Form_Script)
- [Actor Script](https://falloutck.uesp.net/wiki/Actor_Script)
- [ObjectMod Script](https://falloutck.uesp.net/wiki/ObjectMod_Script)
- [Game Script](https://falloutck.uesp.net/wiki/Game_Script)
- [Utility Script](https://falloutck.uesp.net/wiki/Utility_Script)
- [Script File Structure](https://falloutck.uesp.net/wiki/Script_File_Structure)
- [Extending Scripts](https://falloutck.uesp.net/wiki/Extending_Scripts_(Papyrus))
- Internal: `docs/engine/scripting.md` (ECS scripting architecture)
