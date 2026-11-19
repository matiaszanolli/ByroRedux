# Oblivion skill use — what the original does, and the emitter contract

**Status**: Phase 0 design (#7049, 2026-11-18) of the [Oblivion skill-use leveling PRD](../prd/oblivion-skill-use-leveling.md); **Phase 1 built (#7050)**: §1's arithmetic and §4's contract run (`crates/core/src/character/skill_use.rs`, `crates/scripting/src/oblivion_skill_use.rs`); **Phase 2 built (#7051)**: the emitters in `byroredux/src/skill_use_emit.rs` feed 8 of the 21 skills (§4); **Phase 3 built (#7052)**: the level-up, the attribute picks and the completed-sleep trigger (`crates/scripting/src/oblivion_skill_use.rs`).
It records what `Oblivion.exe` does when the player uses a skill (OSL-XP-007's action census, V3 and V4), and the contract every emitter in
the engine follows. The level-up, the attribute bonus and the sleep trigger are in the PRD; the Health term is in
[charal-oblivion-ruleset.md](charal-oblivion-ruleset.md). Addresses are `Oblivion.exe` 1.2.0.416 (sha256 `a8f31384…dc6`, image base
`0x400000`). The raw notes are `.claude/issues/7049/INVESTIGATION.md`.

## 1. The original's model

Three routines, one behind the other (`this` is `PlayerCharacter`, `[0xb333c4]`):

| Routine | Does |
|---|---|
| `0x668c30(skill, usage, mult)` (vtable slot 231, offset `0x39c`) | reads the amount from the skill's `SKIL` record, `use value[usage]` (`SKIL + 0x38 + usage · 4`), multiplies by `mult` when `mult` is not 0, drops anything not above 0, and passes the rest on |
| `0x668b30(skill, amount, record, flag)` | **a skill at 100 or more banks nothing** (`GetAV(skill) >= 100 → return`); otherwise `exp += amount`, and when the skill's cost is **strictly below** the experience, calls the rank-up once |
| `0x668070(record, …)` | the rank-up: `exp = max(exp − cost, 0)` (a surplus stays banked), the skill's **base** value `+ 1`, the cost recomputed, the attribute tally and the major count updated, the level made pending at the tenth major increase |

So one call ranks a skill at most once; a large amount ranks again on the next use. The engine's single entry point (OSL-XP-001) loops
the original's step and returns the ranks it reached.

The cost (`0x548030`, stored by `0x663c50`): `(fSkillUseFactor · L) ^ fSkillUseExp · (specialization ? fSkillUseSpecMult : 1) · (major ?
fSkillUseMajorMult : fSkillUseMinorMult)`. The setting is **`fSkillUseMinorMult`** (not `fSkillUseMinor`); the major multiplier replaces
the minor one. Executable defaults 1 / 1 / 1.25 / 0.75 / 0.75 (Factor / Exp / Minor / Major / Spec); `Oblivion.esm` authors Factor 0.35
and Exp 1.5.

### The amounts: `SKIL` use values (V3)

UESP's per-action experience are the two use values each `Oblivion.esm` `SKIL` record authors (`DATA`: action, attribute,
specialization, use value 1, use value 2); `usage` 0 reads value 1, `usage` 1 value 2. They are held as data, read from the load order
(OSL-USE-001), not copied here:

| Skill | Value 1 | Value 2 | Skill | Value 1 | Value 2 |
|---|---|---|---|---|---|
| Armorer | 1.5 | 1 | Acrobatics | 0.3 | 3 |
| Athletics | 0.03 | 0.04 | Light Armor | 1.5 | 1 |
| Blade | 0.5 | 1 | Marksman | 0.8 | 1 |
| Block | 1.25 | 1 | Mercantile | 0.4 | 1 |
| Blunt | 0.5 | 1 | Security | 1.5 | 1 |
| Hand to Hand | 0.6 | 1 | Sneak | 0.75 | 2 |
| Heavy Armor | 1.25 | 1 | Speechcraft | 2.4 | 1 |
| Alchemy | 5 | 0.5 | Alteration | 0 | 4 |
| Conjuration | 0 | 6 | Destruction | 0 | 1.2 |
| Illusion | 0 | 3 | Mysticism | 0 | 3 |
| Restoration | 0 | 0.6 | | | |

The CS's `GetSkillUseIncrement` is not a command of this executable.

## 2. The action census (V4)

Every call of `0x668c30` is a call through the vtable slot; the executable has 19 of them in 15 routines. Each row gives the skill, the
`usage` (which use value) and the multiplier, with what triggers it. **Confidence**: *read* means the skill, the usage and the code that gates the call were read;
*named* means the skill and the usage were read and the trigger is named from the menu or system that owns the routine (not verified
by running the original).

| # | Skill, usage, mult | Trigger | Site | Confidence |
|---|---|---|---|---|
| 1 | Alchemy, 0, 0 | a potion is made at the alchemy menu (the potion count `player + 0x688` rises with it) | `0x594cd1` (`0x594ca0`) | named |
| 2 | Alchemy, 1, 0 | an ingredient is eaten (the magic-item type switch's case 8) | `0x5fca20` (`0x5fc9f3`) | read |
| 3 | Sneak, 1, 0 | an item is taken from a pickpocketed container, behind a static flag (`0xb13e90`, 1 by default) | `0x5983d5` | named |
| 4 | Mercantile, 0, gold ÷ 100 | a completed barter, the traded value over 100, in two routines | `0x59abad`, `0x5d985a` | named |
| 5 | Security, 0, 0 | the lock-picking menu, at two points | `0x5b0378`, `0x5b0c0c` | named |
| 6 | Speechcraft, 0, 0 | a persuasion game completed with all four quadrants filled and the disposition changed (`player + 0x6bc` rises) | `0x5c0036` | named |
| 7 | Armorer, 0, 0 | an item repaired at the repair menu | `0x5d287c` | named |
| 8 | Block, 0, 0 | a hit blocked | `0x5f5bf9` (Actor vtable slot 237) | named |
| 9 | Light Armor 27 or Heavy Armor 18, 0, 0 | the player is hit: the armor piece struck (`0x5e5a00` picks the piece; a form of type `0x14` is tested for heavy by `0x4b4c70`) | `0x5ffc90` | read |
| 10 | Acrobatics, 1, 0 | the player lands and takes fall damage (the fall-damage block; not under feather, water or flight) | `0x5efe4d` | read |
| 11 | Acrobatics, 1, 0 | a second landing site gated by a setting at `0xb37490` (greater than 0) | `0x5efea3` | named |
| 12 | Acrobatics, 0, 0 | in the per-frame player update: a jump | `0x672ad2` | named |
| 13 | Sneak, usage `edi`, 0 | in the per-frame update, once a one-second timer (`player + 0x5a4`) fills | `0x671866` | named |
| 14 | Athletics, 1, 0 | the same, a one-second timer (`player + 0x5a0`): swimming | `0x6718c8` | named |
| 15 | Athletics, usage `edi`, 0 | the same, a one-second timer (`player + 0x59c`): running | `0x671922` | named |
| 16 | the school of the spell, usage 0 (spell types 0 and 5), 0 | a spell effect applied by the player | `0x5fca20` | named |
| 17 | a skill from `vtable + 0x21c` (the weapon's skill, or 0x48 for none), once per attack counted by `vtable + 0x214` | the swing loop at `0x5fd816` | `0x5fd83c` | named |
| 18 | forwarder | `Actor` slot 229 hands the call to the middle process | `0x5e58d9` | read |

So the answers to V4:

- **Which armor piece takes a hit's experience**: one piece per hit, the one `0x5e5a00` selects; Light or Heavy Armor by that piece
  (row 9). A mixed outfit feeds whichever piece is struck.
- **Jump, fall and dodge**: Acrobatics value 1 (3) for taking fall damage (rows 10 and 11) and value 0 (0.3) for a jump (row 12). The
  executable has no dodge action; UESP's "dodge" is the sidestep, which the census does not find as a separate call.
- **A pickpocket counts**: Sneak value 2 per item successfully taken (row 3).
- **Time-based skills**: Athletics (value 1 swimming 0.04, value 0 running 0.03) and Sneak bank on a one-second timer, not per frame
  (rows 13 to 15); the engine's per-second emitter of OSL-USE-005 matches.

**Not yet read to the end** (carried as Phase 2 verify items on #7051, since each needs its system first): which usage rows 13 and 15
select (`edi`), the second Acrobatics site (row 11), whether the swing loop (row 17) feeds per swing or per landed hit
(OSL-USE-002 requires a landed hit), and the spell-school routine's usage for school skills (their value 1 is 0, so a usage-0 call would
bank nothing; the schools' value 2 is the amount UESP lists). Until read, those emitters are not fed.

### UESP's action count (the denominator of the PRD's "fed actions" metric)

UESP's *Oblivion: Increasing Skills* table (local dump `uesp-wiki/Oblivion/Increasing Skills.wiki`) lists **26 actions across 21 skills**:
**8 combat** (Armorer repair; Athletics running and swimming; Blade, Blunt and Hand to Hand hits; Block; Heavy Armor hit taken),
**8 magic** (Alchemy potion made and ingredient eaten; one spell cast with a valid target for each of the six schools) and
**10 stealth** (Acrobatics jump or dodge and damaging fall; Light Armor hit taken; Marksman shot hitting; Mercantile sale; Security tumbler fixed
and Auto-Attempt click; Sneak undetected movement and pickpocket; Speechcraft turn). Its per-action amounts agree with the `SKIL` use
values of §1 (for example Alteration 4.0 = value 2, Alchemy potion 5.0 = value 1, Sneak pickpocket 2.0 = value 2). Where UESP's wording and the
executable differ, the executable's census is the source: Mercantile banks per barter by value traded (row 4), not per sale, and UESP's
"dodge" has no call of its own (row 10 to 12). The metric's denominator is therefore 26, split 8 / 8 / 10 by school.

## 3. What the engine can feed

OSL-USE-006 and -007 stand: a skill whose system the engine lacks is reported unfed, never fed by a stand-in action. `levelup` prints the
unfed list; its source is `FED_SKILLS` in `oblivion_skill_use.rs`, where a skill joins in the commit that adds its emitter.

**Fed since #7051 Phase 2a** (5 of 21 skills), all at `combat_damage_system`'s one `HitEvent` site (`byroredux/src/skill_use_emit.rs`):

- **Blade, Blunt, Hand to Hand** (usage 0: 0.5, 0.5, 0.6): a melee blow the player lands on a living target. The weapon's skill is its
  `WEAP` type: 0 and 1 are blades, 2 and 3 blunt weapons, no weapon is Hand to Hand (UESP's weapon pages). A staff (type 4) casts and a
  bow (type 5) has no shot path in the engine, so neither feeds a melee skill. UESP and the PRD say "each hit", so the emitter feeds per
  landed blow, not per swing; whether the executable's swing loop (row 17) counts swings is not read. Live (`oblivion-level-up.sh`): six
  unarmed blows on an Anvil patron took Hand to Hand from 5 to 7 with 0.59 banked, which is 6 × 0.6 less the two rank costs (1.30 and 1.71).
- **Light and Heavy Armor** (usage 0: 1.5, 1.25): a hit the player takes. The struck piece is `0x5e5a00`'s roll, decoded this phase: a
  number from 0 to 99 is compared with the running total of the five slots' weights, Helmet 10, Cuirass 25, Greaves 15, Gauntlets 10,
  Boots 10 (`iArmorDamageHelmChance` and its siblings, executable defaults; the setting objects are created in the order Cuirass,
  Greaves, Helm, Gauntlets, Boots and are summed in the order Helm, Cuirass, Greaves, Gauntlets, Boots), and a roll past the fifth slot
  strikes the shield, the remaining 30. An empty struck slot re-rolls, up to seven times; a piece that is a heavy armor feeds Heavy Armor,
  any other Light Armor. Oblivion's heavy flag is `BMDT` General Flags bit 7, now kept as `armor_type` 1 (the Skyrim field). The engine's roll is
  a deterministic mix of the hit counter and the two actors, not the executable's RNG.

**Fed since #7051 Phase 2b** (3 more skills, 8 of 21 in all):

- **Athletics, Sneak** (`stance_use_system`, the player's per-frame update `0x671620`): the original keeps three one-second timers that
  count only while their stance holds, fire at a second and reset. Sneaking and moving feeds **Sneak usage 0** (0.75), else swimming and
  moving feeds **Athletics usage 1** (0.04), else running and moving feeds **Athletics usage 0** (0.03). The two usages the Phase 0 census
  left open (rows 13 and 15) are decoded: both push `edi`, which is 0 for an unmounted player, so usage 0. The executable gates its Sneak
  branch on a flag at `player + 0x5a8` that was not traced; the engine reads UESP's and the PRD's "undetected" from the detection level
  (`PlayerDetection`, below Noticed). The engine's "running" is the Sprint action, as the detection tick reads it.
- **Acrobatics** (a jump, usage 0, 0.3): the controller's `jump_fired` on land. UESP: "each jump or dodge".

**Not fed, and why** (OSL-USE-006):

- **The six schools** (row 16): the engine has no player cast action. A spell leaves the player only through the console's `magic.cast`
  and a script's `Cast`, and a script-forced cast is not the player's own action, so feeding from it would be a stand-in. Row 16's routine `0x5fca20` was read this phase: it is the item-use path, and its spell cases pass
  usage 0, whose value is 0 for every school, so the spell-casting site is not among the 19 vtable calls and stays unread. The
  school of a multi-effect spell is the effect with the highest base cost (UESP *Spell Making*); that rule waits for the cast action.
- **Block**: a blocked hit needs a target-side guard, and no producer models one (`HitEvent::blocked` is the aggressor's own stance, #4324).
- **Marksman**: the bow (type 5) has no shot path; the engine's ranged path is the guns'.
- **Acrobatics' fall** (rows 10 and 11): the engine has no fall damage. Row 11's gating setting (`0xb37490`) is still unnamed.
- **Sneak's pickpocket** (row 3): no pickpocket action.
- **Alchemy (rows 1 and 2), Armorer (7), Security (5), Speechcraft (6), Mercantile (4)**: menus and minigames the engine does not run for Oblivion.

**Metrics** (PRD §8, published in the ROADMAP compatibility matrix): fed skills 8 / 21; fed actions 9 / 26 of UESP's table: combat 6 / 8
(Athletics running and swimming, Blade, Blunt, Hand to Hand, Heavy Armor), magic 0 / 8, stealth 3 / 10 (Acrobatics' jump, Light Armor, Sneak's movement).

## 4. The emitter contract (OSL-XP-007)

- **One pending resource.** Emitters in the combat, magic, movement and detection systems append `(skill code, amount)` records to one
  resource (`PendingSkillUse`; the amount is the `SKIL` use value of §1 times the multiplier, resolved at the emitter in Phase 2) (precedent: `PendingSkillAdvances`, `byroredux/src/combat.rs`, drained by the late stage). They do **not** lock the player's
  `ActorValues`; invariants 3 and 4 of `CLAUDE.md` (interior mutability by `RwLock`, TypeId-sorted lock acquisition) stay with the one
  drain.
- **One draining system** (`combat::skill_use_system`). It reads the records, resolves the player's class at each use (OSL-CLS-001,
  OSL-CLS-002), calls the entry point of OSL-XP-001 (`bank_skill_use`) and appends its rank-ups to `SkillUseLevel`. The banked
  experience is the per-skill `SkillProgress` the Skyrim path already saves; the new saved column is `SkillUseLevel` (format 137).
- **Scheduler access declared.** The system declares the resource it writes and the components it reads, as `PendingSkillAdvances` does
  in `boot/schedule/late/scripted_combat.rs`, so the scheduler orders it against the emitters.
- **The same entry for script calls.** `AdvancePCSkill` and `ModPCSkill` (one handler in the executable, `0x50d020`) raise a skill by
  `Amount` **ranks** (V14), through the rank-up path and its tallies, not through use.

## 5. Levels (#7052)

The level-up the rank-ups build toward (`crates/scripting/src/oblivion_skill_use.rs`, `byroredux/src/skill_use_emit.rs`):

- **Pending levels.** `SkillUseLevel` keeps the running attribute tallies and the major-increase count. Every `iLevelUpSkillCount`-th
  (10th) major increase freezes the tallies, **including that increase's own** (V13), into a `PendingLevel` and starts fresh tallies
  (`0x65fb30`); a rank-up after it belongs to the next level. Several levels can be pending, each with its own tallies (OSL-LVL-005).
  `AdvancePCLevel` freezes whatever the tallies are now. Saved: `FORMAT_MAJOR` 138.
- **Picks.** `levelup attr <attribute>` picks one of a pending level's three attributes (distinct, none at 100). The offered bonus is
  `oblivion_attribute_bonus` of the frozen tally (+1 for none, up to +5 for ten; Luck, governing nothing, is always +1). The console is the
  interim surface (PRD Q3, #7048).
- **The sleep.** `sleep_system`, which `SetPCSleepHours` and the jail bed's rest both feed, raises a `SleepCompleted` event after the clock has
  run; `oblivion_level_system` takes the oldest pending level on each completed, uninterrupted sleep (OSL-LVL-004). Waiting raises none, and
  nothing in the engine interrupts a sleep today, so `interrupted` is always false (OSL-SLP-002). A bed outside a jail has no rest action
  yet (the rest menu, #7048), so no other bed raises the event. A level with fewer than three picks stays pending and the sleep says so.
- **Taking a level** (`0x66c3e0`, V7 to V9): each pick's base rises by its bonus, capped at 100, and an unspent tally rolls over to nothing;
  Health's **stored** term gains `floor(0.1 × Endurance)`, Endurance read after the picks (V8), and Magicka's and Fatigue's stored terms are
  untouched (V9); the level rises by one and the count falls by ten, a surplus carrying to the next level. With every attribute at 100 there
  is no level (OSL-LVL-006): "Perfection cannot be improved upon." and the level stays pending. The notification is "You are now level N."
  followed by the load order's `sLevelUp{N}` text (`Oblivion.esm` authors 2 to 20); the tenth major increase says the executable's default
  `sMeditate` (V12).

