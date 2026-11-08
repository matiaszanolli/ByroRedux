# Perk Entry Points — Product Requirements

**Status**: ACTIVE (2026-11-05). Revision 6 settles hook 56's Target tab and
records PERK-SUB-002 shipped. Revision 5 files the tracking issues.
Revision 4 recorded the owner's §10 decisions, every one the
recommendation. Revision 3 folded in review round 2:
every §3.3 claim re-read at HEAD; Skyrim perk fragments (PERK-DEC-011) and
the activate entry's run path (PERK-HOOK-012) added; Skyrim's "Not Used"
hooks Inert (PERK-HOOK-011); the embedded-script and FO3/FNV ability-rank
verify items closed from xEdit and the GECK; the Starfield census rule made
consistent with its decode-only scope; Q1, Q3, Q4, Q5 and Q7 restated with
the facts their consequences rest on. Revision 2 (2026-11-01) folded in
round 1; revision 1 was the first draft of the M24.2 remainder: the `PERK`
decode gaps left at HEAD and the runtime that consumes them. Statements
about how an original engine, format or tool behaves cite external sources
(the Creation Kit and GECK wikis in their local dumps, xEdit's record
definitions); a claim that could not be verified is marked **Phase 0:
verify** instead of being stated as fact.

**Tracking**: #6635 (parent). Phases: #6651 (0), #6652 (1), #6654 (2), #6657 (3), #6659 (4).

