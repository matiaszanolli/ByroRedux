# M47.3 — Quest Alias System: ALST/ALLS decode + alias-fill runtime

**Status:** Phase 0 shipped (2026-07-21) — `ALST`/`ALLS` decode landed and
cross-validated against real `Skyrim.esm`/`Fallout4.esm` bytes, including one
real spec correction (`ALFI`/Force Into Alias — see "Phase 0 shipped" below).
Phases 1+ (the fill-and-apply runtime) remain. Tier 3 (extends the M47
scripting family); cross-cuts Tier 7's `PACK` backlog (alias-injected
packages) and M47.2's `QuestRef`/`ObjectRef` `Property` resolution (already
built, still declines on any alias-bound VMAD entry pending Phase 2 here).

**Goal:** decode the `QUST` record's `ALST`/`ALLS` alias sections and build
the runtime that fills them with live references at quest start — the
mechanism Radiant Story quests use to target content dynamically ("kill
the bandit leader" without naming a specific NPC). This directly unblocks
three things already built and waiting:

1. **`QuestRef::Property`/`ObjectRef::Property` alias resolution.** Both
   `resolve_quest` (`fragment.rs`) and the new `resolve_property_form_id`
   (M47.2, landed 2026-07-21) already decline whenever a VMAD `Object`
   property has `alias != -1` — deliberately, because there was nothing to
   resolve the alias index *against*. Live-corpus measurement the same
   night found this is the *dominant* real-world idiom
   (`ObjectReference k = SomeAlias.GetActorRef()`), so this is the highest-
   leverage unblock for the `AddItem`/`MoveTo` effects just shipped.
2. **`RunOn::QuestAlias` condition evaluation.** M47.1's `ConditionContext::resolve`
   (`crates/scripting/src/condition.rs:269`) already recognizes
   `RunOn::QuestAlias` as a CTDA "Run On" target and logs `"alias ...
   resolvers deferred"` — a stub with a real consumer waiting.
3. **Radiant/companion quest behavior generally** — the alias-injected
   packages/factions/spells/inventory are how a quest modifies an actor's
   behavior *without* touching its base record, and are the actual
   mechanism behind most companion and radiant (MQ/Companions/Thieves
   Guild-style) quest logic.

**Non-goals (this scoping pass):**
- No general "Story Manager" world-search engine for the *Find Matching
  Reference* fill type on day one — that is the single hardest fill type
  (an open-ended conditioned world query) and the closest existing
  analog in this codebase, `PACK`'s `NearReference` resolution, was
  separately investigated and found only ~12% resolvable on real FNV
  data. Scope it as its own follow-up once the cheap fill types are
  live and real content shows how much it actually matters.
- No cross-quest alias bookkeeping for *External Alias Reference* in the
  first phases — it requires the *other* quest to already be running and
  its alias already filled, an ordering dependency worth its own pass.
- No new "alias-injected spells/keywords" components — grepped the ECS
  and found `FactionRanks` (real, used by M47.1's `GetFactionRank`) and
  `Inventory` (real, just gained a fragment-effect consumer tonight) but
  no `SpellList`/`KnownSpells`/`Keywords` component anywhere. Spells and
  keywords stay data-only (parsed, not applied) until those components
  exist — matching the M47.2 fragment-effect precedent of shipping the
  parse side even when the apply side has to wait.

---

## What's already built (the substrate)

| Piece | Where | State |
|---|---|---|
| QUST block-state-machine (`INDX`→stage, `QOBJ`→objective) | `crates/plugin/src/esm/records/misc/quest.rs::parse_qust` | **pattern to extend** — `QustBlock` enum + `flush_block`, directly generalizes to a third `Alias` variant |
| CTDA condition parsing + `ConditionList` | `crates/plugin/src/esm/records/condition.rs`, M47.1 | done — reusable verbatim for `ALST`/`ALLS`'s "Match Conditions" |
| `RunOn::QuestAlias` | `crates/scripting/src/condition.rs` | **recognized, stubbed** — logs and returns `None`; real consumer once aliases resolve |
| `resolve_entity_by_global_form_id` | `crates/scripting/src/condition.rs:326` | done — the FormID→EntityId resolver every "forced"/"unique actor" fill type needs, already load-bearing for M42.5–8 AI packages and tonight's M47.2 object-targeting effects |
| `FactionRanks` component | `crates/core/src/ecs/components/faction_ranks.rs` | done — direct target for alias-injected factions |
| `Inventory` component + `AddItem` fragment effect | `crates/core/src/ecs/components/inventory.rs`, M47.2 | done — direct target for alias-injected `CNTO` items |
| `QuestRef::Property` / `ObjectRef::Property` alias decline | `crates/scripting/src/translate/{compose,effects}.rs`, `fragment.rs` | done, *waiting* — the `alias != -1` branch this milestone activates |
| M42 AI packages (Follow/Escort/Guard/Travel/Sandbox/Wander/Patrol) | `byroredux/src/systems/{follow,escort,guard,travel,...}.rs` | done — the eventual consumer of alias-injected packages (Tier 7 `PACK` backlog), not touched by this milestone directly |

**Also found in passing, unrelated to this milestone but a cheap separate
fix:** `RunOn::Reference`'s stub comment claims "`find_by_form_id` requires
an interned FormId… not yet wired" — stale, in the very file that defines
`resolve_entity_by_global_form_id` 60 lines below. A one-line fix, filed
here so it isn't lost, not bundled into this design.

---

## The spec (verified against source, not guessed)

No spec was in-repo. Per the project's standing "no guessing" discipline
(the same one VMAD's decoder followed — "no public byte-spec was on hand
… derived by decoding real Skyrim.esm VMAD records"), the field table
below comes from the UESP wiki's `QUST` file-format page, whose content is
sourced from TES5Edit's own record definitions (the tool the modding
community treats as authoritative for this exact question):

- <https://en.uesp.net/wiki/Skyrim_Mod:Mod_File_Format/QUST> (`ALST`/`ALLS`
  section)
- <https://github.com/TES5Edit/meta/blob/master/UESPWiki/QUSTDef.wiki>
  (the underlying xEdit record definition, fetched directly 2026-07-21)

**Before writing the parser, cross-validate this table against real
`Skyrim.esm` `ALST`/`ALLS` bytes** (the same empirical-derivation step
`trace_block`/`nif_stats` do for NIF, and the VMAD decoder did for
Skyrim's compiled scripts) — the table gives field *identity and type*,
not a byte-offset guarantee, and Bethesda sub-record streams have a
history of undocumented quirks (see `#1611`'s NetImmerse markers, or
VMAD's own "must be processed sequentially, lengths not given" warning).

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
ALRT formid           — Location Alias Reference (ALST only): LCRT lookup
  ALFA int32          —   companion field: unknown, "may be a formid with
                          co-opted flag bits" per the source — decline/
                          carry raw, do not interpret speculatively
(no fill field at all) — Find Matching Reference: CTDA-only, hardest case
CTDA* struct[32]      — Match Conditions (repeatable)
  CIS2 zstring        —   CTDA auxiliary variable name

── Fill-type companions ──
ALCA int32            — companion to ALCO (unknown meaning per source)
ALCL int32            — companion to ALCO (unknown meaning per source)

── Properties / injected data (any subset, any order per source) ──
FNAM int32            — flags (table below)
ALED empty            — block terminator ("always the final field")
VTCK formid           — additional valid voice type (NPC_ or FLST)
ALDN formid           — Display Name → MESG record
ALFC formid*          — injected Factions (FACT), repeatable
ALFI int32            — unknown
ALPC formid*          — injected Packages (PACK), repeatable
ALSP formid*          — injected Spells (SPEL), repeatable
COCT int32            — CNTO count (absent if zero)
CNTO struct[8]*       — injected inventory: {formid item, uint32 count}
ECOR formid           — Combat Override package list (FLST)
KNAM formid           — unknown, single KYWD
KSIZ uint32           — KWDA count
KWDA formid[KSIZ]     — injected Keywords (KYWD)
NAM0 int32            — unknown
QTGL int32            — unknown
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
0x00800  unknown
0x01000  Allow Destroyed
0x02000  Closest              (Find Matching Reference sub-option, needs 0x20)
0x04000  Uses Stored Text
0x08000  Initially Disabled
0x10000  Allow Cleared        (ALLS only)
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
          alias_id: u16, name: String,
          fill_type: AliasFillType,      // enum, one variant per fill field above
          flags: AliasFlags,             // FNAM bits
          match_conditions: ConditionList,  // reuse M47.1 verbatim
          injected: AliasInjectedData,   // factions / packages / spells / inventory / keywords — raw FormIds, uninterpreted
        }
                        │
                        │  Phase 1 — runtime: alias-fill system (quest start / cell load)
                        ▼
        FilledAlias { quest: QuestFormId, alias_id: u16, entity: Option<EntityId> }
          — a keyed resource (HashMap<(QuestFormId, u16), FilledAlias>),
            the exact shape QuestStageFragments already established for
            per-quest runtime state
                        │
          ┌─────────────┼──────────────────────────────┐
          ▼              ▼                               ▼
  QuestRef::Property   RunOn::QuestAlias              AliasInjectedData
  / ObjectRef::Property  condition resolution           applied onto the
  (alias branch,         (condition.rs's stub           filled entity —
  Phase 2)                consumer, Phase 2)             Phase 3
```

### Fill-type-by-fill-type feasibility (drives phase ordering)

| Fill type | Field | Resolution | Cost |
|---|---|---|---|
| Forced Reference | `ALFR` | direct `resolve_entity_by_global_form_id` | **trivial — Phase 1** |
| Unique Actor | `ALUA` | same resolver against an NPC_'s (presumed already-loaded) ACHR instance | **trivial — Phase 1**, declines gracefully if not loaded (same "not loaded → skip" discipline as everywhere else in this codebase) |
| Created Object | `ALCO` | needs a genuine spawn action (new entity at another alias's *already-filled* location, or in its inventory) | moderate — Phase 3+, ordering-sensitive (depends on other aliases being filled first, matching the source's own "aliases fill in order, dependencies only go upward" rule) |
| External Alias Reference | `ALEQ`+`ALEA` | cross-quest `FilledAlias` lookup by `(other_quest, alias_id)` | moderate — Phase 4, needs the other quest already running |
| Location Alias Reference | `ALRT` | `LCRT` lookup against the quest's `LCTN.LCSR` | blocked — no `LCTN`/Location-alias ECS model exists yet; scope separately |
| Forced Location | `ALFL` (ALLS) | direct `LCTN` reference | blocked — same LCTN gap as above |
| Find Matching Reference | (CTDA-only) | Story-Manager-style world search + `ConditionList` evaluation | **hardest — own follow-up**, per the `PACK` `NearReference` precedent (~12% resolvable) |

Phase 1 (Forced Reference + Unique Actor) is deliberately the "cheapest
20%" — both compose entirely from infrastructure that already exists
today (the resolver, the `ConditionList` type, the block-parsing pattern),
with zero new spawn/search/location machinery.

---

## Phase 0 shipped (2026-07-21)

`QustBlock` gained an `Alias(QuestAlias)` variant (mirrors the existing
`INDX`/`QOBJ` state machine exactly); `QuestAlias`/`AliasFillType`/
`AliasFlags`/`AliasInjectedData` decode the full shape from the field
table above. 15 new unit tests (one per fill type + companion-field
attachment + multi-alias flush independence + a full flag-catalog
round-trip); the whole workspace (3,000 tests) stays green.

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

Also confirmed empirically: `ALCA`/`ALCL` (companions to `ALCO`) really
are opaque — one real sample decoded to `0x8000_0001` as a raw `i32`,
not a plausible count or flag-clean value, matching the source's own
"Unknown" admission. Carrying them raw (rather than guessing a meaning)
was the right call.

---

## Phased plan

### Phase 0 — Parser (`crates/plugin`) — done
See "Phase 0 shipped" above for the deliverable, the tools, and the
`ALFI` spec correction.

### Phase 1 — Fill the cheap 20% (`crates/scripting`)
`FilledAlias` resource + reservation set (a `HashSet<u32>` of
reserved-reference FormIds is enough for the "prevents other quests from
using this ref" contract — full multi-quest priority arbitration can wait).
Alias-fill system resolves `Forced Reference` and `Unique Actor` via
`resolve_entity_by_global_form_id`, gated on the "in loaded area" the
resolver already implies (a not-yet-loaded target simply doesn't resolve
this pass — re-attempted on the next cell load / stream tick, not polled).
**Deliverable:** a real vanilla quest with a Forced-Reference or
Unique-Actor alias (e.g. a quest-giver) fills its alias on a real cell
load; `entities`/`byro-dbg` can show the `FilledAlias` mapping.

### Phase 2 — Wire the two waiting consumers
- `resolve_quest`/`resolve_property_form_id`'s `alias != -1` branch:
  instead of declining, look up `FilledAlias` for `(quest, alias)` and
  resolve through it.
- `ConditionContext::resolve`'s `RunOn::QuestAlias` arm: same lookup.
**Deliverable:** the `AddItem`/`MoveTo` fragment effects shipped tonight
go from "correct but ~0% real yield" to actually firing on real content
that uses the Phase-1 fill types; `RunOn::QuestAlias` conditions
evaluate instead of returning the safe-default `0.0`.

### Phase 3 — Injected data (factions + inventory first)
Apply `AliasInjectedData.factions` onto `FactionRanks` and `.inventory`
onto `Inventory` (push, mirroring the just-shipped `AddItem` semantics)
when an alias fills; remove/reverse on alias clear (factions "removed on
clear" per the source; inventory items are **not** removed, matching the
documented "permanent" Bethesda behavior — do not overcorrect this into
symmetry it doesn't have). Packages, spells, and keywords stay
parsed-not-applied pending their own components/consumer investigation
(see Non-goals).
**Deliverable:** a real alias-injected faction/item shows up on the
filled entity's `FactionRanks`/`Inventory`, verified against a known
vanilla companion or radiant quest.

### Phase 4+ — Deferred, each its own scoping pass
Created Object, External Alias Reference, Location aliases (blocked on
an LCTN/location-alias model), Find Matching Reference (Story-Manager
search). Do not build these speculatively — sequence by what the Phase-0
corpus survey shows is actually load-bearing on real content.

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
- [x] Unit tests per fill-type shape + the "no fill field → Find Matching"
      case (15 new tests, 3,000 total workspace tests passing)

**Phase 1**
- [ ] `FilledAlias` resource + reservation set
- [ ] Forced Reference + Unique Actor resolve on a real cell load
- [ ] A not-yet-loaded target declines gracefully (no panic, no wrong
      resolution), consistent with the rest of the resolver family

**Phase 2**
- [ ] `QuestRef`/`ObjectRef` alias branch resolves through `FilledAlias`
      instead of declining
- [ ] `RunOn::QuestAlias` resolves through `FilledAlias`
- [ ] Live-corpus re-measurement of `fragment_coverage`'s `AddItem`/
      `MoveTo` yield shows a real (non-zero) hit rate

**Phase 3**
- [ ] Alias-injected factions land on `FactionRanks`, removed on clear
- [ ] Alias-injected inventory lands on `Inventory` via the shared
      `AddItem`-style push, **not** removed on clear
- [ ] Verified against one real vanilla companion/radiant quest end to
      end

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
- Code: `crates/plugin/src/esm/records/misc/quest.rs` (parser substrate), `crates/scripting/src/condition.rs` (`RunOn::QuestAlias` stub + `resolve_entity_by_global_form_id`), `crates/scripting/src/fragment.rs` (the `alias != -1` decline this milestone activates)
- Tools: [`crates/plugin/examples/qust_alias_survey.rs`](../../crates/plugin/examples/qust_alias_survey.rs) (fill-type frequency over a whole ESM), [`crates/plugin/examples/qust_alias_rawdump.rs`](../../crates/plugin/examples/qust_alias_rawdump.rs) (raw sub-records for one `QUST` by FormID — reuse for any future "does the wiki table match reality" question)
