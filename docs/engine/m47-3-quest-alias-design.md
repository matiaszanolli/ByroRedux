# M47.3 — Quest Alias System: ALST/ALLS decode + alias-fill runtime

**Status:** Phases 0–3 shipped. The parser preserves every alias block and
remaps its FormIDs; `SceneActorBindings` is the canonical live alias table.
The runtime fills forced-reference, unique-actor, loaded-candidate condition,
location-ref-type, linked-near, closest, external, and Force-Into aliases; enforces reuse,
reservation, Allow Reserved, Allow Dead, and quest start/stop lifetime; and
applies faction/inventory injections while exposing the complete alias record
and all injected metadata as source-attributed overlays. Permanent inventory
grants carry a save-persistent ledger, so loading and refreshing aliases cannot
duplicate authored `CNTO` stacks. VMAD object properties and
`RunOn::QuestAlias` consume the same table. Created-object spawning, Story Manager event fills,
true LCTN aliases, reference collections, and unloaded-world search remain
bounded follow-ups because their owning subsystems do not yet exist.

**Goal:** decode the `QUST` record's `ALST`/`ALLS` alias sections and build
the runtime that fills them with live references at quest start — the
mechanism Radiant Story quests use to target content dynamically ("kill
the bandit leader" without naming a specific NPC). This directly unblocks
three things already built and waiting:

1. **VMAD `ObjectRef::Property` alias resolution.** Alias-bound object
   properties resolve through the owning quest's live binding, enabling
   `AddItem`, `MoveTo`, `Activate`, `SetOpen`, and actor-targeted effects.
2. **`RunOn::QuestAlias` condition evaluation.** M47.1 resolves the alias id
   through the same table used by fragments, scenes, packages, and dialogue.
3. **Radiant/companion quest behavior generally** — the alias-injected
   packages/factions/spells/inventory are how a quest modifies an actor's
   behavior *without* touching its base record, and are the actual
   mechanism behind most companion and radiant (MQ/Companions/Thieves
   Guild-style) quest logic.

**Remaining subsystem boundary:**
- Find Matching evaluates CTDAs across loaded `SceneAliasCandidate`s. A true
  Story Manager search across unloaded world data requires a world-query
  service that is not present yet.
- `CreatedObject` requires the base-record spawn pipeline; `FromEvent`
  requires Story Manager event payloads; `ForcedLocation` requires an LCTN
  runtime. The parser and overlays preserve all inputs without fabricating
  entities.
- Packages, spells, keywords, names, voice types, and combat overrides remain
  exposed in `QuestAliasInjectedOverlays`; flags and the complete source alias
  are in `QuestAliasRuntimeOverlays`. Factions and inventory are the two
  metadata families with canonical mutable ECS components today.

---

## What's already built (the substrate)

| Piece | Where | State |
|---|---|---|
| QUST block-state-machine (`INDX`→stage, `QOBJ`→objective, `ALST`/`ALLS`→alias) | `crates/plugin/src/esm/records/misc/quest.rs::parse_qust` | done |
| CTDA condition parsing + `ConditionList` | `crates/plugin/src/esm/records/condition.rs`, M47.1 | done — reusable verbatim for `ALST`/`ALLS`'s "Match Conditions" |
| `RunOn::QuestAlias` | `crates/scripting/src/condition.rs` | **done** — resolves through `SceneActorBindings` |
| `resolve_entity_by_global_form_id` | `crates/scripting/src/condition.rs:326` | done — the FormID→EntityId resolver every "forced"/"unique actor" fill type needs, already load-bearing for M42.5–8 AI packages and tonight's M47.2 object-targeting effects |
| `FactionRanks` component | `crates/core/src/ecs/components/faction_ranks.rs` | done — direct target for alias-injected factions |
| `Inventory` component + `AddItem` fragment effect | `crates/core/src/ecs/components/inventory.rs`, M47.2 | done — direct target for alias-injected `CNTO` items |
| `ObjectRef::Property` alias resolution | `crates/scripting/src/fragment.rs` | **done** — alias-bound VMAD objects resolve to live entities |
| M42 AI packages (Follow/Escort/Guard/Travel/Sandbox/Wander/Patrol) | `byroredux/src/systems/{follow,escort,guard,travel,...}.rs` | done — the eventual consumer of alias-injected packages (Tier 7 `PACK` backlog), not touched by this milestone directly |

## The spec (verified against source, not guessed)

No spec was in-repo. Per the project's standing "no guessing" discipline,
the field table below is checked directly against xEdit's per-game record
definitions and then against real plugin bytes:

- <https://en.uesp.net/wiki/Skyrim_Mod:Mod_File_Format/QUST> (`ALST`/`ALLS`
  section)
