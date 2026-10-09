# Story Manager event dispatch

**Status**: Phases 0–4 **landed 2026-10-07/08** (#5366). The runtime is
complete for the Skyrim dialect and the FO4/SF decode is in: the
`SMBN`/`SMEN`/`SMQN` parse (`crates/plugin/src/esm/records/misc/story_manager.rs`
→ `EsmIndex.story_manager_nodes`, FO4/SF tail typed-raw), the `SmTree`
fold + `StoryEvent` dispatcher (`crates/scripting/src/story_manager.rs`),
three producers (`KILL` at combat death, `CLOC` at `XLCN` LCTN
granularity, `AHEL` at conversation open — activation greeting and
force-greet), `RunOn::EventData` over the corpus-decoded R1/R2/L1/L2
slots, `FromEvent` alias fills, and the `DNAM` policies with `RNAM`
reset windows over save-persistent node state. Remaining under the
Phase-4 umbrella: breadth producers whose subsystems have not landed
(`SCPT` needs the Papyrus `SendStoryEvent` native surface, `LEVL` a
level-up transition, crime/crafting/pickpocket their systems) and the
unconsumed `DNAM`/`HNAM`/`MNAM`/`XNAM`/`QNAM` semantics — named from
xEdit since #5420, awaiting runtime consumers. This document remains the design authority; it is the
scoping pass for the Story Manager half of M43's open scope ("Story
Manager event payloads and search") and the M47.2 row's "ESM-native
event dispatch / Story Manager" item.

## 1. What the Story Manager is (and which games have one)

The Creation engine (Skyrim onward) starts most of its quests from an
event-driven node tree stored in the plugin file, not from script calls.
The CK tutorial in the local reference wiki
(`ck-uesp-wiki/(main)/Bethesda Tutorial Story Manager.wiki`) describes the
model: when a story event fires (an actor killed, a location entered, a
level gained…), the engine walks the tree for that event type,
evaluating each node's conditions against the *event data* (killer,
victim, location…), and a passing quest node starts its quest — filling
that quest's aliases from the same event data. Nodes are either
**stacked** (children evaluated top to bottom) or **random** (one child
chosen at random), and a node that fires without the "shares event"
setting stops traversal for the rest of the tree.

**Pre-Creation titles do not have one.** A top-level-group walk of every
master on disk (method: §4) finds no SM records at all in Oblivion,
FO3, or FNV — their quests are driven by script (`Begin GameMode` /
result scripts), which is the FO3/FNV dialogue/scripting cluster already
being worked. The Story Manager item is therefore Skyrim-plus scope:

| Master | SMBN | SMEN | SMQN | Nodes |
|---|---:|---:|---:|---:|
| `Skyrim.esm` | 99 | 24 | 448 | 571 |
| `Dawnguard.esm` | 11 | 1 | 36 | 48 |
| `Dragonborn.esm` | 15 | 2 | 43 | 60 |
| `Fallout4.esm` | 75 | 17 | 219 | 311 |
| `SeventySix.esm` | (present; not re-counted — see §4) | | | |
| `Starfield.esm` | 140 | 20 | 354 | 514 |

**Headline number**: 1191 of `Skyrim.esm`'s 1811 QUST records (65%) are
referenced by at least one SM quest node. Two-thirds of Skyrim's quest
content is gated on this subsystem; no amount of quest-runtime work
short of it will make those quests start as authored.

## 2. What already exists (the foundation this builds on)