**Scope**: What the engine promises a `PERK` record does once an actor holds
it, on Fallout 3, Fallout: New Vegas, Skyrim and Fallout 4: every entry body
(Quest, Ability, Entry Point) decoded and acted on, every entry-point *hook* the
masters author either composed by the calculation it modifies or reported with
the reason it is not, and the record's own availability gates honoured when a
perk is taken. It covers the decode residue (record-level conditions, Fallout 4
parameters, rank chains, typed activate data) and the consumer, because a
decode with no reader is the "parsed but unconsumed" anti-pattern ROADMAP
tracks ([Known Issues](../../ROADMAP.md#correctness-and-content)). Out of it:
building the gameplay systems a hook modifies (critical hits, limb damage,
hacking, karma), a level-up menu, Oblivion (no `PERK` record), Fallout 76, and
the Starfield runtime (§10 Q3).

**Related**: this document states the *promise*; the designs below own the
*how*. Every mechanism promised here has an owner in this table, or Phase 0
writes the missing design.

| Mechanism | Owning design |
|---|---|
| `PERK` record decode, `EsmIndex.perks` | [ESM Records](../engine/esm-records.md) |
| Entry-point composition, `PerkEntryTable`, the shipped hooks | [Scripting § Perk entry points](../engine/scripting.md#perk-entry-points-m472) |
| Perk fragments (FO4 `Fragment_Entry_NN` by `EPFB`, Skyrim's fragment layout by `EPF3` index, `perk.run`) | [Scripting § Perk fragments](../engine/scripting.md#perk-fragments-m472) |
| The surface the activate choice appears on (§10 Q5) | [Game UI Integration PRD](game-ui-integration.md) (surface S3) |
| Quest body: starting a quest and setting its stage | [Scripting § Quest stage runtime](../engine/scripting.md#quest-stage-runtime) |
| Ability body: the actor's `SpellList` and constant-spell application (`magic::add_spell`) | No design owns it at HEAD; Phase 0 adds the subsection to [Scripting § Perk entry points](../engine/scripting.md#perk-entry-points-m472) |
| Condition evaluation of `PRKC` tabs | [Scripting § Condition evaluator](../engine/scripting.md#condition-evaluator-m471) |
| `Perks`, the per-game dialect row, perk gates | [CHARAL](../engine/charal.md) |
| VATS hooks (Concentrated Fire, AP) | [CHARAL FO3/FNV ruleset § VATS](../engine/charal-fnv-fo3-ruleset.md#vats-tuning-action-points-regen-and-the-radiation-gauge--built-6350); the VATS values they modify: [VATS PRD](vats.md) (`VATS-PERK`) |
| Recording a hook the original shipped broken (§10 Q6) | [Built-in Fixes](../engine/builtin-fixes.md) |
| Fallout 4 perk chart (ranks, level gates) | [CHARAL FO4 ruleset § Perk chart](../engine/charal-fo4-ruleset.md#perk-chart--complete-7--7-special-columns-70-perks) |
| Crafting perks (`Mod Tempering Health`) | [Story Manager § Producer inventory](../engine/story-manager.md#64-producer-inventory-what-can-fire-today) |

**Sources**: wiki pages are cited by short name and read from the local
dumps under `/mnt/data/src/reference/`: *CK* = `ck-uesp-wiki/(main)/` (Skyrim
Creation Kit), *FO4 CK* = `falloutck-uesp-wiki/(main)/`, *GECK* =
`geck-uesp-wiki/(main)/`, *CS* = `cs-uesp-wiki/(main)/`, *UESP* =
`uesp-wiki/`; "CK *Perk*" is `ck-uesp-wiki/(main)/Perk.wiki`, and "the FO4
CK entry-point template" is `falloutck-uesp-wiki/Template/Perk Entry
Point.wiki`, which the FO4 CK's *Perk Entry Point* page includes. xEdit record
definitions are `Core/wbDefinitions<Game>.pas` on the TES5Edit `dev-4.1.6`
branch (<https://github.com/TES5Edit/TES5Edit/tree/dev-4.1.6/Core>), commit
`d4c0524`, fetched 2026-11-01; the copies read are under `/mnt/data/tmp/`.

Normative terms **MUST**, **SHOULD** and **MAY** have their RFC 2119
meanings, as in the related designs. Requirement IDs use the `PERK-` prefix.

---

## 1. Product thesis

A perk is the player's reward for levelling and most of what makes two
characters of the same level play differently. In every Fallout and in Skyrim
it is data: the Creation Kit says an entry point "allows you to modify a
specific, predefined part of the code" (CK *Perk*), and the GECK
describes each hook by the calculation it runs in ("Run when the player gains
any experience", GECK *Entry Point*). A perk the engine decodes but does not
apply is worse than one it ignores: the player spends a level on it and nothing
happens, with no message. The feature is for players of Fallout 3, New Vegas,
Skyrim and Fallout 4, and for the authors of perk mods, whose work is this same
entry-point data (Ordinator is in the
[conformance corpus](legacy-mod-compatibility.md#72-candidate-corpus)).

The decode half of M24.2 is mostly done at HEAD and the composition runtime
already exists for a handful of hooks (§3). What is left is the long tail, and
the promise that makes the tail tractable is this one:

**Every authored hook has exactly one stated outcome.** For each game, every
entry-point index the masters author is either composed by the calculation it
names, or listed with the reason it is not (no such calculation in the engine,
the original never ran it, a subject the engine cannot supply). No entry is
silently inert. The engine will not invent a calculation to give a hook a home,
and will not fire an entry whose conditions it cannot evaluate.

---

## 2. Terminology

| Term | Meaning |
|---|---|
| **Entry** | One `PRKE`…`PRKF` block of a `PERK` record: a header (type, rank, priority), a body and its condition tabs. `PerkEntry` in `crates/plugin/src/esm/records/misc/magic.rs`. |
| **Body** | The entry's payload, one of `PerkEntryBody::Quest`, `::Ability`, `::EntryPoint`. |
| **Hook** | An entry-point index in one dialect — the hard-coded calculation an `EntryPoint` body modifies. "Calculation hook" in [scripting.md](../engine/scripting.md#perk-entry-points-m472). |
| **Dialect** | The per-game entry-point numbering, `PerkEntryPointDialect` on the CHARAL profile row (`crates/core/src/character/profile.rs`). |
| **Function** | `DATA` byte 1 of an `EntryPoint` body: the math (Set, Add, Multiply, …) or a non-value action (Add Leveled List, Add Activate Choice, Select Spell, Select Text, Set Text). |
| **Value function** | Functions 1–7 and, on Skyrim and later, 12–14: the ones that produce a number from a number. Everything else is a **non-value function**. |
| **Parameter type** | `EPFT`, which selects the `EPFD` shape. Not the function (xEdit `wbDefinitionsTES5.pas`, `PERK`). |
| **Tab** | A `PRKC` condition group. Tab 0 is the perk owner; the others are subjects the calculation supplies (weapon, target, attacker, item, …). |
| **Subject** | The entity a tab's conditions run on. |
| **Composition** | `compose_entry_point`: the owner's held entries on one hook applied to a base value in priority order. |
| **Hook catalog** | The machine-readable table this PRD adds, one row per hook per dialect, carrying the hook's state (§3) and its tab subjects. |
| **Gate** | A condition on *taking* a perk, as opposed to on an entry firing: the record-level conditions, the `DATA` level and playable flag, the next-rank link. |

---

## 3. Core model

### 3.1 Hook states

Every hook in every in-scope dialect has exactly one state. The hook catalog
holds it and `perk.hooks` (PERK-DX-002) prints it.

| State | Meaning | Reported as |
|---|---|---|
| **Consumed** | A calculation in the engine composes the hook. | `consumed: <calculation>` |
| **Pending** | In this PRD's scope; the calculation exists or is built here, the composition call is not yet made. | `pending: <requirement id>` |
| **Blocked** | The calculation the hook modifies does not exist in the engine, and building it belongs to another milestone. | `blocked: <system>, <milestone or issue>` |
| **Inert** | The original engine does not run the hook (the GECK marks it "Not Implemented", the CK *Perk Entry Point* "Not Used"), or xEdit names it reserved or unused. | `inert: <source>` |

### 3.2 Entry fireability

An entry on a Consumed hook is, at a given call site, one of:

| Class | Meaning |
|---|---|
| **Fireable** | Every tab that carries conditions has a supplied subject, and every condition is one the M47.1 evaluator models. |
| **Subject-gated** | A conditioned tab has no subject at this call site (today: the weapon tab of every hook but the weapon-damage and Damage Threshold ones, and any weapon tab on a creature's natural attack). The entry does not fire. |
| **Unmodelled** | A condition function the evaluator does not model (`condition::is_modeled` is false). The entry does not fire. |

Both non-fireable classes are fail-closed today (`tabs_pass` in
`crates/scripting/src/perk_entry_points.rs`) and stay that way; this PRD adds
the reporting (PERK-DX-003) and shrinks the first class (PERK-SUB).

### 3.3 Inventory at HEAD

The inventory below is the state at HEAD on 2026-11-03, read from the code
cited (every row re-read in review round 2). It is hand-written and goes
stale; once Phase 0 checks in the hook catalog (PERK-HOOK-001), the hook rows
are generated from it and this table keeps only the record and body rows.

**Record and body decode** (`parse_perk`, `decode_perk_epfd` in
`crates/plugin/src/esm/records/misc/magic.rs`)

| # | Item | Today |
|---|---|---|
| D1 | `PRKE` header, three body shapes | Decoded on every game (`parse_perk`); ROADMAP's M24.2 row says the same |
| D2 | `EPFD` per parameter type | Decoded for FO3/FNV (types 1, 2, 3) and Skyrim (1, 2, 3, 4, 5, 7) by `decode_perk_epfd`; every other game returns `PerkFunctionData::None`. ROADMAP lists this as open; for FO3/FNV/Skyrim it shipped |
| D3 | Per-entry conditions by `PRKC` tab | Decoded into `PerkEntry.condition_tabs` (test `parse_perk_splits_conditions_by_prkc_tab`). ROADMAP lists this as open; it shipped |
| D4 | Record-level conditions (before the first `PRKE`) | Dropped: the `CTDA` arm pushes only inside an open entry block |
| D5 | `DATA` Level | Read and discarded (`let _level`) |
| D6 | `DATA` playable / hidden / trait | Decoded; no reader outside the parser and its tests |
| D7 | Quest body stage | Read as one byte (`r.u8_or_default() as u16`) |
| D8 | `NNAM` next perk | Not read |
| D9 | `EPF2` / `EPF3` | Kept as raw bytes (`formatter`, `extra_flags`), no reader |
| D10 | `EPFB` entry id (FO4) | Not stored on the entry; read only by `examples/perk_fragment_census.rs`. Fragments are keyed by the `VMAD` index |
| D11 | FO3/FNV embedded activate script | Not decoded by `parse_perk`; `embedded_script_at` (`crates/plugin/src/esm/records/script.rs`) already decodes the same sub-record layout for `QUST` stage and `INFO` result scripts |
| D12 | Starfield layout | Parsed with the FO3/FNV field order; the Starfield `DATA` and `PRRK` rank blocks differ (§5.1) |
| D13 | Skyrim `VMAD` perk fragments | Not decoded: `parse_perk` skips the `VMAD` on Skyrim, whose fragment section carries a file name and no script entry ([scripting.md § Perk fragments](../engine/scripting.md#perk-fragments-m472)) |

**Runtime**

| # | Item | Today |
|---|---|---|
| R1 | Composition | `PerkEntryTable` built from `EsmIndex.perks` per load order, not saved (`registry_completeness_tests.rs`, `PerkEntryTable` row); `compose_entry_point` per [scripting.md](../engine/scripting.md#perk-entry-points-m472) |
| R2 | Dialects | `Fallout3NV`, `Skyrim`; FO4, FO76 and Starfield rows carry `None` (`CharacterRulesProfile` in `profile.rs`), so their table is empty |
| R3 | Consumed hooks | Weapon damage (`combat.rs`, `compose_outgoing_damage`), kill XP hook 9 and skill points hook 10 (`combat.rs`), FNV Damage Threshold 56/58 (`compose_damage_threshold`), carry weight (`encumbrance.rs`), Skyrim buy/sell 8/60 (`barter.rs`), Skyrim tempering 76 (`crates/scripting/src/tempering.rs`) |
| R4 | Weapon and DT target tabs | Supplied since rev 6: `compose_outgoing_damage` and `compose_damage_threshold` lend the attacker's weapon to `WeaponSubject`, and FNV hooks 56/58 get their target tab. Every other hook's weapon tab is still unsupplied |
| R5 | Ability body | No consumer: `Effect::ChangePerk` only sets the rank (`fragment/effects/actor_lists.rs`) |
| R6 | Quest body | No consumer |
| R7 | Value-entry fragments (FO4) | Not reported by composition; `perk.run` is the only source (`fragment/perk.rs`) |
| R8 | Taking a perk | `levelup perk` checks perk points and `num_ranks` only ("Perk requirements are not checked here", `commands/gameplay.rs`, `LevelUpCommand`) |
| R9 | Concentrated Fire | `vats_queue_part` prices each repeat shot (#6422); `concentrated_shot: 1` at both scan sites prices a part's first shot (`systems/vats.rs`) |
| R10 | Composition trace | `compose_entry_point_traced` exists "for the debug console"; no command calls it |
| R11 | Hook indices in consumers | `combat.rs` passes `fnv::ADJUST_EXPERIENCE_POINTS` and `fnv::ADJUST_GAINED_SKILL_POINTS` directly; `barter.rs` takes its hooks from the profile row (`BarterModel::perk_hooks`) |

### 3.4 Dialect sizes

From the xEdit definitions (see **Sources**): FO3
`wbDefinitionsFO3.pas`, FNV `wbDefinitionsFNV.pas` (`wbPERKEntryPoints`),
Skyrim `wbDefinitionsTES5.pas` and FO4 `wbDefinitionsFO4.pas`
(`wbEntryPointsEnum`), Starfield `wbDefinitionsSF1.pas`.

| Dialect | Games | Hook indices | Numbering shared with |
|---|---|---|---|
| `Fallout3NV` | FO3 | 0–36 | FNV 0–36 |
| `Fallout3NV` | FNV | 0–73 (73 `Reserved`) | — |
| `Skyrim` | Skyrim LE / SE | 0–91 | — (0 is *Calculate Weapon Damage*, as FNV, but the tables diverge from 3) |
| new `Fallout4` | FO4 | 0–157 | — (0 is *Mod Breath Timer*) |
| — | Starfield | 0–254 (`wbEntryPointsEnum` in `wbDefinitionsSF1.pas`; runtime out of scope, §10 Q3) | — (0 is *Mod Breath Timer*, as FO4) |

---

## 4. Goals and non-goals

### 4.1 Goals

- **PERK-G-001 — Zero decode mismatches.** The census harness reports no
  shape mismatch on any in-scope master (§7.2, scenario 1).
- **PERK-G-002 — Every authored hook classified.** Every hook a vanilla master
  authors at least one entry for has a hook-catalog state other than Pending
  by the last phase.
- **PERK-G-003 — Every body acted on.** Quest and Ability entries take effect
  when their perk is held; no body type has zero consumers.
- **PERK-G-004 — Taking a perk honours its gates.** A perk the record says is
  unavailable cannot be taken through the engine's level-up path.
- **PERK-G-005 — Explainable.** For any actor and hook, one console command
  shows which entries applied, in what order, and why any did not.
- **PERK-G-006 — Fallout 4 joins.** FO4's dialect is decoded and composed with
  the same composer as FO3/FNV and Skyrim, with no FO4 branch in a consumer.

### 4.2 Non-goals

- **PERK-NG-001.** Oblivion. It has no `PERK` record (the record list in
  UESP *Oblivion Mod:Mod File Format* has none); its skill mastery perks are game-setting driven
  (`fPerkSneakAttack…Mult`, `iPerkBlockDisarmChance`, … in the CS wiki) and
  belong to the CHARAL Oblivion ruleset.
- **PERK-NG-002.** Building the calculation behind a Blocked hook (critical
  hits, limb damage, disposition, terminal hacking, karma, addiction). The hook
  is catalogued Blocked with the owning milestone; that milestone composes it.
  *Adjust Limb Damage* (FO3/FNV 6, the player's own limbs per GECK *Entry
  Point*) is owned by the VATS PRD's
  [VATS-PERK-006](vats.md#57-perk-hooks-vats-perk).
- **PERK-NG-003.** A level-up menu or perk-chart screen. Menus are M48; this
  PRD gates the data and the console path the menu will call.
- **PERK-NG-004.** The extender perk API (`GetNthEntryValue`,
  `SetNthPerkEntryValue1`, …). The SDK compatibility table owns extender
  functions ([LMC-SCR](legacy-mod-compatibility.md#54-scripts-lmc-scr)).
- **PERK-NG-005.** Bug-for-bug reproduction of hooks the original shipped
  broken, unless a built-in fix entry records the decision
  ([LMC-NG-009](legacy-mod-compatibility.md#42-non-goals)); see §10 Q6.
- **PERK-NG-006.** Fallout 76: no ESM stub and no cell loading
  ([game-compatibility.md](../engine/game-compatibility.md#fallout-76)), so no
  actor ever holds a perk.

---

## 5. Requirements by area

### 5.1 Decode residue (`PERK-DEC`)

- **PERK-DEC-001.** The record-level `CTDA` list (the conditions before the
  first `PRKE`, xEdit `wbConditions` on `PERK` in the FO3, FNV, TES5 and FO4
  definitions; Starfield's definition has none at record level, its
  conditions sit in each `PRRK` rank block) MUST be stored on `PerkRecord`,
  its FormIDs passed through `remap_fid` as the entry conditions' are. Today
  it is dropped (§3.3 D4).
- **PERK-DEC-002.** The `DATA` level byte ("Min Level" in xEdit FNV, "Level" in
  TES5 and FO4) MUST be stored on `PerkRecord`. Today it is discarded (D5).
- **PERK-DEC-003.** A Quest body's stage MUST be read as the 16-bit field xEdit
  declares (`Quest Stage`, `itU16`, in the FO3, FNV, TES5 and FO4
  definitions). Today one byte is read (D7); a test with stage 300 MUST round
  through `parse_perk` unchanged. **Measured exception** (`perk_body_census`, 2026-11-08):
  the FO3 and FNV vanilla bodies carry fill in the byte after the stage
  (`0xCD`, `0x61`, `0x6C`), so their 35 Quest entries read as 16-bit name no
  stage of their quest while the low byte names one in every case; Skyrim is
  the same (#6784). Only Fallout 4 and later read the high byte (104 of its
  Quest entries name a stage as 16-bit, up to 1610).
- **PERK-DEC-004.** `NNAM` ("Next Perk", xEdit TES5 and FO4) MUST be stored on
  `PerkRecord` as a FormID passed through `remap_fid`. Today it is not read
  (D8).
- **PERK-DEC-005.** `EPF3` MUST decode into typed flags: `Run Immediately` and
  `Replace Default` (a `u16` of flag bits in xEdit's TES5 and FO4 `EPF3`),
  plus Skyrim's `u16` `Fragment Index`; on FO3/FNV `EPF3` is a single `u16`
  `Run Immediately` boolean (xEdit FO3 and FNV). Today it is raw bytes (D9).
- **PERK-DEC-006.** On FO4, `decode_perk_epfd` MUST decode parameter types 1,
  2, 3, 4, 5, 7 and 8 with the shapes of xEdit's FO4 `EPFD` union, where type 2
  under functions 5 / 12 / 13 / 14, and type 8 (`AVIF`, which FO4's `EPFT` enum
  adds and Skyrim's lacks), are an `AVIF` FormID plus a float (FO4
  `wbEPFDDecider` maps type 2 under those functions to case 8; FO4
  `wbActorValue` is a FormID to `AVIF`, not Skyrim's float-encoded index).
  Today FO4 returns `None` (D2).
- **PERK-DEC-007.** On FO4, the entry's `EPFB` ("Perk Entry ID (unique)", `u16`)
  MUST be stored on the `EntryPoint` body. Today only the census example reads
  it (D10).
- **PERK-DEC-008.** On FO3/FNV, the embedded script of an activate entry MUST
  be decoded by `embedded_script_at` into the `ScriptRecord` the M47.3
  interpreter runs for result scripts. The block follows `EPF3` and is the
  sub-record set of a standalone script — `SCHR`, `SCDA`, `SCTX`, `SLSD` +
  `SCVR` locals, `SCRO` / `SCRV` references (xEdit `wbEmbeddedScriptPerk` in
  `wbDefinitionsFNV.pas`, `wbEmbeddedScript` in `wbDefinitionsFO3.pas`).
  Today `parse_perk` does not decode it (D11).
- **PERK-DEC-009.** On Starfield, `parse_perk` MUST NOT report `is_trait`,
  `num_ranks`, `playable` or `hidden` from the FO3/FNV byte order: Starfield's
  `DATA` is category, skill group, crew requirement, flags (Trait, Playable
  and Hidden are bits of the last byte), and its entries sit
  inside `PRRK`…`PRRF` rank blocks (xEdit `wbDefinitionsSF1.pas`, `PERK`).
  Today they are read positionally (D12). The fields MUST either decode from
  the Starfield layout or be absent.
- **PERK-DEC-010.** The census harness (`examples/perk_entry_census.rs`) MUST
  accept FO4's shapes (PERK-DEC-006), and its per-master `mismatches: N` line
  MUST read zero on `Fallout3.esm`, `FalloutNV.esm`, `Skyrim.esm` (LE and SE)
  and `Fallout4.esm`. On `Starfield.esm`, whose `EPFD` shapes and `PRRK` rank
  blocks this PRD does not decode (§10 Q3), the census MUST count those
  entries as undecoded, apart from mismatches. Today the line is printed, but
  its shape rule knows only the FO3/FNV and Skyrim shapes, so an FO4 or
  Starfield entry whose parameter type promises data (every one of them
  decodes to `None`, D2) counts as a mismatch.
- **PERK-DEC-011.** On Skyrim, the `VMAD` fragment section (xEdit TES5
  `wbVMADFragmentedPERK`: a version byte, a file name, then per fragment a
  `u32` index, one unknown byte, the script name and the fragment name) MUST
  decode into `PerkRecord.script_fragments`, each keyed by the index that an
  activate entry's `EPF3` `Fragment Index` (PERK-DEC-005) names. The CK
  places these fragments on activate entries ("which is where these
  fragments reside", CK *Perk Fragments*). xEdit names both fields
  "Fragment Index" and states no link between them; **Phase 0: verify** that
  every fragment's index equals one activate entry's `EPF3` index, by a
  `Skyrim.esm` census like `perk_fragment_census`'s FO4 `EPFB` match. Today
  they are not decoded (D13).
- **PERK-DEC-012.** `EPF2` MUST decode into the activate choice's button label:
  an lstring on Skyrim and FO4, a string on FO3/FNV (xEdit TES5 and FO4
  `wbLString(EPF2, 'Button Label')`, FO3 and FNV `wbStringKC`). Today it is
  raw bytes (D9).

### 5.2 Dialect and translation boundary (`PERK-DIA`)

- **PERK-DIA-001.** `PerkEntryPointDialect` MUST gain a `Fallout4` variant set
  on `CharacterRulesProfile::FALLOUT4`, with the numbering of xEdit's FO4
  `wbEntryPointsEnum` (§3.4). Today FO4's row is `None` (R2).
- **PERK-DIA-002.** A consumer outside `perk_entry_points` MUST NOT name a
  per-dialect hook index: it names the calculation, and the dialect maps it to
  an index in one table per dialect (the `BarterModel::perk_hooks` row is the
  existing shape). Today `combat.rs` passes `fnv::` constants (R11). This is
  the translation boundary of CHARAL: the per-game difference is the dialect
  row, never a branch in combat, encumbrance or barter.
- **PERK-DIA-003.** The value-function set MUST be a property of the dialect
  (FO3/FNV 1–7; Skyrim and FO4 1–7 and 12–14, per the function enums of their
  xEdit definitions). Today `resolve_op` gates 12–14 on `Skyrim` alone.
- **PERK-DIA-004.** The composition order MUST be a property of the dialect.
  The FO3/FNV and Skyrim order (ascending priority, ties by perk order then
  entry order) is decided in [scripting.md](../engine/scripting.md#perk-entry-points-m472)
  and is not reopened. The FO4 CK says "a perk entry with a lower numbered
  priority will take precedence"
  (FO4 CK *Perk*),
  the opposite of Skyrim's CK text. **Phase 0: verify** whether any
  `Fallout4.esm` hook carries a Set Value entry and another entry at a
  different priority on the same hook, which is the only case where the order
  is observable; if none does, §10 Q2 decides.
- **PERK-DIA-005.** The FNV subtract-table hooks (53, 56, 58) MUST stay a
  property of the `Fallout3NV` dialect, and the `Fallout4` dialect MUST have
  none: xEdit's FO4 definition declares one function enum and no per-hook
  label table (FNV's is `wbPERKFunctionNames`, whose `epftSubtract` row reads
  "Subtract Value" for function 1, selected per hook by the `FunctionTable`
  column of `wbPERKEntryPoints`).

### 5.3 Condition subjects (`PERK-SUB`)

- **PERK-SUB-001.** Each Consumed hook's catalog row MUST list the subject of
  every tab above 0. For FO3 and FNV the source is xEdit's `wbPERKEntryPoints`
  condition column (`epcWeaponTarget`, `epcAttackerAttackerWeapon`, …, with
  the tab captions of `wbPERKCondition`); `wbDefinitionsFO3.pas` declares the
  same table for hooks 0–36, identical to FNV's rows 0–36. For Skyrim and FO4
  xEdit has no such table, so **Phase 0: verify** each consumed hook's tab
  subjects against the CK / FO4 CK entry-point pages and the census's per-tab
  condition functions (`CENSUS_ENTRY_POINT=<n>`).
- **PERK-SUB-002.** On the weapon-damage and Damage Threshold hooks, the
  attacker's equipped weapon MUST be supplied as the weapon tab's subject,
  and on the Damage Threshold hooks 56 and 58 the target tab's subject MUST
  be supplied as well (xEdit FNV `wbPERKEntryPoints` gives both
  `epcWeaponTarget`: Perk Owner, Weapon, Target). Hook 56's owner is the actor
  hit, and its Target tab names the attacker: `CENSUS_ENTRY_POINT=56` on
  `FalloutNV.esm` lists two tab-2 entries, Stonewall ("against all Melee Weapons
  and Unarmed attacks") and Hit the Deck ("against any and all Explosives"), both
  `IsWeaponSkillType`, which only reads as the attack if Target is the attacker. A
  bare-handed actor's weapon is the `Fists` record (`0x1F4`, skill 45 Unarmed, in
  `Fallout3.esm` and `FalloutNV.esm`); that the original lends it is inferred from
  the record and those descriptions, not from a source that says so. A creature's
  natural attack has no weapon form, so its weapon tabs are Subject-gated. The
  mechanism is §10 Q4. Shipped in rev 6 (R4).
- **PERK-SUB-003.** A weapon subject MUST answer the weapon-form conditions the
  census finds on weapon tabs (Phase 0 lists them) and MUST be Unmodelled for
  any condition on per-instance weapon state it cannot answer, never a guessed
  value.
- **PERK-SUB-004.** Unmodelled conditions MUST continue to keep an entry from
  firing (`tabs_pass`), as decided in [scripting.md](../engine/scripting.md#perk-entry-points-m472).

### 5.4 Hooks (`PERK-HOOK`)

- **PERK-HOOK-001.** The hook catalog MUST exist as data in
  `crates/scripting/src/perk_entry_points`, one row per hook index per
  in-scope dialect, carrying the xEdit name, the §3.1 state, the consuming
  call site or the reason, and the tab subjects. A unit test MUST assert that
  every hook constant in the module has a row, and every Consumed row names
  its call site as a `path::function` whose source a scan test finds calling
  a `compose_*` function.
- **PERK-HOOK-002.** The GECK's "Not Implemented" hooks (Adjust Range Penalty,
  Calculate Inventory AP Cost, Calculate Weapon Range, Get Bad Karma, Get Good
  Karma, Get Should Assist, Get Should Attack — GECK *Entry Point*)
  MUST be catalogued Inert on `Fallout3NV`, with that page as the source.
- **PERK-HOOK-003.** FO3/FNV *Has Concentrated Fire* (33) MUST be composed from
  0 when VATS opens; when the result is non-zero, the VATS scan MUST pass the
  queued shot's repeat count as `concentrated_shot` instead of 1 (GECK: "If
  the value is modified to be non-zero, the player will be able to use
  concentrated fire"; *VATS Settings* `iVATSConcentratedFireBonus`). What
  counts as the same target part for the repeat count is the VATS PRD's
  [VATS-PERK-001](vats.md#57-perk-hooks-vats-perk), not this ID. Today
  `has_concentrated_fire` composes the hook and `vats_queue_part` prices each repeat
  shot at the same part (#6422); the scan still prices a part's first shot.
- **PERK-HOOK-004.** FO3/FNV *Player Kill AP Reward* (35) MUST compose the AP
  restored when a VATS sequence ends with at least one kill, from a base of 0
  (GECK: "Modifies the AP reward from 0 to another value"). Its tabs 1 and 2
  are weapon and target (xEdit FNV `epcWeaponTarget`), so weapon-conditioned
  entries stay Subject-gated until the hook gets PERK-SUB-002's weapon subject.
- **PERK-HOOK-005.** FNV *Action Point Regen* (39) and *Action Point Cost* (40)
  SHOULD compose the AP regeneration and the VATS AP cost, which the VATS
  PRD consumes ([VATS-PERK-004 and -005](vats.md#57-perk-hooks-vats-perk)).
  *Calculate Weapon Attack AP Cost* (3) is not the VATS cost: the GECK runs it
  "when the player attacks outside of VATS", and it is catalogued on its own
  row. The GECK leaves the descriptions of 39 and 40 blank; **Phase 0:
  verify** what value each composes (rate or multiplier; per shot or per
  attack) from a vanilla entry's function and operand in the census and a perk
  description that states the effect. Both carry a weapon tab (xEdit FNV
  `epcWeapon`).
- **PERK-HOOK-006.** *Add Leveled List On Death* (FO3/FNV 21, Skyrim 9, FO4 9)
  MUST add the entry's leveled list to the victim's inventory when the
  perk owner kills it, without running the inventory-add script events (GECK:
  "Adds to the inventory of the target without causing OnAdd blocks to run").
- **PERK-HOOK-007.** *Activate* (FO3/FNV 27, Skyrim 14, FO4 14) with
  *Add Activate Choice* MUST offer one extra choice labelled by `EPF2`
  (PERK-DEC-012) when the player activates a reference whose tabs pass; with
  *Run Immediately* it runs without a choice, with *Replace Default* the
  default activation does not run (CK *Perk Entry Point*, *Activate*). Only
  the player's activations run these entries (GECK *Entry Point*: "Run when
  the player activates something"; CK *Perk Fragments*: "NPCs won't run
  activate entry point perk entries"). On FO4 the sources disagree on *Run
  Immediately*: the FO4 CK's entry-point template repeats "run in addition to
  the default activation", its *Perk Fragments* page says the fragment "is
  run every frame conditions are met"; **Phase 0: verify** which, from what
  the vanilla Run Immediately fragments (the FO4 activate fragments
  `perk_fragment_census` lists) do with `akTargetRef`, or, if their bodies
  do not settle it, an in-game count of how often one runs per activation.
  The surface is §10 Q5.
- **PERK-HOOK-008.** When a composition applies an FO4 entry whose `EPFB` has a
  bound fragment, the composer MUST report the run (`report_perk_entry`), per
  the CK's "Value entries: Run whenever conditions are met"
  (FO4 CK *Perk Fragments*).
  Today composition never reports (R7).
- **PERK-HOOK-009.** Skyrim's spell-applying hooks (*Apply Combat Hit Spell*,
  *Apply Bashing Spell*, *Apply Sneaking Spell*, …) SHOULD cast the entry's
  spell on the subject the CK names when the triggering event happens, through
  `magic_effects::cast_spell` (`crates/scripting/src/magic_effects.rs`). Each
  hook is its own catalog row; one not built is Pending with this ID.
- **PERK-HOOK-010.** The FO4 dialect's carry-weight (*Set Max Carry Weight*,
  10) and weapon-damage hooks MUST be composed at the same call sites as the
  other dialects' (`encumbrance.rs`, `combat.rs`) through PERK-DIA-002.
  **Phase 0: verify** which FO4 hook is the outgoing weapon-damage
  calculation: xEdit FO4 names 35 *Mod Weapon Attack Damage* and 93 *Mod
  Typed Weapon Attack Damage*, and the FO4 CK entry-point template leaves
  both descriptions blank, so the census's per-hook entries and their perk
  descriptions settle it.
- **PERK-HOOK-011.** Skyrim's *Adjust Limb Damage* (4) and *Calculate Mine
  Explode Chance* (3), which the CK *Perk Entry Point* marks "Not Used", MUST
  be catalogued Inert on `Skyrim`, with that page as the source.
- **PERK-HOOK-012.** Picking an activate choice, or a *Run Immediately* entry
  firing, MUST run what the entry carries: on FO4 the fragment bound to its
  `EPFB` (`report_perk_entry`); on Skyrim the fragment its `EPF3` index binds
  (PERK-DEC-011), reported the same way; on Skyrim and FO4 the `EPFT` 4
  spell, cast on the activated reference ("The contact spell to apply to the
  target", CK *Perk Entry Point*, *Activate*); on FO3/FNV the embedded
  script (PERK-DEC-008). Today none of the four runs from an activation; only
  `perk.run` reports an FO4 entry (R7).

### 5.5 Quest and Ability bodies (`PERK-BODY`)

- **PERK-BODY-001.** While an actor holds a perk at the rank an Ability entry
  is authored for, the entry's spell MUST be in the actor's spell list
  (`magic::add_spell`, which applies a constant spell's value changes), and
  MUST be removed when the perk is removed ("Spells and Abilities added in
  this way will be removed when the Perk is removed", CK *Perk*). Today
  `Effect::ChangePerk` only sets the rank (R5). The rank rule is sourced on
  FO3/FNV ("If a specific perk entry is applicable at multiple ranks, it must
  be specified multiple times, once for each rank", GECK *Perk Entry*) and on
  FO4 ("If an entry is applicable at multiple ranks, it must be specified for
  each rank", FO4 CK *Perk*). The Skyrim CK marks entry Rank "Not used", so
  **Phase 0: verify** the Skyrim rule from a `Skyrim.esm` perk carrying
  Ability entries at more than one rank (the census's multi-rank listing).
- **PERK-BODY-002.** After a save and reload, a perk-granted ability spell MUST
  be in `SpellList` exactly once, with its value changes counted once in the
  reloaded `ActorValues` (both columns are saved at HEAD,
  `byroredux/src/save_io/registry.rs`). One round-trip test MUST assert both.
  The mechanism (derived from `Perks` or stored like `AddSpell`) is §10 Q7.
- **PERK-BODY-003.** When an actor gains a perk with a Quest entry, the quest
  MUST be started and set to the entry's stage through the canonical quest
  lifecycle ("A Quest Perk Entry will start a specified quest and set it to a
  specified stage", CK *Perk*).
  Today there is no consumer (R6). **Phase 0: verify** whether it fires once at
  acquisition or at every rank, and whether the entry's tabs gate it; none of
  CK *Perk*, GECK *Perk Entry* or FO4 CK *Perk* says, so the settling
  evidence is an extender source that applies the entry (xNVSE or SKSE
  perk-entry visitors) or a vanilla quest whose stage is only ever set by a
  perk entry.
- **PERK-BODY-004.** Perks granted from `NPC_` (`PRKR`, stamped by
  `npc_spawn/stamps.rs`; xEdit declares `PRKR` in the TES5 and FO4 `NPC_`
  definitions and in neither FO3's nor FNV's, and the parser gates it off
  there) MUST receive their Ability entries the same way as the player's.

### 5.6 Taking a perk (`PERK-GATE`)

- **PERK-GATE-001.** `levelup perk` MUST refuse a perk whose record-level
  conditions (PERK-DEC-001) fail on the player, naming the failing condition
  function. CK: "Sets the conditions necessary to make this perk available";
  FO4 CK: "When conditions are met this perk may be available during level up".
  Today requirements are not checked (R8).
- **PERK-GATE-002.** `levelup perk` MUST refuse a perk whose `DATA` level
  exceeds the player's level (FO4 CK: "The minimum player level where this
  perk becomes available"). On Skyrim the CK marks Level "Not applicable";
  **Phase 0: verify** that every `Skyrim.esm` perk authors 0 there before the
  check applies to Skyrim.
- **PERK-GATE-003.** `levelup perk` MUST refuse a perk whose `DATA` playable
  flag is clear (CK: playable "identifies … that the perk can be chosen by the
  player when leveling up").
- **PERK-GATE-004.** Where a perk authors `NNAM`, taking the next rank MUST
  grant the `NNAM` perk rather than raising the held perk's rank. **Phase 0:
  verify**, by census of `Skyrim.esm` and `Fallout4.esm`, how `NNAM` chains
  and `num_ranks` above 1 coexist (the Skyrim CK marks Num Ranks "Not
  applicable" and entry Rank "Not used"; the FO4 CK documents both Num Ranks
  and Next Perk).
- **PERK-GATE-005.** Script and console grants (`AddPerk`, `Effect::ChangePerk`)
  MUST NOT be gated: the record's conditions are "Level-Up Menu Conditions"
  (FO4 CK *Perk*), and neither GECK *AddPerk* nor CK *AddPerk - Actor*
  describes a check. Today `Effect::ChangePerk` checks nothing (R5), which
  this ID keeps.

### 5.7 Observability (`PERK-DX`)

- **PERK-DX-001.** A console command `perk.compose <hook> <base> [entity]` MUST
  print the composed value and, per applied entry, the perk, the operation and
  the running value (`compose_entry_point_traced`). Today no command calls the
  traced composer (R10).
- **PERK-DX-002.** A console command `perk.hooks [state]` MUST print the hook
  catalog for the loaded dialect, one line per hook with its state.
- **PERK-DX-003.** `perk.compose` MUST also list each held entry on the hook
  that did not apply, with its class (§3.2) and, for Unmodelled, the condition
  function index.
- **PERK-DX-004.** At load-order install, the engine MUST log one line naming
  the loaded dialect with the counts of entries composed, entries on
  non-Consumed hooks by state, and entries skipped for an undecoded shape.
  Today it logs only `Installed perk entry points: N perks`, and nothing when
  N is 0 (`byroredux/src/asset_provider/script.rs`).
- **PERK-DX-005.** On save load, a held perk whose FormID has no `PERK` record
  in the load order MUST be reported once and contribute nothing. Today it
  already contributes nothing (`compose_entry_point_traced` skips a FormID
  with no table row) and nothing reports it. (The GECK records that Fallout 3
  before 1.5 crashed loading such a save, GECK *Perk Entry*, CAUTION.)
- **PERK-DX-006.** `SpellList` MUST be registered for the debug server's
  `inspect` command, so a perk-granted ability is visible on the actor. Today
  `crates/debug-server/src/registration.rs` registers `Perks` and not
  `SpellList`.

---

## 6. Deliberate deviations and risks

| # | Original behaviour | Redux behaviour | Becomes redundant | Risk and its requirement |
|---|---|---|---|---|
| 1 | The original evaluates every perk condition ("The conditions under which the Perk is effective", CK *Perk*) | A condition the evaluator does not model keeps the entry from firing (PERK-SUB-004) | — | A perk silently weaker than authored → reported per entry (PERK-DX-003) and counted (§8 fireability) |
| 2 | The weapon tab runs on "the weapon being used (if any)" (GECK *Entry Point*) | Runs on a subject that answers weapon-form conditions (PERK-SUB-002/003) | — | Per-instance conditions (mods, condition) answer Unmodelled, not guessed → PERK-SUB-003 |
| 3 | Skyrim *Mod Attack Damage* (35) and *Calculate Weapon Damage* (0) order is not documented | 35 then 0, as shipped (scripting.md) | — | None for vanilla (all multiplicative there); a mod mixing Set and Multiply across them could differ → visible in PERK-DX-001 |
| 4 | Fallout 3 before 1.5 crashed on a save holding a perk from a removed plugin (GECK *Perk Entry*) | The perk is reported and contributes nothing (PERK-DX-005) | Advice never to uninstall a perk mod mid-game | The `Perks` entry stays in the save; re-adding the plugin restores it, which is intended |
| 5 | FO3 *Calculate To Hit Chance* (8) "Broken as of 1.1.0035 patch" (GECK *Entry Point*) | Per §10 Q6 | Any mod that works around the broken hook | A perk that did nothing in the shipped game starts working → Q6 records it as a built-in fix entry or keeps it Inert; the VATS PRD's VATS-PERK-003 shows the result |
| 6 | Ability spells live in the actor's spell list ("the Ability is added to the player's 'spell list'", GECK *Perk Entry*) | Derived from `Perks` and reconciled, if §10 Q7 takes its recommendation (PERK-BODY-002) | — | A script that `RemoveSpell`s a perk's ability sees it return on the next perk change → **Phase 0: verify** whether any vanilla script removes a perk-granted ability |
| 7 | *Add Range To Value* adds `Random(X, Y)`, which the CK *Perk Entry Point* links to *RandomFloat - Utility* ("between the minimum and maximum values (inclusive)"); the distribution is not documented (**Phase 0: verify** by sampling the composed value of a test perk on a Consumed hook in the original game) | Draws uniformly from the seeded `StoryManagerRng` | — | A distribution that differs from the original's shifts averages, not ranges → visible in PERK-DX-001 |
| 8 | Level-up availability enforced by the menu ("Level-Up Menu Conditions: When conditions are met this perk may be available during level up", FO4 CK *Perk*) | Enforced by the level-up path the menu will call (PERK-GATE) | — | None; script grants stay ungated (PERK-GATE-005) |
| 9 | The activate choice appears in "the popup menu" (CK *Perk Fragments*) | Appears on an engine-native surface until M48's menus, per §10 Q5 | — | It does not look like the original's; the label and what picking it runs are the authored ones (PERK-HOOK-007, PERK-HOOK-012) |

---

## 7. Per-game scope and acceptance

### 7.1 Per-game scope

Waves follow the engine tiers in
[game-compatibility.md](../engine/game-compatibility.md#per-game-support-detail)
and the dialects that exist at HEAD.

| Game | In scope | Why / wave |
|---|---|---|
| Oblivion | n/a | No `PERK` record (PERK-NG-001) |
| Fallout 3 | yes | Wave 1. `Fallout3NV` dialect, hooks 0–36; Tier 1 engine support |
| Fallout: New Vegas | yes | Wave 1. Reference title; the shipped hooks were derived on it |
| Skyrim (LE / SE / AE) | yes | Wave 1. `Skyrim` dialect; one xEdit `PERK` definition serves LE and SE (no SSE branch in `wbDefinitionsTES5.pas`'s `PERK`), so LE is counted with SE |
| Fallout 4 | yes | Wave 2. Needs the decode (PERK-DEC-006/007) and the dialect (PERK-DIA-001) first |
| Fallout 76 | no | PERK-NG-006 |
| Starfield | decode only | PERK-DEC-009 keeps the record honest; the runtime waits on the CHARAL Starfield ruleset ([charal.md §8](../engine/charal.md#8-rollout-order), item 8) and §10 Q3 |

### 7.2 Acceptance

Data: the vanilla masters of each in-scope game, installed locally and read
from the directories `crates/plugin/src/esm/test_paths.rs` resolves
(`FO3_ENV`, `FNV_ENV`, `SKYRIM_SE_ENV`, `FO4_ENV`, `STARFIELD_ENV`; the
Skyrim LE master, which has no variable there, is passed to the census by
path). No game file is checked in. The scenarios:

1. **Decode census.** `cargo run --release -p byroredux-plugin --example
   perk_entry_census -- <ESM>` reports `mismatches: 0` on `Fallout3.esm`,
   `FalloutNV.esm`, `Skyrim.esm` (SE and LE) and `Fallout4.esm`, and on
   `Starfield.esm` reports its undecoded entries apart from mismatches
   (PERK-DEC-010). Each run's per-hook entry counts are the input to the hook
   catalog.
2. **Unit composition.** `cargo test -p byroredux-scripting perk_entry_points`
   passes, including the existing `only_the_current_ranks_entry_applies`,
   `condition_tabs_run_on_their_own_subject` and
   `an_unmodelled_condition_keeps_the_entry_from_firing`, and new tests named
   `fo4_dialect_composes_avif_mult`, `weapon_tab_subject_answers_weapon_form`
   and `ability_entry_spell_follows_the_perk`.
3. **Parse.** `cargo test -p byroredux-plugin parse_perk` passes, including new
   `parse_perk_keeps_record_conditions`, `parse_perk_quest_stage_is_u16`,
   `parse_perk_reads_next_perk`, `parse_perk_fo4_epfd_shapes` and
   `parse_perk_skyrim_fragments_key_on_epf3`.
4. **Real-data composition.** The ignored
   `vanilla_fnv_perks_compose_as_their_descriptions_say` and
   `vanilla_fnv_damage_threshold_perks_compose` (`crates/scripting/tests/perk_entry_points_real_data.rs`)
   pass with `FalloutNV.esm`, and a new weapon-tab leg composes
   `DemolitionExpert` on an explosive weapon. New Skyrim and FO4 siblings in the
   same file pass with their masters.
5. **Hook catalog.** The PERK-HOOK-001 test passes, and `perk.hooks` on a
   `--bench-hold` FNV session lists no hook authored by `FalloutNV.esm` as
   Pending at the end of Phase 3.
6. **Live, new script `perk-entry-points.sh`** (written in Phase 2, indexed in
   [docs/smoke-tests/README.md](../smoke-tests/README.md)): FNV Goodsprings,
   `levelup xp` to the first perk level, `levelup perk` on a perk whose
   record conditions fail (refused, failing function named), then on Swift
   Learner (accepted), then `perk.compose 9 100 player` shows the Swift Learner
   entry applied. A Skyrim leg takes a perk with an Ability entry and
   `inspect` on the player shows the spell in `SpellList` once, before and
   after F5 / F9 (PERK-DX-006).
7. **Existing gates stay green.** `m43-tempering.sh` (Skyrim hook 76) and
   `vats-targeting.sh` (extended in Phase 3 with a Concentrated Fire leg: the
   second queued shot at one limb shows the bonus).
8. **Activate entries** (Phase 3, FO4 leg in Phase 4). A new unit test
   `activate_choice_runs_its_entry` in `perk_entry_points` passes, and an
   activate leg of `perk-entry-points.sh` on FNV takes a perk the Phase 0
   census lists on hook 27, activates a reference its tabs accept, sees one
   extra choice with the entry's `EPF2` label on §10 Q5's surface, and picks
   it; the log shows the embedded script ran (PERK-HOOK-007, PERK-HOOK-012).

---

## 8. Metrics

The numbers live in ROADMAP's [M24.2 row](../../ROADMAP.md#tier-6--engine-infrastructure)
until the PRD ships, then in the compatibility matrix's *Verified content*
column ([ROADMAP](../../ROADMAP.md#compatibility-matrix)).

| Metric | Definition | Granularity |
|---|---|---|
| Decode mismatches | census shape mismatches | per master |
| Hook coverage | vanilla entry-point entries on Consumed hooks ÷ vanilla entry-point entries on non-Inert hooks | per game |
| Fireability | entries on Consumed hooks that are Fireable at their call site ÷ entries on Consumed hooks | per game |
| Classification | hooks with a vanilla entry and a state other than Pending ÷ hooks with a vanilla entry | per dialect |
| Body coverage | body types with a consumer ÷ body types authored | per game |

---

## 9. Delivery sequence

Phases are ordered by what unblocks the most authored entries on the
reference title. Phases 1 and 2 depend only on HEAD. Phase 3's VATS legs
compose at the VATS scan that exists at HEAD (`concentrated_shot` in
`byroredux/src/systems/vats.rs`); the repeat-count grouping waits on the VATS
PRD's VATS-PERK-001 verify item, and that PRD's Phase 5 consumes the results.
Phase 4 needs no other milestone: the CHARAL FO4 ruleset and the
`CharacterRulesProfile::FALLOUT4` row exist at HEAD
([charal.md §8](../engine/charal.md#8-rollout-order), item 3, struck as
shipped; `crates/core/src/character/profile.rs`), and only this PRD's
PERK-DEC-006/007 and PERK-DIA-001 stand before it.

**Phase 0 — Census and catalog (no engine code).** Run the census on every
master in §7.2 scenario 1 and draft the hook catalog from its output, each
authored hook with a state and a source. Close the **Phase 0: verify** items
(one row each in the verify list):

| Item | Requirement |
|---|---|
| Skyrim fragment index ↔ `EPF3` index | PERK-DEC-011 |
| FO4 priority case | PERK-DIA-004 |
| Per-hook tab subjects (Skyrim, FO4) | PERK-SUB-001 |
| Hook 56's Target tab (settled in rev 6: the attacker) | PERK-SUB-002 |
| FNV hooks 39 / 40 | PERK-HOOK-005 |
| FO4 *Run Immediately* timing | PERK-HOOK-007 |
| FO4 weapon-damage hook | PERK-HOOK-010 |
| Skyrim Ability rank rule | PERK-BODY-001 |
| Quest entry timing | PERK-BODY-003 |
| Skyrim's level byte | PERK-GATE-002 |
| `NNAM` versus `num_ranks` | PERK-GATE-004 |
| Scripted removal of perk abilities | §6 row 6 |
| *Add Range To Value* distribution | §6 row 7 |
| FO3/FNV hook-8 status and fix source | §10 Q6 |

Write the Ability-grant subsection of scripting.md (Related). Resolve §10.
Exit: the catalog draft and the verify answers are checked in, every item
closed or filed as an issue.

**Phase 1 — Decode residue.** PERK-DEC-001 to -012. Exit: §7.2 scenarios 1
and 3 pass on every master scenario 1 names.

**Phase 2 — Bodies, gates and observability.** PERK-BODY, PERK-GATE,
PERK-DX, PERK-HOOK-001/002/011. Exit: §7.2 scenarios 2 and 6 pass; the
catalog test passes.

**Measured answers, found in Phase 2** (`perk_body_census`, 2026-11-08, vanilla Fallout3/FalloutNV/Skyrim/Fallout4 masters;
these close the Phase 0 verify rows they name):

- *Skyrim Ability rank rule* (PERK-BODY-001): every Skyrim Ability entry is authored at rank 0 and no Ability
  perk has `num_ranks` above 1, so the shared "entry rank == held rank − 1" rule and the CK's "Rank: not used"
  agree on every vanilla perk. FO3 has 8 perks with Ability entries at ranks 0/1/2, FNV 2 (`ActionBoy`,
  `ActionGirl`): the per-rank rule is what they need.
- *Quest entry timing* (PERK-BODY-003): fires on acquiring the entry's rank, one entry per rank
  (`IntenseTraining` authors ranks 0-9 with stages 101-110 of one quest). Every vanilla Quest entry has zero
  condition tabs, so whether tabs gate it is unobservable; they are honoured (fail-closed). The stage byte of an
  FO3/FNV Quest body is a `u8` with fill after it (PERK-DEC-003 note).
- *Skyrim's level byte* (PERK-GATE-002): all 375 `Skyrim.esm` perks author level 0, so the level check applies
  to Skyrim without changing any vanilla perk.
- *`NNAM` versus `num_ranks`* (PERK-GATE-004): a chain's records each carry the total at their rank
  (`IronFist01..05` ×1.2…×2.0, `LightFingers00..80`), so the next rank replaces the held record. Skyrim's chains
  end at a record with no `NNAM`; Fallout 4's are rings whose wrap is the link to a lower-level record in 72 of
  79 rings (the other seven are bounded by "already held").

**Phase 3 — Subjects and the FO3/FNV/Skyrim hooks.** PERK-DIA-002/003,
PERK-SUB, PERK-HOOK-003 to -009 and -012. The activate legs need §10 Q5's
surface; on its recommendation that is the `MessageBox` page, which exists at
HEAD. Exit: §7.2 scenarios 4, 5, 7 and 8 pass; hook coverage and
fireability published for FO3, FNV and Skyrim.

**Phase 4 — Fallout 4.** PERK-DIA-001/004/005, PERK-HOOK-008/010 and the FO4
legs of PERK-HOOK-007/012. Exit: the FO4 siblings of scenarios 4 and 8 pass;
FO4 hook coverage published.

---

## 10. Decisions required

Each needs the project owner. The recommendation is the first option.

1. **Q1 — Scope: decode plus consumer, or decode only.** ROADMAP's M24.2 row
   names decode work only. Recommended: decode and runtime together, as this
   document does, because a decode without a reader is the anti-pattern
   ROADMAP tracks and most of M24.2's listed decode already shipped (§3.3 D2,
   D3); the ROADMAP row is rewritten to point here. Alternative: keep M24.2
   decode-only (PERK-DEC) and file the runtime under M47.2, where the composer
   lives; consequence: M47.2 is listed as completed (2026-10-12) in ROADMAP's
   Completed Milestones, so it would have to be reopened or a new milestone
   created; until then the decoded fields stay unread and Phase 0's catalog
   has no milestone to land in.
   **Decided** (2026-11-03): the recommendation — decode and runtime
   together, as this document does; ROADMAP's M24.2 row is rewritten to
   point here.
2. **Q2 — FO4 composition order, if Phase 0 finds no observable case.**
   Recommended: follow the FO4 CK text (lower number takes precedence, so the
   lower number applies last) as a property of the FO4 dialect (PERK-DIA-004);
   consequence: FO4 composes in descending priority, the reverse of FO3/FNV
   and Skyrim, and the order key becomes a dialect property. Alternative: one
   ascending order for every dialect; consequence: simpler, and wrong for FO4
   mods that rely on the documented direction.
   **Decided** (2026-11-03): the recommendation — if Phase 0 finds no
   observable case, FO4 follows the FO4 CK text (lower number applies last)
   as a property of the FO4 dialect (PERK-DIA-004).
3. **Q3 — Starfield.** Recommended: decode correctness of the `DATA` fields
   now (PERK-DEC-009), and Starfield's `EPFD` shapes, `PRRK` rank blocks and
   runtime when the CHARAL Starfield ruleset is buildable (charal.md §8, item
   8, still open); the census reports Starfield's entries as undecoded until
   then (PERK-DEC-010). Alternative: mark Starfield out of this PRD entirely;
   consequence: `PerkRecord` keeps reporting Starfield's trait, rank, playable
   and hidden fields from the wrong bytes until someone notices.
   **Decided** (2026-11-03): the recommendation — Starfield gets `DATA`
   decode correctness now (PERK-DEC-009); its `EPFD` shapes, `PRRK` rank
   blocks and runtime wait for the CHARAL Starfield ruleset, and the census
   reports its entries as undecoded until then (PERK-DEC-010).
4. **Q4 — The weapon subject.** Recommended: a scratch subject entity, created
   once at boot and lent the attacker's equipped weapon's base form for the
   length of one composition, as `TemperingSubject` is lent the item being
   tempered (`crates/scripting/src/tempering.rs`); consequence: conditions on
   the weapon's base form answer (the GECK *Entry Point*'s example is an
   `IsEnergyWeapon` check), per-instance ones (mods, condition) stay
   Unmodelled (PERK-SUB-003), and no two compositions may lend the same
   scratch entity at once. Alternative: make equipped weapons real entities;
   consequence: per-instance conditions become answerable, at the cost of an
   inventory and equip model change far outside this PRD.
   **Decided** (2026-11-03): the recommendation — a scratch subject entity
   created once at boot and lent the attacker's equipped weapon's base form
   for one composition, as `TemperingSubject` is lent the item being
   tempered.
5. **Q5 — Where the activate choice appears.** The CK *Perk Entry Point* (and
   the FO4 CK's entry-point template) adds the choice "in addition to the
   normal activation prompt", both CKs' *Perk Fragments* pages call it "the
   popup menu", and the GECK *Entry Point* says it "Presents a choice list".
   Recommended: the engine-native `MessageBox` page (`GameMenuPage`, surface
   S3 in the [Game UI Integration PRD](game-ui-integration.md)), which exists
   at HEAD and shows a title, text and authored buttons (`MessageBoxView`,
   `crates/debug-ui/src/panels/`): activating a reference with at least one
   passing choice opens it with the default action and one button per `EPF2`
   label, until M48's menus replace it; consequence: one surface for every
   game, drivable headless, at the price of an extra page where Skyrim and FO4
   show the choice on the prompt. Alternative A: an extra key on the native
   HUD's activation prompt (`draw_hud`), as the Skyrim CK describes;
   consequence: closer to Skyrim and FO4, but it needs a second activate
   binding, and `InteractionPrompt` must carry an authored label where today
   it holds only a compile-time verb. Alternative B: no choice surface; only
   Run Immediately entries fire, and FO4's fragments stay reachable through
   `perk.run`; consequence: FO3/FNV and Skyrim activate perks, and FO4's
   non-immediate ones, remain unusable in play.
   **Decided** (2026-11-03): the recommendation — the activate choice
   appears on the engine-native `MessageBox` page (surface S3) until M48's
   menus replace it.
6. **Q6 — FO3 *Calculate To Hit Chance* (8), broken in the shipped game.**
   The GECK says "Broken as of 1.1.0035 patch" and nothing more, so **Phase
   0: verify** what "broken" means (never runs, or runs with a wrong value)
   and whether FNV, which shares the `Fallout3NV` dialect, runs it, from a
   community fix that addresses it (the UFO3P / FOSE sources
   [builtin-fixes.md §3](../engine/builtin-fixes.md#3-fix-catalog) lists) and
   the census's vanilla hook-8 entries. Recommended: compose it on both games,
   and record it as an *engine behaviour* fix entry
   ([builtin-fixes.md §2](../engine/builtin-fixes.md#2-kinds-of-fix): "ordinary
   correct code plus a catalog entry and test"), in the fix-record shape of
   [§4](../engine/builtin-fixes.md#4-fix-record): a stable namespaced `id`
   that is never reused, `games` FO3 with the rules profile split, `kind
   engine`, `source` the GECK note plus whatever fix the verify item finds
   (FIX-003: no source, no fix), `test` the composition test (FIX-008), and
   listed in the fix report (FIX-009). Being composed, the hook needs no
   per-game branch (FIX-004), and LMC-NG-009's default of no bug-for-bug
   reproduction holds. The built-in fix catalog is PROPOSED with no entries
   built, so the hook catalog row cites the entry until that catalog exists; if
   no community fix is found, the entry is class `intent`, whose default is
   builtin-fixes.md's own open question
   ([§8](../engine/builtin-fixes.md#8-open-questions), item 2). Consequence:
   a hook-8 perk that did nothing in the shipped FO3 changes the VATS chance
   in Redux, and the fix report says so. The VATS PRD's VATS-PERK-003 shows
   the composed chance either way. Alternative: catalogue
   it Inert on FO3 only, matching the shipped game; consequence: a perk the
   GECK author meant to work never does, and the hook catalog must split the
   shared `Fallout3NV` row by rules profile.
   **Decided** (2026-11-03): the recommendation — compose FO3 *Calculate To
   Hit Chance* (8) on both games, recorded as an engine-behaviour fix entry
   in the builtin-fixes catalog.
7. **Q7 — How a perk's ability spell is held** (PERK-BODY-002). At HEAD
   `SpellList` is saved, and so is `ActorValues`, which already holds the
   value changes a constant spell made (`byroredux/src/save_io/registry.rs`);
   `magic::add_spell` and `remove_spell` are idempotent (a spell already
   present is neither added nor applied again); the actor's racial spells are
   the derived precedent (`RaceSpells`, re-derived at spawn and not saved,
   `crates/scripting/src/magic.rs`). Recommended: derived from `Perks`, by a
   reconcile through `add_spell` / `remove_spell` whenever `Perks` changes or
   a save loads, so the spell follows the perk on every path that adds or
   removes it, and a reload finds the spell already in the saved list and
   applies nothing twice; consequence: one reconcile pass per `Perks` change,
   and a spell that both a perk and a script's `AddSpell` granted leaves with
   the perk, because `SpellList` does not record who added a spell.
   Alternative: write it into the saved `SpellList` once at acquisition, like
   `AddSpell`; consequence: no reconcile pass, but a path that removes a perk
   and forgets the spell leaves it on the actor, and the CK's "removed when the
   Perk is removed" (CK *Perk*) then depends on every such path.
   **Decided** (2026-11-03): the recommendation — a perk's ability spell is
   derived from `Perks` by a reconcile through `add_spell` / `remove_spell`
   on every `Perks` change and save load.

---

## 11. Document map

| Need | Go to |
|---|---|
| How the composer works, the shipped hooks, priority and rank rules | [scripting.md § Perk entry points](../engine/scripting.md#perk-entry-points-m472) |
| FO4 perk fragments and `perk.run` | [scripting.md § Perk fragments](../engine/scripting.md#perk-fragments-m472) |
| Which condition functions are modelled | [scripting.md § Condition evaluator](../engine/scripting.md#condition-evaluator-m471) |
| `Perks`, the dialect row, perk gates in CHARAL | [charal.md](../engine/charal.md), [§7](../engine/charal.md#7-what-stays-out-of-scope), [§8](../engine/charal.md#8-rollout-order) |
| VATS model and Concentrated Fire | [charal-fnv-fo3-ruleset.md § VATS](../engine/charal-fnv-fo3-ruleset.md#vats-tuning-action-points-regen-and-the-radiation-gauge--built-6350) |
| FO4 perk chart | [charal-fo4-ruleset.md § Perk chart](../engine/charal-fo4-ruleset.md#perk-chart--complete-7--7-special-columns-70-perks) |
| `PERK` and the records tier | [esm-records.md](../engine/esm-records.md) |
| Tempering and crafting perks | [story-manager.md § 6.4](../engine/story-manager.md#64-producer-inventory-what-can-fire-today) |
| Engine tiers per game | [game-compatibility.md](../engine/game-compatibility.md#per-game-support-detail) |
| Smoke-test index | [docs/smoke-tests/README.md](../smoke-tests/README.md) |
| Bug-for-bug policy and the extender API | [legacy-mod-compatibility.md](legacy-mod-compatibility.md) |
| Built-in fix rules and the fix record (§10 Q6) | [builtin-fixes.md](../engine/builtin-fixes.md) |
| VATS values the perk hooks modify (`VATS-PERK`) | [vats.md](vats.md) |
| Native pages and menu surfaces (§10 Q5) | [game-ui-integration.md](game-ui-integration.md) |
| Live milestone state and published metrics | [ROADMAP.md](../../ROADMAP.md) |

---

## 12. Revision and review log

| Rev | Date | Change | Review |
|---|---|---|---|
| 1 | 2026-11-01 | First draft | — |
| 2 | 2026-11-01 | Review round 1 folded in: FO3 tab subjects from xEdit's FO3 `wbPERKEntryPoints`, FO4 `EPFT` 8, Starfield hook range 0–254, `wbPERKFunctionNames`, CK Num Ranks wording, FO4 CK rank rule, Phase 3/4 dependencies (CHARAL FO4 row shipped), Q6 on builtin-fixes §2/§4 and FIX-003/004/008/009, Q1 split into Q7, Related owners for both bodies, VATS PRD rev 2 cross-references (PERK-HOOK-003/005, VATS-PERK-001/004/005/006), §6 rows 2 and 7 sourced | claude: revise |
| 3 | 2026-11-03 | Review round 2 folded in: §3.3 re-read at HEAD (DT target tab unsupplied, `embedded_script_at`, Skyrim fragments D13); Skyrim perk fragments PERK-DEC-011 and the activate run path PERK-HOOK-012; `EPF2` split out as PERK-DEC-012; Skyrim "Not Used" hooks Inert PERK-HOOK-011; activate entries player-only and FO4 Run Immediately verify; PERK-DEC-008 and FO3/FNV ability-rank verify items closed from xEdit and GECK *Perk Entry*; DT target subject and hook-56 verify; census rule consistent with Starfield decode-only; HEAD states on DEC-010, DX-004, DX-005; §6 rows 1, 6, 7, 8 sourced, row 9 added, row 6 pointed at Q7; Phase 0 verify list as a table; Q1 (M47.2 closed), Q3, Q4 (shared scratch entity), Q5 (`MessageBox` page, sources), Q7 (saved `ActorValues`, idempotent `add_spell`) | claude: revise |
| 4 | 2026-11-03 | Owner decisions Q1–Q7 recorded, every one the recommendation; ROADMAP's M24.2 row now points here (Q1) | owner: approved |
| 5 | 2026-11-03 | → ACTIVE: parent #6635 and phase issues #6651, #6652, #6654, #6657, #6659 filed; the Phase 0 verify items are acceptance criteria of #6651 | — |
| 6 | 2026-11-05 | PERK-SUB-002 shipped ahead of Phases 0-2 at the owner's request, for the FNV perks that had no effect: hook 56's Target tab settled from the census, `IsWeaponSkillType` modeled from the newly decoded `DNAM` skill, R4 and §3.2 updated | claude: revise |