- <https://github.com/TES5Edit/TES5Edit/blob/dev-4.1.5/Core/wbDefinitionsFO3.pas>
  (FO3/FNV layouts, including 32-bit `QOBJ`)
- <https://github.com/TES5Edit/TES5Edit/blob/dev-4.1.5/Core/wbDefinitionsTES5.pas>
  (Skyrim layouts and alias semantics)
- <https://github.com/TES5Edit/TES5Edit/blob/dev-4.1.5/Core/wbDefinitionsFO4.pas>
  (FO4 flags, collection aliases, and extension fields)

The parser's synthetic fixtures and the `qust_alias_survey`/
`qust_alias_rawdump` tools cross-validate this table against real
`Skyrim.esm`/`Fallout4.esm` subrecord streams. That empirical pass remains
important because the definitions give field identity and type, not a
byte-offset guarantee.

### Alias-block shape (one `ALST`/`ALLS` → next block or EOF)

```
ALST int32           — AliasID (Reference alias) — opens the block
  or ALLS int32       — AliasID (Location alias)  — opens the block
ALID zstring          — alias name ("Location", "QuestGiver", …)

── Fill type (mutually exclusive; the field PRESENT determines fill type) ──
ALUA formid           — Unique Actor:            NPC_ base record
ALCO formid           — Created Object:          base record to instantiate
ALEQ formid           — External Alias Reference: source QUST
  ALEA int32          —   companion field: the AliasID in that QUST
ALFE char[4]          — From Event:              SMEN short name
  ALFD int32          —   companion field: event data
ALFL formid           — Forced Location (ALLS only): fixed LCTN
ALFR formid           — Forced Reference (ALST only): fixed ACHR/REFR
ALFA int32            — Location/Reference Alias source alias
  KNAM formid         —   optional linked-reference keyword (KYWD)
  ALRT formid         —   optional reference type (LCRT)
ALNA int32            — Find Matching Reference Near Alias: source alias
  ALNT uint32         —   relationship type
(no fill field at all) — Find Matching Reference: CTDA-only, hardest case
CTDA* struct[32]      — Match Conditions (repeatable)
  CIS2 zstring        —   CTDA auxiliary variable name

── Fill-type companions ──
ALCA {int16,uint16}   — companion to ALCO: target alias + create mode
ALCL uint32           — companion to ALCO: encounter level

── Properties / injected data (any subset, any order per source) ──
FNAM int32            — flags (table below)
ALED empty            — block terminator ("always the final field")
VTCK formid           — additional valid voice type (NPC_ or FLST)
ALDN formid           — Display Name → MESG record
ALFC formid*          — injected Factions (FACT), repeatable
ALFI int32            — Force Into Alias target
ALCC int32            — choose closest to this alias
ALPC formid*          — injected Packages (PACK), repeatable
ALSP formid*          — injected Spells (SPEL), repeatable
COCT int32            — CNTO count (absent if zero)
CNTO struct[8]*       — injected inventory: {formid item, uint32 count}
ECOR formid           — Combat Override package list (FLST)
SPOR/OCOR/GWOR formid — spectator/dead-body/guard-warn override lists
ALLA {formid,int32}*  — FO4 linked keyword/alias pairs
KSIZ uint32           — KWDA count
KWDA formid[KSIZ]     — injected Keywords (KYWD)
ALFV formid           — FO4 forced voice type
ALDI formid           — FO4 death-item leveled list
ALCS/ALMI             — FO4 collection alias + initial fill limit
```

