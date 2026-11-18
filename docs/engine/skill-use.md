# Oblivion skill use — what the original does, and the emitter contract

**Status**: Phase 0 design (#7049, 2026-11-18) of the [Oblivion skill-use leveling PRD](../prd/oblivion-skill-use-leveling.md); **Phase 1 built (#7050)**: §1's arithmetic and §4's contract run (`crates/core/src/character/skill_use.rs`, `crates/scripting/src/oblivion_skill_use.rs`); no emitter feeds it yet (Phase 2, #7051).
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

OSL-USE-006 and -007 stand: a skill whose system the engine lacks is reported unfed, never fed by a stand-in action. Which rows the engine
has a system for is the PRD's §3 and Phase 2's census (#7051). From this document's side: alchemy (rows 1 and 2), the lock-picking game
(5), the persuasion game (6), repair (7) and barter (4) are menus and games the original runs and the engine does not run for Oblivion;
combat, magic, movement and stance time (rows 8 to 17) are the actions systems the engine already has to varying degrees.

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
