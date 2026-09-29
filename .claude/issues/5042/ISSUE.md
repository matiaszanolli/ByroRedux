# CHAR-2026-09-29-D4-01: Constant-spell modifiers on formula-derived AVs create a carried `{base 0, +mod}` entry — `GetActorValue` returns the bare modifier, `melee_damage_charal_bonus` the bare formula (FNV Finesse, FO3/FNV ghouls, FO4 Strong)

**Labels**: medium,bug,character,game:fnv,game:fo3,game:fo4

**Source report**: `docs/audits/AUDIT_CHARACTER_2026-09-29.md`

- **Severity**: MEDIUM
- **Dimension**: Population Boundary
- **Game**: FNV, FO3, FO4 (Skyrim: only via scripted `AddSpell`, e.g. `doomSteedAbility` CarryWeight +100)
- **Source**: charal.md §6 (derived AVs computed from the formula; modifiers layer on the composed value, per the `ActorValues` base/permanent/temporary/damage model). Formula rows: FNV/FO3 derived table (CritChance `1.0·Luck`, RadResist `(END−1)·2`, MeleeDamage `0.5·STR`); FO4 CW `200+10·STR`. The spell data comes from the real-master probe (see Evidence).
- **Location**:
  - `crates/core/src/ecs/components/actor_values.rs:132-134` (`mod_permanent` → `entry().or_default()`)
  - `byroredux/src/npc_spawn.rs:145-167` (`stamp_spell_list`)
  - `crates/scripting/src/magic.rs:202-226` (`apply_constant_modifiers` / `apply_modifiers`)
  - `crates/scripting/src/condition.rs:582-590` (carried fast path) and `:604-632` (derived fall-through)
  - `byroredux/src/combat.rs:444-488` (`melee_damage_charal_bonus`)
- **Status**: NEW. It became production-reachable with `cd4fc019a` (#4415). The `modav` shape predates that.
- **Description**: For most derived stats, the NPC derivation carries no key. Examples are FO3/FNV CritChance, MeleeDamage and RadResist, and FO4 CarryWeight for some actors. The value comes from `CharacterRuleset::derived_value` on read. When `stamp_spell_list` applies an ability that modifies such a stat, `mod_permanent` creates a new entry with base 0. From then on the consumers split:
  - `GetActorValue` sees the key as carried and returns `0 + mod`; the formula is never reached.
  - `melee_damage_charal_bonus` reads only `derived_value`; the modifier is never reached.

  The correct value, formula output plus permanent modifier, comes out of neither. The same entry is also created by `magic::add_spell`, by `modav`, and by the SDK `ActorValueOperation::ModifyPermanent/ModifyTemporary` on any derived key.
- **Evidence**: The probe (`ResolvedNpc::resolve` → `resolve_actor_spells` → `derive_resolved_actor_values`) counted NPCs that carry the ability and whose derivation lacks the key:

  | Master | Spell | Effect | NPCs | Derivation carries the key |
  |---|---|---|---|---|
  | FNV | `PerkFinesse` 0x94EC0 | CritChance +5 | 66 | 0 |
  | FNV | `RadResistGhoul` 0x617BD | RadResist +85 | 12 | 0 |
  | FNV | `MotorRunnerBuff` 0xFE409 | MeleeDamage +10 | 1 (V03MotorRunner) | 0 |
  | FO3 | `RadResistGhoul` | RadResist +85 | 74 | 0 |
  | FO4 | `AbStrongStats` 0x225F7D | STR/AGI +10, CarryWeight +140 | 1 (CompanionStrong) | 0 |

  FO4's `AbMagLiveLoveCompanionPerks` (CW +10) is on 8 companions; 7 of them carry CW via PRPS and compose correctly.

  What a consumer reads today:
  - A Finesse NPC with Luck *L*: `GetActorValue CritChance` returns 5, not `L + 5`.
  - CompanionStrong: `GetActorValue CarryWeight` returns 150, not `200 + 10·STR + 150`.
  - V03MotorRunner's melee bonus ignores the ability's +10.
- **Impact**:
  - CTDA conditions on these AVs silently read wrong values for about 150 vanilla actors across three games.
  - Combat ignores the melee ability.
  - The two CHARAL consumers disagree on the same actor and stat.
  - Nothing warns or fails.
- **Related**: #4415 (open, magic runtime), #2933 (GetActorValue contract), #4452 (melee consumer contract), CHAR-2026-09-29-D1-01 (player-side twin), GAME-D5-2026-09-24-02 / #4819 (spell-delta persistence)
- **Suggested Fix**: Compose in one place with one accessor, `CharacterRuleset::actor_value(avif, &avs, level)`: for a derived Absolute row, `derived_value + permanent_mod + temporary_mod − damage` of the carried entry; otherwise `current`. Route `GetActorValue` and `melee_damage_charal_bonus` through it. The "carried wins" fast path must not treat a base-0 modifier-only entry as authored. Pin with the FNV Finesse and FO4 Strong shapes.

**Validated at HEAD 9fcfdc3fc**: `ActorValues::mod_permanent` still does `entry(avif).or_default()`; the `GetActorValue` arm in `crates/scripting/src/condition.rs` returns `avs.current(..)` whenever `avs.get(..).is_some()` before reaching the ruleset; `melee_damage_charal_bonus` in `byroredux/src/combat.rs` returns only `ruleset.derived_value(..)`.

## Completeness Checks
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