### `FNAM` alias flags (verified bit table)

```
0x00001  Reserves Location (ALLS) / Reserves Reference (ALST)
0x00002  Optional
0x00004  Quest Object
0x00008  Allow Reuse in Quest
0x00010  Allow Dead
0x00020  In Loaded Area       (Find Matching Reference sub-option)
0x00040  Essential
0x00080  Allow Disabled
0x00100  Stores Text
0x00200  Allow Reserved
0x00400  Protected
0x00800  Forced by Aliases
0x01000  Allow Destroyed
0x02000  Closest              (Find Matching Reference sub-option, needs 0x20)
0x04000  Uses Stored Text
0x08000  Initially Disabled
0x10000  Allow Cleared        (ALLS only)
0x20000  Clear Names When Removed
0x40000  Actors Only
0x80000  Create Temporary
0x100000 External Linked
0x200000 No Pickpocket
0x400000 Apply to Non-Aliased Refs
0x800000 Companion
0x1000000 Optional All Scenes
```

This matches [`crates/core`'s convention]: a plain `u32` newtype with
named-constant bits (see `LIGHT_FLAG_*` in `components/light.rs`), not a
`bitflags!` macro — mirror that, not introduce a new flags idiom.

---

## Architectural spine

```
QUST sub-record stream
  ALST/ALLS → ALID → [fill-type field(s)] → CTDA* → FNAM → [injected data] → ALED
                        │
                        │  Phase 0 — parser: extend QustBlock (mirrors INDX/QOBJ)
                        ▼
        QuestAlias {
          alias_id: i32, name: String,
          fill_type: Option<AliasFillType>, // absent for condition/force-filled aliases
          flags: AliasFlags,             // FNAM bits
          match_conditions: ConditionList,  // reuse M47.1 verbatim
          injected: AliasInjectedData,   // factions / packages / spells / inventory / keywords — raw FormIds, uninterpreted
        }
                        │
                        │  Phase 1 — runtime: alias-fill system (quest start / cell load)
                        ▼
        SceneActorBindings { (QuestFormId, i32) -> EntityId }
          — a keyed resource holding only currently resolved aliases,
            the exact shape QuestStageFragments already established for
            per-quest runtime state
                        │
          ┌─────────────┼──────────────────────────────┐
          ▼              ▼                               ▼
  QuestRef::Property   RunOn::QuestAlias              AliasInjectedData
  / ObjectRef::Property  condition resolution           applied onto the
  (alias branch,         (shared live table,             filled entity —
  Phase 2)                consumer, Phase 2)             Phase 3
```

### Fill-type-by-fill-type feasibility (drives phase ordering)

| Fill type | Field | Resolution | Cost |
|---|---|---|---|
| Forced Reference | `ALFR` | direct `resolve_entity_by_global_form_id` | **trivial — Phase 1** |
| Unique Actor | `ALUA` | same resolver against an NPC_'s (presumed already-loaded) ACHR instance | **trivial — Phase 1**, declines gracefully if not loaded (same "not loaded → skip" discipline as everywhere else in this codebase) |
| Created Object | `ALCO` | needs a genuine spawn action (new entity at another alias's *already-filled* location, or in its inventory) | moderate — Phase 3+, ordering-sensitive (depends on other aliases being filled first, matching the source's own "aliases fill in order, dependencies only go upward" rule) |
| External Alias Reference | `ALEQ`+`ALEA` | cross-quest binding lookup by `(other_quest, alias_id)` | supported; fixed-point resolution handles authored dependency order |
| Location Alias Reference | `ALFA`+`KNAM`/`ALRT` | loaded XLRT candidates for the reference-type subset; true location/keyword traversal still needs LCTN/linked-ref models | partial |
| Near Alias | `ALNA`+`ALNT` | follows loaded XLKR neighbors in either authored link direction | supported for TES5/FO4 linked-ref types |
| Forced Location | `ALFL` (ALLS) | direct `LCTN` reference | blocked — same LCTN gap as above |
| Find Matching Reference | (CTDA-only) | evaluates `ConditionList` over loaded candidates; unloaded-world search needs Story Manager | supported for loaded references |

Phase 1 (Forced Reference + Unique Actor) is deliberately the "cheapest
20%" — both compose entirely from infrastructure that already exists
today (the resolver, the `ConditionList` type, the block-parsing pattern),
with zero new spawn/search/location machinery.

---

## Phase 0 shipped (2026-07-21)

`QustBlock` gained an `Alias(QuestAlias)` variant (mirrors the existing
`INDX`/`QOBJ` state machine exactly); `QuestAlias`/`AliasFillType`/
`AliasFlags`/`AliasInjectedData` decode the full shape from the field
table above. The focused QUST suite now has 27 tests spanning fill types,
companion fields, collections, conditions, targets, metadata, remapping, and
the full flag catalog.

**Cross-validated against real bytes, as required — and it caught a real
spec gap.** Two tools landed alongside the parser:
[`qust_alias_survey`](../../crates/plugin/examples/qust_alias_survey.rs)
(fill-type frequency + sanity counters over a whole ESM) and
[`qust_alias_rawdump`](../../crates/plugin/examples/qust_alias_rawdump.rs)
(every raw sub-record for one `QUST` by FormID — the tool that actually
resolved the finding below).

**The real distribution, measured, not assumed:**

| Fill type | Skyrim.esm | Fallout4.esm |
|---|---:|---:|
| UniqueActor | 22.5% | 13.6% |
| ForcedReference | 20.8% | 8.6% |
| FromEvent | 16.0% | 8.6% |
| LocationAliasReference | 15.8% | 18.2% |
| FindMatching (conditions only) | 10.9% | 23.8% |
| *(no fill type, no conditions)* | 7.2% | 8.7% |
| CreatedObject | 4.9% | 11.6% |
| ForcedLocation | 1.3% | (negligible) |
| ExternalAlias | 0.6% | (negligible) |

Phase 1's chosen pair (Forced Reference + Unique Actor) covers **43.3%**
of Skyrim's aliases — better than assumed — but only **22.2%** of FO4's,
where `FindMatching`/`LocationAliasReference` dominate instead. Sequencing
should account for this per-game skew rather than assuming Skyrim's curve
generalizes; FO4 content will lean on the harder fill types sooner than
Skyrim does.

**Spec correction: `ALFI` is "Force Into Alias," not a bare unknown.**
The UESP/xEdit source table lists `ALFI` as `int32, unknown`. Raw-byte
inspection (`qust_alias_rawdump` on `Skyrim.esm` quest `0002C258`) showed
it's the mechanism from a separately-known CK feature: once an alias
fills, it can *also* propagate its resolved value onto another alias by
index. Concretely: alias 1 (`Nurelion`, `ALFR`-filled to a real NPC
reference) carries `ALFI = 8`; alias 8 (`NurelionEssential`) has **no**
fill-type field and **no** `CTDA` at all — it exists solely to receive
alias 1's value under the Essential flag (the same pattern repeats for
`Quintus`/`QuintusEssential`, aliases 5→9). This is now decoded as
`QuestAlias::force_into_alias: Option<i32>`, independent of `fill_type`.