- **Parse side.** The three record types ride the unknown-group skip in
  `parse_esm_with_load_order`'s top-level dispatch
  (`crates/plugin/src/esm/records/parse.rs`) — full-file parses already
  skip them cleanly at 100% recoverable. The natural insertion point is
  the `dispatch_misc_gameplay_a` arm list (where `QUST`/`SCEN`/`DLBR`
  already route), with the typed record following the
  `records/misc/quest.rs` pattern. `EsmIndex.navmeshes` (#1272) is the
  precedent for a new per-FormID map on the index, including the
  load-order remap the walker already applies.
- **Conditions.** `CITC`/`CTDA`/`CIS2` appear on SM nodes in exactly the
  shape the M47.1 condition evaluator (`crates/scripting/src/condition.rs`)
  already consumes elsewhere (count-prefixed CTDA array, IS-variable
  adapters). The one extension needed is a *run-on target* for event
  data (see §6.3).
- **Quest lifecycle + aliases.** M43 shipped quest start/stop/stage
  transitions and save-persistent progress; P4 shipped loaded-reference
  alias fill with conditions
  (`crates/scripting/src/scene/quest_alias.rs`). An SM-fired quest start
  reuses both — the alias fill just gets its candidates from event data
  instead of the world query.
- **Event producers.** The ECS event model
  (`crates/scripting/src/events.rs` — transient marker components added
  by producers, consumed same-frame) is the bus a dispatcher reads.
  Several SM events already have producer-shaped engine surfaces (§6.4).
- **Reference material.** Local reference wikis carry the CK-side
  documentation: `ck-uesp-wiki` and `falloutck-uesp-wiki` both have the
  tutorial, a `Category:Story Manager`, and one `OnStory*` page per
  event (Papyrus-side mirror of the same dispatch). Per the standing
  rule, these are checked before any web search.

## 3. On-disk structure (corpus-verified)

All three types are flat top-level records; the tree is encoded by
pointers, not nesting. Field semantics below are marked
**[verified]** where the corpus itself proves them (method in §4) and
**[open]** where they land in the §5 alignment pass. Nothing unmarked-by-
verification below should be treated as decoded.

| Field | On | Meaning | Status |
|---|---|---|---|
| `EDID` | all | editor ID | **[verified]** |
| `PNAM` u32 | all | parent node FormID; `0x0000005B` on every SMEN = the root sentinel (its Skyrim EDID resolves to literally `Root`) | **[verified]** |
| `SNAM` u32 | all | next-sibling node FormID (the stack order) | **[verified]** — 441/443 Skyrim, 218/219 FO4, 366/366 Starfield in-tree targets share the source's `PNAM` |
| `ENAM` char[4] | SMEN | event mnemonic; exactly one SMEN per mnemonic per master (24 Skyrim / 17 FO4 / 20 Starfield) | **[verified]** structure; per-mnemonic mapping in §3.1 |
| `CITC` u32 + `CTDA`×N + `CIS2` | all | node conditions (existing CTDA representation) | **[verified]** shape |
| `NNAM` u32 | SMQN | the QUST this node starts | **[verified]** — Skyrim 425/448 resolve to QUST records via EDID cross-check (e.g. `MS05KingOlafsFestivalStarter` → its QUST) |
| `DNAM` u32 | all | node-policy flags — 0x1 random, 0x10000 do-all-before-repeating, 0x20000 shares-event (Phase-3 decode, §3.3); 0x2 warn-if-no-child-quest-started, 0x40000 the Num-quests-to-run checkbox | **[decoded]** — #5420 closed the last two bits from xEdit `TES5.pas`; the runtime still consumes only the Phase-3 three |
| `RNAM` f32 | SMQN | per-quest hours-until-reset window, follows its `NNAM` | **[decoded]** (Phase 3; §3.3) |
| `HNAM` u32 (as f32) | SMQN, FO4+ | **Hours until reset** (xEdit FO4+/SF1) — 72.0 on the Minutemen node, 0.3–24.0 elsewhere | **[named]** #5420 (typed-raw since Phase 4; runtime does not gate) |
| `MNAM` u32 | SMQN, FO4 rare / SF common | **Num quests to run** (xEdit FO4/SF1) — observed 1–22 | **[named]** #5420; the runtime still starts one quest per node |
| `XNAM` u32 | all | **Max concurrent quests** (xEdit `TES5.pas:7056-7111`) — observed 0/1/2 | **[named]** #5420 |
| `QNAM` u32 | SMQN | **Quest Count** — the `NNAM` array's `SetCountPath`; census `QNAM == count(NNAM)` on 448/448 Skyrim, 219/219 FO4, 354/354 Starfield | **[named]** #5420 (not a priority/repeat knob) |
| `FNAM` u32 | SMQN, TES5 | **24 Hours Till Reset** bool (xEdit `TES5.pas`; 5 vanilla SMQNs) | **[named]** #5420 (raw; no runtime consumer) |
| `UNAM` u32 | SMQN, FO76 | **Priority**, per quest (xEdit FO76) | **[named]** #5420 (raw; no runtime consumer) |
| `SNAM` u32 | SMBN/SMEN | when present, points at another node — see §5 | **[open]** (distinct from the sibling use on SMQN) |
| FO4+: repeated `NNAM` pool + `RNAM` | SMQN | pools at Skyrim-unseen scale (`RETravelQuests` 44; the Minutemen node's pool measured **14**, not the 16 this doc first guessed), `RNAM` pairing identical to Skyrim's | **[decoded]** (Phase 4 census; same bits, same `RNAM` semantics) |
| SF: `MNAM` | SMQN | Num quests to run (#5420) | **[named]** |

A census artifact worth recording so the Phase-0 test floors aren't
misread: FO4 and Starfield keep their DIAL/INFO inside per-QUST child
groups (their top-level group lists have no `DIAL`), so a naive
records-at-group-top count sees "1 QUST" in those masters. The engine's
real walker recurses (`records/grup_walker.rs`); the Rust-side floors
will measure the true counts, and the Python census's FO4/SF
quest-resolution rates are meaningless for that reason (§4 note).

### 3.2 Event-data slots (the Phase-2 census)

Conditions and alias fills that read the *event itself* select their
slot with a 2-byte ASCII tag stored in the low half of the selecting
integer — the CTDA tail (`extra_data_id`) when `run_on == 7`
(`RunOn::EventData`), and QUST `ALFD` beside `ALFE` for
`AliasFillType::FromEvent` fills:

| Tag | Value | Holds (per mnemonic, Papyrus `OnStory*` param order) |
|---|---|---|
| `R1` | `0x3152` | first reference — KILL victim, CLOC actor, CAST caster, SCPT ref1 |
| `R2` | `0x3252` | second reference — KILL killer, CAST target, SCPT ref2 |
| `L1` | `0x314C` | first location — CLOC old, SCPT/SendStoryEvent `akLoc` |
| `L2` | `0x324C` | second location — CLOC new |

Census (2026-10-07, Skyrim.esm + DLCs): 302 run-on-7 SM-node CTDAs read
R1×218 / R2×83 / L×0 (the L vocabulary is `ALFD`-only on nodes) plus
one malformed non-ASCII tail; 2 065 Skyrim `FromEvent` alias fills span
all four tags. The semantic anchors that pin the mapping: the CK
tutorial's own `DA08KillFriendNode` ("we're checking that the player is
the one doing the killing, wielding the Ebony Blade") authors
`GetIsID(0x7)` + `GetEquipped(0x4A38F)` both on the **R2** tag → R2 =
killer; `WIKill06`'s `Victim` alias fills from **R1** → R1 = victim;
`WIChangeLocation04` fills `OldLocation`←L1 and `NewLocation`←L2.
Floored by `story_manager_skyrim_event_data_slot_floor`
(`crates/plugin/tests/parse_real_esm.rs`), including the DA08 anchor.

One catalog fall-out measured at the same time: Skyrim condition
functions the M47.1 catalog did not carry — `GetInFaction` (**71**, all
6 904 corpus uses take a FACT param; FO3/FNV author no CTDA at 71, so
the shared catalog cannot mislabel them) landed with Phase 2, and
several more were identified by param-record-type census for later:
`GetStage`-family **56** (316 QUST-param uses — distinct from the
catalog's 58, which Skyrim also uses 8 029×), `GetGlobalValue` **74**,
random-roll **77**, `HasSpell` **223**, `GetInCurrentLoc` **359**,
`HasKeyword` **560**, `LocationHasKeyword` **562**, and a location
comparator at **565**. `fn 576` (330 uses, `param_1` packing the same
ASCII-tag vocabulary in its *high* half — DA08's `0x3256_0002` names
R2) is the suspected `GetRelationshipRank`-on-event-data but stays
undecoded pending verification.

### 3.1 Event catalogs (observed mnemonics)

| Game | Mnemonics |
|---|---|
| Skyrim | `CRFT ARRT CAST INTM FLAT LOCK JAIL LEVL ESJA BRIB ADCR CHRR DEAD AHEL NVPE SKIL REMP ASSU AIPL AFAV ADIA SCPT CLOC KILL` |
| FO4 | `ADCR ASSU LEVL TMEE AHEL IRON HACK LOCK REMP AIPL LCLD AOBJ CLOC ADIA SCPT KILL OAAT` |
| Starfield | FO4's 17 + `XPLL DOCK LAND` (20) |

Proven from SMEN/SMBN EDIDs in-corpus: `LEVL`=IncreaseLevel,
`ADCR`=CrimeGoldEvent, `ASSU`=AssaultActorEvent, `SCPT`=the
WIScriptEvent branch, `CRFT`=the TutorialCrafting node's parent event.
High-confidence from the CK wiki `OnStory*` names (align fully in §5):
`KILL` KillActor, `CLOC` ChangeLocation, `CAST` CastMagic, `ARRT`
Arrest, `AIPL` AddToPlayer, `REMP` RemoveFromPlayer, `SKIL`
IncreaseSkill, `NVPE` NewVoicePower, `DEAD` DiscoverDeadBody, `BRIB` /
`INTM` / `FLAT` Bribe/Intimidate/FlatterNPC. Unmapped pending §5:
`ADIA AHEL AFAV CHRR ESJA JAIL LOCK AOBJ IRON OAAT TMEE LCLD XPLL DOCK
LAND`. The mnemonic→event-name table is runtime data, not code — it
ships as a per-game table with provenance comments.

### 3.3 Node policies and reset windows (the Phase-3 census)

The CK's node-property model (SM Event Node, local wiki) decodes onto
`DNAM`'s observed domain (`{0, 1, 2, 0x10000…0x70001}` — a low byte plus
`0x10000/0x20000/0x40000`):

| Bit | Property | Corpus anchors |
|---|---|---|
| `0x1` | Random (else Stacked) | the `CompanionsRadiantNode` / `WERoadQuests` branches; `SMEN` roots are always 0 |
| `0x10000` | Do all before repeating | clusters on the radiant-cycle families (BQ bounty holds `0x30001`, WE/WI wilderness incidents, `WEPriorityQuests`) |
| `0x20000` | Shares Event (SMQN only) | every `*SHARES*`-named quest node; `WIKillEventsRandomChance`=`0x10000` vs sibling `WIKillEventsNoRandomChanceSHARES`=`0x20000`; branches never set it, matching the CK UI |

`RNAM` (346 Skyrim `SMQN`s) is the per-quest **Hours until reset** —
"the Story Manager will not attempt to start this quest again until the
indicated number of Game Hours has passed" — following its `NNAM`
(`MQ304SovngardeScenes` pairs 2.4/4.8/4.8 across its three-link pool;
the `WEBountyCollector*` holds carry 1152.0 = 48 game days; most RNAMs
are 0.0 = ignored). `0x2` (`MS04/MS06IncreaseLevelNodeSHARES`=`0x20002`;
candidate: Warn if no child quest started) and `0x40000`
(`FavorChangeLocation*`=`0x60000`; candidates: the Num-quests-to-run /
Max-concurrent checkboxes, `QNAM` 0–68 as one of the numbers) stay
**[open]**, raw in `SmNodeRecord::dnam`. Floored by
`story_manager_skyrim_node_policy_floor` (`parse_real_esm.rs`).

## 4. Provenance — how every number above was measured

One-off Python census (mmap walk mirroring `EsmVariant` header sizes:
24-byte Tes5Plus record/group headers, 6-byte subrecord headers with the
`XXXX` overflow protocol), run 2026-10-07 against the masters on disk:
top-level group labels per master; SM-group record/subrecord walks;
FormID→EDID cross-index over each whole master for pointer resolution.
Key cross-checks that make the field table **[verified]**:

- `PNAM=0x5B` on all 24 Skyrim SMEN, and `0x5B`'s EDID is `Root`.
- Sibling test: for every in-tree `SNAM` target, target's `PNAM` ==
  source's `PNAM` (rates in §3).
- `NNAM` resolution: 425/448 Skyrim SMQN `NNAM`s hit QUST records whose
  EDIDs match the node's own (the misses are zero/DLC refs).
- 1191/1811: distinct QUSTs referenced by any SMQN `NNAM` (with the
  0x01 self-master byte remap FO4/SF use for their own records).

Phase 0 replaces this script's numbers with Rust parser-floor tests (the
house pattern: floors, not pinned counts, exactly like
`parse_rate_fnv_esm`).

## 5. What remains to be interpreted (the Phase-0 alignment pass)

Each item has a method; none is a guess:

1. **`DNAM` flag bits.** Resolved at Phase 3 for the three policy bits
   (§3.3); #5420 closed `0x2` (warn if no child quest started) and
   `0x40000` (the Num-quests-to-run checkbox) from xEdit's definitions.
2. **`XNAM` / `QNAM` ints.** #5420: xEdit defines `XNAM` as Max
   concurrent quests and `QNAM` as the `NNAM` Quest Count (the census's
   100% `QNAM == count(NNAM)` match). Both stay raw on the record until
   a runtime consumer needs them.
3. **`SNAM` on SMBN/SMEN.** Distinct from the SMQN sibling use; likely
   a "node to notify / shared event target" link. Method: enumerate the
   ≤100 occurrences with EDIDs; the pattern will be obvious.
4. **FO4/SF dialect**: `HNAM`, `NNAM` pools, `RNAM`, `MNAM`, and the
   quest-count truth under nested groups. Method: same census in Rust,
   where the walker already recurses.
5. **Unmapped mnemonics** (§3.1 list). Method: each SMEN's subtree
   SMBN/SMQN EDIDs + the CK wiki's `Category:Story Manager` /
   `OnStory*` pages; Starfield's `XPLL DOCK LAND` against its CK wiki.

## 6. Runtime design

### 6.1 Parse phase

`records/misc/story_manager.rs` (name TBD by the implementer):
`SmNodeRecord { kind: Branch|Event|Quest, editor_id, parent, next_sibling,
event_mnemonic, conditions: Vec<Ctda>, quest_links: Vec<FormId>,
dnam/xnam/qnam raw ints }` — raw ints stay raw until §5 settles them.
Three `parse_sm*` functions + one dispatch arm; `EsmIndex.story_nodes:
HashMap<u32, SmNodeRecord>` with the standard load-order remap. Plugin
merging follows the existing last-wins FormID overlay (nodes are plain
records; no special merge semantics).

### 6.2 Tree resource

At load-order build time, fold the flat map into a
`StoryManagerTree` resource: `Vec<SmNode>` storage with
parent→first-child and sibling indices resolved once, roots (the SMEN
per mnemonic) indexed by mnemonic → root node. Cycle/orphan guard at
build (a malformed mod must not hang the walker); unresolved parent
pointers demote the node to a root-with-warning, matching the
fail-visible-not-fatal posture used elsewhere.

### 6.3 Dispatch

One new transient marker family mirroring `events.rs` (landed shape,
Phase 2):

```rust
pub struct StoryEvent {
    pub mnemonic: [u8; 4],           // the SMEN.ENAM catalog (§3.1)
    pub reference_1: EntityId,       // "R1" — KILL victim, CLOC actor…
    pub reference_2: Option<EntityId>, // "R2" — KILL killer…
    pub location_1: Option<u32>,     // "L1" — CLOC old LCTN…
    pub location_2: Option<u32>,     // "L2" — CLOC new LCTN…
}
```

The slots are the wire format's own (§3.2) — the Phase-1 open question
"typed per-mnemonic enum vs slot-based" resolved in favor of exactly
these four positional slots, because that is what CTDA tails and `ALFD`
address and no mnemonic carries more. `story_manager_dispatch_system`
(Stage::Update, after `quest_startup_system`): for each `StoryEvent`,
look up the mnemonic's root, walk children in sibling order evaluating
each node's `CTDA` set through the M47.1 evaluator with
`ConditionContext::event_data` set — `RunOn::EventData` resolves the
R1/R2 tags (L-tags fail pending a location runtime). A passing SMQN
starts its quest via the existing quest lifecycle, records its slots in
`StoryEventAliasFill`, and the P4 alias refresh fills `FromEvent`
aliases from them. Traversal honors the decoded `DNAM` bits (§3.3): shares-event →
continue past a processed quest node (else consume the event), random →
shuffled child order stopping at the first fire / uniform pool pick,
do-all-before-repeating → the persisted round-robin marks, `RNAM`
windows → the persisted last-fire timestamps (Phase 3, landed).
`SendStoryEvent` (already catalogued in the SKSE compatibility surface)
lowers onto the same marker — that is the `SCPT` event node's producer
(and `sm.event`, the debug-console producer, landed with Phase 2 for
live gating).

### 6.4 Producer inventory (what can fire today)

Phasing follows this, not the catalog order — start where the engine
already has the surface:

- **Landed**: `KILL` (combat death site, Phase 1), `CLOC`
  (cell-transition at LCTN granularity, Phase 2), `AHEL` (every opened
  conversation — activation greeting and force-greet, Phase 4).
- **Deferred until their subsystems land**: `SCPT` (needs the Papyrus
  `SendStoryEvent` native-call surface — no native runtime yet),
  `LEVL`/`SKIL` (no level-up/skill-increment transition site),
  `DEAD` (no dead-body discovery logic), crime/arrest/crafting/
  pickpocket families, FO4's `HACK`/`LOCK`/`IRON`/`OAAT`/`TMEE`/
  `LCLD`/`AOBJ`, Starfield's ship surface.
- **Needs a producer**: crime/arrest/jail family (`ADCR ARRT ESJA JAIL
  BRIB INTM FLAT` — no crime system yet), crafting (`CRFT`),
  pickpocket/steal (`AIPL REMP AFAV`), `DEAD`, `SKIL`, FO4's
  `HACK LOCK IRON OAAT TMEE LCLD AOBJ`, Starfield's ship surface.

### 6.5 Persistence

Node-local state (last-fired frame/time, round-robin cursor, "quest
already running" checks) joins the save alongside `QuestRevision`-style
quest state, so a quickload cannot resurrect a radiant quest the
pre-save world already fired. `NOT_SAVED_BY_DESIGN`: nothing in the
tree itself (rederived from the load order every boot).

## 7. Phases

1. **Phase 0 — decode + floors. Landed 2026-10-07 (#5366).**
   `records/misc/story_manager.rs` decodes all three record types
   (pointer fields remapped, conditions through the shared `push_ctda`,
   `DNAM`/`XNAM`/`QNAM` deliberately raw, FO4/SF tail in `extras`),
   routed through the `dispatch_misc_gameplay_a` arm into
   `EsmIndex.story_manager_nodes` (a counted + merged category). Gates:
   the Skyrim census floor test reproduces the Python census exactly —
   571 nodes, 24 distinct mnemonics, 448 SMQNs, 1191 SM-referenced
   quests, 441/443 sibling integrity, root `Root`@`0x5B` with every SMEN
   parented to it — plus an FO4 count floor (311 nodes / 17 mnemonics;
   FO4's own-record `0x01` self-master byte means quest-link resolution
   is only meaningful in the load-order lane, so it is not floored
   there). The §5 alignment pass is **still open** — nothing decodes
   `DNAM`/`XNAM`/`QNAM` semantics yet.
2. **Phase 1 — first event end-to-end. Landed 2026-10-07 (#5366).**
   `crates/scripting/src/story_manager.rs`: `build_story_manager_tree`
   (children ordered by the authored `SNAM` chains from each group's
   chain head; cycle/headless fallback + a visited bitmap so malformed
   mods cannot loop dispatch), the `StoryEvent` Pattern-B marker
   (#2672-registered in `cleanup.rs`'s two-pattern contract),
   `story_manager_dispatch_system` (mnemonic root → sibling-order walk,
   CTDA evaluation through the shared M47.1 evaluator with
   Subject/Object run-on, quest start through the canonical
   `QuestStageState` lifecycle with the registry's authored start-up
   stage), scheduled in `Stage::Update` directly after
   `quest_startup_system` so SM-started quests are indistinguishable
   from Start Game Enabled ones downstream. Producers: `KILL` at
   `combat_damage_system`'s death transition (aggressor → subject, slain
   actor → object), `CLOC` from `story_change_location_system` over the
   cell-loader location contexts (interior: cell editor-id; exterior:
   worldspace+grid — coarser than LCTN granularity, tightened in Phase
   2). Shares-event is the continue-past default until `DNAM` decodes
   (§5). The **live gate landed 2026-10-07**
   ([`sm1-story-manager.sh`](../smoke-tests/sm1-story-manager.sh)):
   booting the Skyrim profile into WhiterunDragonsreach with a player
   fires CLOC on the initial location-context install, and the
   dispatcher walks the CLOC subtree — #5380 narrowed the boot set to
   the genuinely-gated nodes: a node gated on an uncatalogued
   condition function declines (`WIGreetingNodeSHARES`'s fn-145 `== 0`
   used to pass vacuously through the Unknown→0.0 default and was one
   of the two asserted starts before the decline landed), while
   `CWChangeLocationScenes` starts through its newly cataloged
   `GetQuestRunning(CWFinale) == 0` gate, read back through byro-dbg
   as `state: running` with aliases bound same-frame.
   The smoke's first run also caught a real defect: holding the
   `QuestStageState` write guard across condition evaluation deadlocks
   against `GetStage`-family reads (lock_tracker); the dispatcher is
   two-phase by necessity — a read-only walk collecting candidates,
   then the starts under the write guard.
3. **Phase 2 — conditions on event data + alias fill. Landed
   2026-10-07 (#5366).** The event payload is the wire format's four
   positional slots (`StoryEvent { reference_1, reference_2,
   location_1, location_2 }`; §3.2), with Subject≡R2-else-R1 and
   Target≡R1 preserving the Phase-1 context mapping over the
   corpus-verified slots. `RunOn::EventData` resolves its tag through
   the new `ConditionContext::event_data` (`EventDataSlots`) — R-tags
   to entities, L-tags to `None` (no location-as-entity runtime yet,
   a documented Phase-3+ deferral). `FromEvent` aliases fill from the
   same slots: the dispatcher records them per started quest in
   `StoryEventAliasFill` and requests an alias refresh, and
   `refresh_scene_actor_bindings` binds R-tagged fills directly
   (L-tagged stay unbound, surfacing as
   `StoryManagerEventUnavailable` in `quest.aliases`). `GetInFaction`
   (Skyrim 71) joined the M47.1 catalog. The CLOC producer keys on the
   cell's `XLCN` LCTN when one resolves (Skyrim's location
   granularity; wilderness grids keep the Phase-1 key) and carries
   old/new LCTN as L1/L2. Gates: the Rust e2e
   `mgsuspension_kill_gate_on_real_skyrim_content` (scripting crate,
   real authored node — matching killer starts the quest, non-player
   killer and non-faction victim each keep it stopped), the corpus
   floor `story_manager_skyrim_event_data_slot_floor`, and two live
   legs in `sm1-story-manager.sh` (`sm.event` raising the real marker;
   both non-matching KILL events refused). New console command:
   `sm.event`.
4. **Phase 3 — node policies + persistence. Landed 2026-10-07
   (#5366).** `DNAM` decodes (§3.3) into `SmNodePolicies`; the walk
   honors all three bits: random parents shuffle their child chain and
   stop at the first child that fires (the tutorial's "choose one of
   its child nodes randomly"), random quest nodes pick uniformly from
   the eligible pool, do-all-before-repeating round-robins the pool
   (fired marks persisted, wrapping when every eligible entry has run),
   and a PROCESSED quest node without Shares Event consumes the event
   ("the Story Manager will stop as soon as it finishes with that
   node" — keyed on processing per the CK rule text and its
   compatibility warning, not on a quest actually starting). The
   Phase-1 continue-always placeholder is gone. The live boot set is
   the authored one minus #5380's declines: `CWChangeLocationScenes`
   starts through its cataloged fn-56 gate and then consumes
   (`CRHoldExpansion` below it correctly stays stopped — pinned as
   the sm1 negative); `WIGreetingNodeSHARES`'s uncatalogued fn-145
   gate declines (the sm1 decline negative).
   `RNAM` windows gate re-fires through `StoryClock` (synced from
   `GameTimeRes` each frame) with last-fire timestamps in
   `StoryManagerNodeState` — save-registered, so a quickload cannot
   resurrect a fired radiant. Gates: the policy/round-robin/hours/
   consume/consume-without-start unit family, the node-policy corpus
   floor, the `story_manager_node_state_survives_save_load_and_still_
   gates` round-trip, the updated sm1 live legs, and the F5/F9 leg of
   `p5-quest-persistence.sh` extended to the SM-fired `WIGreeting`.
   Eligibility moved from `is_started` to `is_running` — stopped
   radiants re-fire, the rerun path.
5. **Phase 4 — FO4/SF dialect + breadth producers. Landed
   2026-10-08 (#5366).** The census (doc §3 field table): FO4/SF share
   Skyrim's `DNAM` bit vocabulary exactly (SF adds 0x50001/0x70001
   combinations of the same bits), author pools at scale
   (`RETravelQuests` 44), pair `RNAM` identically, and add the
   typed-raw `HNAM` hours-float + `MNAM` int tail. The Minutemen
   anchor is measured at a **14**-link pool (this doc's "16" was the
   pre-census guess), `HNAM` 72.0h, first `RNAM` 4320.0h, random +
   do-all. Every large FO4 pool gates on fn 576/56/77 (all outside the
   M47.1 catalog — full-path reachability corpus-checked), so the gate
   runs on the fully-reachable `SuperMutantConversationQuests`
   (4-pool, random+shares, its only conditions `GetInFaction` on the
   event's R1/R2 — the Phase-2 run-on live on FO4 content):
   `fo4_radiant_pool_cycles_on_real_content` crafts the faction,
   proves the faction-negative refuses, and cycles the whole pool
   across seeded fires. The `AHEL` producer landed at the #5367
   conversation-open surfaces (`OnStoryHello`: L1 session LCTN, R1
   greeter, R2 greeted). Producers still deferred per §6.4.
   Floors: `story_manager_fo4_dialect_floor`.

## 8. What this document does NOT decide

- Honouring Num-quests-to-run (`MNAM`) / Max-concurrent (`XNAM`) — the
  field semantics are named since #5420, but the Phase-3 runtime still
  relies only on the three decoded policy bits and `RNAM`, starting one
  quest per node.
- ~~Whether `StoryEvent` payloads grow a typed per-mnemonic enum or stay
  slot-based~~ — decided at Phase 2: the wire format's four positional
  slots verbatim (§3.2).
- The pre-Creation equivalents: Oblivion/FO3/FNV quest autostart is
  script-side (SCDA quest scripts / result scripts), already M47.3/M43
  scope; no SM runtime is retrofitted for them.
- Mod-tooling surfaces (viewing/editing the tree) — M50 territory.