An important correctness implication for Phase 1+: **a `None` `fill_type`
does not mean "this alias never resolves."** ~926 Skyrim aliases (7.2%)
and ~1,011 FO4 aliases (8.7%) have neither a fill-type field nor Match
Conditions — but only 2 of those, in each game, carry their *own* `ALFI`.
The rest are Force-Into-Alias *targets*: nothing in the target alias's own
data reveals this — the runtime must scan every alias in the same
`QustRecord` for a `force_into_alias` pointing at it. Confirmed only 123
(Skyrim) / 467 (FO4) aliases carry a non-`None` `force_into_alias` at
all, so this is a real, if secondary, mechanism worth a Phase 1/2 line
item, not primarily what explains the "no fill" bucket's bulk (most of
that bulk is still unaccounted for — likely aliases genuinely filled by
something outside this record, e.g. a `PLDT`-attached quest-giver
resolved via the parent quest's Story Manager event, not decoded here;
flag for the Phase-1 fill-and-apply pass to re-examine with live data
rather than guessing further from static bytes alone).

The final format audit corrected the created-object companions against current
xEdit definitions: `ALCA` is `{target alias: int16, create mode: uint16}` and
`ALCL` is the authored `uint32` encounter level. The previously observed
`0x8000_0001` value is therefore two packed 16-bit fields, not an opaque i32.

---

## Implementation state

### Phase 0 — Parser (`crates/plugin`) — done
See "Phase 0 shipped" above for the deliverable, the tools, and the
`ALFI` spec correction.

### Phase 1 — Live fill table — done
`SceneActorBindings` fills from every loaded quest/candidate pair and refreshes
after quest lifecycle or cell-candidate changes. Forced Reference, Unique
Actor, loaded Find Matching, Location Alias Reference, External Alias, and
Force Into Alias are supported. Reuse and cross-quest reservations are
deterministic in authored order; missing/unloaded values simply stay unbound.

### Phase 2 — Consumers — done
Alias-bound VMAD object properties and `RunOn::QuestAlias` both resolve
through `SceneActorBindings`. Scene actors, objective markers, packages,
dialogue, conditions, and fragment effects therefore share one identity.

### Phase 3 — Injected data — done for canonical ECS stores
Apply `AliasInjectedData.factions` onto `FactionRanks` and `.inventory`
onto `Inventory` (push, mirroring the just-shipped `AddItem` semantics)
when an alias fills; remove/reverse on alias clear (factions "removed on
clear" per the source; inventory items are **not** removed, matching the
documented "permanent" Bethesda behavior — do not overcorrect this into
symmetry it doesn't have). Packages, spells, and keywords stay
parsed-not-applied pending their own components/consumer investigation
(see Non-goals). `QuestAliasInjectionState` persists the inventory-grant
ledger through `SaveRegistry`; faction bookkeeping and metadata overlays are
derived again from QUST definitions after load.
**Deliverable:** a real alias-injected faction/item shows up on the
filled entity's `FactionRanks`/`Inventory`, verified against a known
vanilla companion or radiant quest.

### Phase 4+ — Bounded follow-ups
Created Object, Story Manager event fills, true LCTN aliases, unloaded-world
search, reference collections, and components/consumers for the remaining
overlay families. Their complete authored records remain available through
`QuestAliasRuntimeOverlays` until those owning subsystems exist.

### M43.1 — Runtime observability

The in-engine command registry exposes the quest pipeline through the same
TCP `Eval` path used by `byro-dbg`:

- `quest.show <formid>` reports definition metadata, canonical lifecycle,
  stage history, objectives, resolved targets, and alias coverage.
- `quest.aliases <formid>` reports authored fill types, flags, injections,
  bound entities/reference identities, pending refresh state, and bounded
  unbound reasons. Read-only inspection never forces a refresh or grants
  inventory as a side effect.
- `quest.start`, `quest.stop`, and `quest.setstage` route through the same
  `Effect`/`apply_effects` path as Papyrus fragments, then refresh derived
  bindings so debug controls cannot drift from production semantics.
- [`m43-quest-runtime.sh`](../smoke-tests/m43-quest-runtime.sh) drives those
  commands against real Skyrim QUST data through the embedded debug server.

---

## Verification checklist for "M47.3 done" (per phase)

**Phase 0** — done (2026-07-21)
- [x] `QustBlock::Alias` decodes `ALST`/`ALLS`/`ALID`/all fill-type
      fields/`FNAM`/`CTDA`/injected-data fields/`ALED` (+ `ALFI`, a real
      addition beyond the original scoping — see "Phase 0 shipped")
- [x] Byte layout cross-validated against real `Skyrim.esm`/`Fallout4.esm`
      (`qust_alias_survey` + `qust_alias_rawdump`), not trusted from the
      wiki table alone — this is exactly what caught the `ALFI` gap
- [x] Corpus-frequency survey run; Phase 1's fill types are the majority
      on Skyrim (43.3%) but not FO4 (22.2%) — phase plan updated to flag
      the per-game skew rather than assuming Skyrim's curve generalizes
- [x] Unit tests per fill-type shape, metadata/condition sections, collections,
      per-log terminal data, version-aware targets, and FormID remapping

**Phase 1**
- [x] Canonical binding resource + reservation set
- [x] Forced Reference + Unique Actor resolve from loaded candidates
- [x] Find Matching, XLRT, External, and Force Into resolution
- [x] A not-yet-loaded target declines gracefully (no panic, no wrong
      resolution), consistent with the rest of the resolver family

**Phase 2**
- [x] `ObjectRef` alias branch resolves through the live binding table
      instead of declining
- [x] `RunOn::QuestAlias` resolves through the live binding table
- [ ] Live-corpus re-measurement of `fragment_coverage`'s `AddItem`/
      `MoveTo` yield shows a real (non-zero) hit rate

**Phase 3**
- [x] Alias-injected factions land on `FactionRanks`, removed on clear
- [x] Alias-injected inventory lands on `Inventory` idempotently and is
      not removed on clear
- [x] The permanent inventory-grant ledger survives save/load and prevents
      duplicate `CNTO` stacks on the first post-load alias refresh
- [x] Quest lifecycle, objectives, targets, bindings, injections, and bounded
      unbound reasons are observable through `byro-dbg`
- [x] A repeatable real-data runtime smoke drives quest inspection and
      lifecycle controls through the production TCP command path
- [x] Remaining metadata is exposed as source-attributed overlays
- [ ] Alias-injected inventory verified against a real vanilla quest
      end to end (unit coverage is complete; requires game data)

Always confirm milestone state against [ROADMAP.md](../../ROADMAP.md).

---

## References

External (verified 2026-07-21, no guessing):
- [Skyrim Mod:Mod File Format/QUST](https://en.uesp.net/wiki/Skyrim_Mod:Mod_File_Format/QUST) — UESP, `ALST`/`ALLS` section
- [TES5Edit/meta QUSTDef.wiki](https://github.com/TES5Edit/meta/blob/master/UESPWiki/QUSTDef.wiki) — the underlying xEdit record definition (fetched directly, quoted verbatim above)
- [Skyrim Mod:Mod File Format/VMAD Field](https://en.uesp.net/wiki/Skyrim_Mod:Mod_File_Format/VMAD_Field) — for the adjacent VMAD alias-scripts section this milestone doesn't touch but should stay consistent with

Internal:
- [`docs/engine/m47-2-design.md`](m47-2-design.md) — the `.pex` decompiler + recognizer chain this milestone's Phase 2 feeds
- [`docs/engine/m47-2-recognizer-scaling.md`](m47-2-recognizer-scaling.md) — the `AddItem`/`MoveTo` empirical-yield finding that motivated this scoping pass
- [`docs/engine/npc-spawn-ai-packages.md`](npc-spawn-ai-packages.md) — the `PACK` runtime this milestone's alias-injected packages eventually feed (Tier 7, not touched directly here)
- Code: `crates/plugin/src/esm/records/misc/quest.rs` (parser), `crates/scripting/src/scene.rs` (binding/injection runtime), `crates/scripting/src/condition.rs` (`RunOn::QuestAlias`), `crates/scripting/src/fragment.rs` (alias-bound object effects)
- Tools: [`crates/plugin/examples/qust_alias_survey.rs`](../../crates/plugin/examples/qust_alias_survey.rs) (fill-type frequency over a whole ESM), [`crates/plugin/examples/qust_alias_rawdump.rs`](../../crates/plugin/examples/qust_alias_rawdump.rs) (raw sub-records for one `QUST` by FormID — reuse for any future "does the wiki table match reality" question)
