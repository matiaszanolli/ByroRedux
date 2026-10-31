# Story Manager event dispatch

**Status**: Phases 0–4 **landed 2026-10-07/08** (#5366); M43 **closed 2026-10-30** with the producers and gameplay below. The runtime is
complete for the Skyrim dialect and the FO4/SF decode is in: the
`SMBN`/`SMEN`/`SMQN` parse (`crates/plugin/src/esm/records/misc/story_manager.rs`
→ `EsmIndex.story_manager_nodes`, FO4/SF tail typed-raw), the `SmTree`
fold + `StoryEvent` dispatcher (`crates/scripting/src/story_manager.rs`),
three producers (`KILL` at combat death, `CLOC` at `XLCN` LCTN
granularity, `AHEL` at conversation open — activation greeting and
force-greet), `RunOn::EventData` over the corpus-decoded R1/R2/L1/L2
slots, `FromEvent` alias fills, and the `DNAM` policies with `RNAM`
reset windows over save-persistent node state. M43 follow-ups
(2026-10-30): the walk honors *Num quests to run* (`MNAM`) and *Max
concurrent quests* (`XNAM`); `ADIA` (NPC-to-NPC random conversations — the
largest event family, with the CK's interrupt hold), `LEVL`, `ASSU`, `AIPL`,
`SKIL`, `JAIL`, `CHRR`, `REMP`, `CAST`, `CRFT` and FO4's `LCLD` joined
`KILL`/`CLOC`/`AHEL`/`SCPT` as producers (§6.4). The rest of M43's leftover list — `DEAD`, the natural `AHEL`,
`ARRT`/`ESJA`, `NVPE` and FO4's `AOBJ`/`HACK`/`LOCK`/`IRON`/`OAAT`/`TMEE` — has producers
as of 2026-10-30 (§6.4 states which rules are the engine's own policy). The corpus's `FromEvent`
fills and node conditions read only `R1`/`R2`/`L1`/`L2`, so a form slot
(`F1`) is only needed by scripts. `HNAM` is consumed as the FO4+ node-level reset window (§3). Unconsumed fields: `FNAM` (xEdit
declares it `wbUnknown` in both the TES5 and FO4 definitions; always `2` in the corpus), `QNAM` (a
redundant pool count), `UNAM` (FO76 only), and `DNAM 0x2` (an editor-side
warning). This document remains the design authority; it is the
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
| `SNAM` u32 | all | **previous**-sibling node FormID (#5385) — the head is the member with `SNAM == 0`, and the fold follows the inverse edges | **[verified]** — xEdit `wbFormIDCkNoReach(SNAM, 'Previous Node', …)` on every game; the same-parent test (441/443 Skyrim, 218/219 FO4, 366/366 Starfield) holds in both directions and could NOT tell next from previous — the direction evidence is the DLC bottom-append shape (7/2/6 FO3-era + 5 FO4 DLC nodes SNAM onto a vanilla node nothing points at, the CK's "below all other entries"), FormID age (the target is older in 302/441 Skyrim, 135/218 FO4, 191/366 Starfield edges), and the CK tutorial's KILL head being `DA08KillFriendNode`, whose `SNAM` is 0 |
| `ENAM` char[4] | SMEN | event mnemonic; exactly one SMEN per mnemonic per master (24 Skyrim / 17 FO4 / 20 Starfield) | **[verified]** structure; per-mnemonic mapping in §3.1 |
| `CITC` u32 + `CTDA`×N + `CIS2` | all | node conditions (existing CTDA representation) | **[verified]** shape |
| `NNAM` u32 | SMQN | the QUST this node starts | **[verified]** — Skyrim 425/448 resolve to QUST records via EDID cross-check (e.g. `MS05KingOlafsFestivalStarter` → its QUST) |
| `DNAM` u32 | all | node-policy flags — 0x1 random, 0x10000 do-all-before-repeating, 0x20000 shares-event (Phase-3 decode, §3.3); 0x2 warn-if-no-child-quest-started, 0x40000 the Num-quests-to-run checkbox | **[decoded]** — #5420 closed the last two bits from xEdit `TES5.pas`; the runtime still consumes only the Phase-3 three |
| `RNAM` f32 | SMQN | per-quest hours-until-reset window, follows its `NNAM` | **[decoded]** (Phase 3; §3.3) |
| `HNAM` u32 (as f32) | SMQN, FO4+ | **Hours until reset** (xEdit FO4+/SF1) — 72.0 on the Minutemen node, 0.3–24.0 elsewhere | **[consumed, inferred]** — the node is passed over until `HNAM` hours after it last started a quest (`SmNodeRecord::reset_hours`, `SmNodeRuntime::last_node_fire_hours`, `FORMAT_MAJOR` 121). **Sources:** xEdit `wbDefinitionsFO4.pas` — `wbFloat(HNAM, 'Hours until reset')`, no scale, unlike the per-quest `RNAM` (`1/24`); the CK's quest-level meaning of the same name; and a 2026-10-30 census of Fallout4.esm — 17 `SMQN`s, values 0.5, 1 (×7), 12 (×2), 24 (×3), 48, 72 (×2), 140, all on random radiant pools (`RETravelQuestsNIGHT` 12, `REChokepointQuests` 24, the `EE_*` ambient conversations 0.5-1, `MinutemenRandomEvents` 72). That it is a *node* cooldown is inferred from the name and those values; **no in-game timing test has confirmed it**. Census also fixed `RNAM`'s unit: Skyrim's raw 576 (250 of 296 links) and FO4's (180 of 296) read 24 h at the xEdit scale, so the stored float is hours × 24 (#5386, unchanged) |
| `MNAM` u32 | SMQN, FO4 rare / SF common | **Num quests to run** (xEdit FO4/SF1) — observed 1–22; present exactly when `DNAM & 0x40000` is set (0 exceptions over Skyrim, FO4, Starfield) | **[consumed]** — the walk starts up to `MNAM` pool quests per pass (`SmNodeRecord::num_quests_to_run`) |
| `XNAM` u32 | all | **Max concurrent quests** (xEdit `TES5.pas:7056-7111`) — observed 0 (dominant) up to 57; nonzero with or without the Num-quests box | **[consumed]** — nonzero caps the pool's running quests, `0` = uncapped (`SmNodeRecord::max_concurrent_quests`); enforced in phase A and again at start |
| `QNAM` u32 | SMQN | **Quest Count** — the `NNAM` array's `SetCountPath`; census `QNAM == count(NNAM)` on 448/448 Skyrim, 219/219 FO4, 354/354 Starfield | **[named]** #5420 (not a priority/repeat knob) |
| `FNAM` u32 | SMQN, TES5 | **unknown** — xEdit's current `wbDefinitionsTES5.pas` and `wbDefinitionsFO4.pas` both declare `wbUnknown(FNAM)` (an earlier reading of "24 Hours Till Reset" does not survive them); 7 vanilla quest links in 5 Skyrim nodes, value **always 2**, none in Fallout4.esm (2026-10-30 census) | **[unknown]** — raw; deliberately not consumed |
| `UNAM` u32 | SMQN, FO76 | **Priority**, per quest (xEdit FO76) | **[named]** #5420 (raw; no runtime consumer) |
| `SNAM` u32 | SMBN/SMEN | previous-sibling link, as on `SMQN` (#5385: xEdit `Previous Node` on every game) | **[verified]** |
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
| `R2` | `0x3252` | second reference — KILL killer, SCPT ref2 (`CAST`'s `akSpellTarget` is documented as always none) |
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
indicated number of Game Hours has passed" — following its `NNAM`.
#5386: the stored float is hours × 24 (xEdit `wbFloat(.., 1/24)` writes
value / scale); the decode divides, so the dominant raw 576 is 24 h and
raw 1152 is 48 h (`MQ304SovngardeScenes`'s raw 2.4/4.8/4.8 are the
0.1/0.2/0.2 h ambient-scene windows; the `WEBountyCollector*` holds
carry raw 1152 = 48 h; most RNAMs are 0.0 = ignored). `0x2`
(Warn if no child quest started) and `0x40000` (the Num-quests-to-run
checkbox) are named from xEdit since #5420, raw in
`SmNodeRecord::dnam` beside the decoded `SmNodePolicies` bits. Floored
by `story_manager_skyrim_node_policy_floor` (`parse_real_esm.rs`).

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
continue past a quest node (else a node that fires a quest consumes the event), random →
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
  conversation — activation greeting and force-greet, Phase 4), `SCPT`
  (the Papyrus `SendStoryEvent` native, M47.2), and `ADIA` (below).
- **`ADIA`, the Actor Dialogue Event** (M43, `systems/random_conversation.rs`).
  The largest family by far: it gates 786 of Skyrim.esm's SM-started quests
  and 298 of Fallout4.esm's (the NPC-to-NPC conversation scenes), and was the
  one thing in the tree nothing produced. The CK's rules (*Scenes →
  Controlling Random Conversations*) are implemented with every number a
  game setting: an actor whose active package allows *Random conversations*
  (`PKDT` interrupt flag `0x2`, `PackRecord::allows_random_conversations`)
  and is running Sandbox, Guard, Patrol or Eat looks, every
  `fAISocialTimerForConversationsMin..Max` seconds, for the nearest valid
  partner inside `fAISocialRadiusToTriggerConversation` (`…Interior` indoors;
  a Sandbox package's own radius replaces it), rolls
  `fAISocialchanceForConversation`, and raises `ADIA` with R1 = the initiator,
  R2 = the partner and L1 = the session LCTN (`OnStoryDialogue(akLocation,
  akActor1, akActor2)`). A valid partner also allows random conversations, is
  not in a running scene (`actors_in_running_scenes`), in combat, dead or
  unconscious, and has not talked to the initiator within
  `fAItalktosameNPCTimer`. The system only runs when the tree has an `ADIA`
  root (so never on Oblivion/FO3/FNV) and `BYRO_NO_RANDOM_CONVERSATIONS=1`
  leaves it out. **Live-gated 2026-10-30**: Skyrim exterior (Whiterun grid,
  radius 2), 34 s in — `random conversation: 8358 -> 8443`, then
  `started quest 0x00016FA6 ('DialogueGenericSceneSpecial01') via node
  'GenericScenesSpecial' on 'ADIA' event` and its scene `DGSceneSpecial01`
  playing phases 0 → 3 to *Finished*. Once a roll succeeds the pair is
  **held** (`ConversationHold`, the CK's "interrupt behavior"): both ambient
  packages are suspended and the package system skips them, the initiator
  walks to `iAISocialDistanceToTriggerEvent` of the partner on the shared
  navmesh step (a stuck initiator starts from where it stands;
  `BYRO_NO_AI_LOCOMOTION=1` raises the event without walking), the partner
  stops and faces it, `ADIA` is raised on arrival, and after
  `fAISocialTimerToWaitForEvent` the hold continues while a scene runs for
  either actor and otherwise releases both. The CK's *complex scene* flag
  holds back every new conversation and hello while a `ComplexSceneMarker` is placed in a
  loaded cell (`complex_scene_active`; the marker is the `DOBJ` `CMPX` object, Skyrim's
  `ComplexSceneMARKER` — 5 placements in Skyrim.esm, none in Fallout4.esm; xEdit's `SCEN` has no such
  flag). **Sources:** CK *Scenes* ("not if the global complex scene flag is set"),
  *SetSceneIsComplex* (a count of the markers in attached cells), xEdit (`CMPX` default object). What
  other systems the flag gates, and the console override, are not documented and not modelled. The scene's Phase 0 waits "until the initiating actor reaches
  `fAIMinGreetingDistance`" (150), nearer than Skyrim's authored
  `iAISocialDistanceToTriggerEvent` (200), so the event is raised at the
  nearer of the two and the scene begins once that holds. The settings in
  play are the master's: Skyrim.esm authors radius 500 / 500, chance 10 % /
  10 %, timers 10-30 s and trigger distance 200 (the CK page's 2000 / 25 % /
  30-60 s are its documented defaults, used only where a master omits one;
  the same-NPC timer and the 0.5 s wait are not authored in Skyrim.esm).
  The CK does not say which valid partner a roll picks; the nearest is used.
  The live run's pair was already inside the trigger distance, so the walk
  itself is covered by the unit test (teleport into range) and the shared
  step, not by that run.
- **`AOBJ`, Attraction Object** (M43, `systems/attraction.rs`). Fallout 4's attraction system (CK
  *AttractionRule*, *SetAttractionActive*, *Quest Script*): a keyword's `DATA` names an `AORU` rule (radius, min
  and max delay, requires line of sight, combat target — xEdit `wbAORU`/`wbKYWD`), and an object and an actor
  that both carry the keyword react to each other. Fallout4.esm has 8 rules on 15 keywords, carried by
  activator / flora / furniture / container bases and NPC bases (census 2026-10-30). New plumbing: `AORU` is a
  dispatched record (`EsmIndex::attraction_rules`), `KywdRecord::attraction_rule`, and the `KWDA` of activator,
  flora and container bases (`placed_object_keywords`; furniture's also lands there). A live NPC and a placed
  object sharing an attraction keyword inside the rule's radius (and in line of sight when required) raise
  `OnStoryAttractionObject` — actor `R1`, object `R2`, location `L1`, `V1` = `abCommanded` = 0 — after a delay
  drawn between the rule's minimum and maximum, repeating while they stay. **Engine policy:** that reading of
  "delay". **Not modelled:** rules with *Combat Target* (nothing says what it changes, so the workshop-resource
  rule is left out rather than guessed), `SetAttractionActive` switching an object off, race keywords, the
  workshop *commanded* path. Unit-tested; the parse plumbing is **live-gated** against the real master by
  [`m43-attraction.sh`](../smoke-tests/m43-attraction.sh) (2026-10-31: 13 modelled keywords, 22 object bases and 118
  NPC bases from `Fallout4.esm`, 2 attraction objects placed in `AndrewStation01`). An actor reacting to an object
  in the running game has not been observed.
- **`HACK`, Hack Terminal** (M43, `lockpick.rs`): a locked terminal (a reference whose base is a `TERM`) at lock
  level 0-100 in a game whose tree has a `HACK` root is hacked on activation — no item and no word game, so it
  always succeeds (engine policy; the Hacker perk ranks are not modelled). It unlocks and raises
  `OnStoryHackTerminal(akComputer, abSucceeded)`: the terminal in `R1`, success in `V1`. The terminal's menu
  and body text are a separate surface and are not built. Unit-tested; not run in the live engine.
- **`TMEE`, Mine Explosion** (M43, `systems/mines.rs`). `OnStoryMineExplosion(akVictim, akAttacker)`. The
  game's data defines a mine as a projectile with the *Alt. Trigger* flag, a trigger distance, a fuse and an
  explosion (xEdit `wbPROJ`; the layout was verified on `Fallout4.esm`: the frag mine is flags `0x66`,
  proximity 100, timer 1.5, explosion `fragMineExplosion` — damage 100, inner radius 100, outer 500), and
  Fallout4.esm places **390** such mines (340 frag). The parsers now decode FO4's `PROJ` `DNAM` and the
  Skyrim/FO4 `EXPL` `DATA` (the old reads only matched the FO3/FNV layout). A live actor within the proximity
  distance trips a placed mine (nearest = victim), the record's timer burns as a fuse, then every live actor in
  the outer radius is hit (full damage to the inner radius, linear to nothing at the outer radius), `TMEE` is
  raised with the victim in `R1` (`R2` none: a placed mine has no owner) and the mine is hidden and disabled in
  the persistent ledger. **Engine policy:** no friend/foe or detection delay (`fMinesDelayMin`,
  `fMineExteriorRadiusMult`, `iProjectileMineShooterCanTrigger` unused), the timer read as a post-trigger fuse,
  the falloff curve. **Not modelled:** player-placed mines, force/stagger, the explosion's visuals and sound.
  Unit-tested and **live-gated** by [`m43-mines.sh`](../smoke-tests/m43-mines.sh) (2026-10-31, release build,
  Fallout 4 interior `AndrewStation01`: `mine.status` tracked all 9 frag mines, the player was walked onto one,
  and the log read `tripped` then `detonated` 2 s later with the mine `spent`).
- **`OAAT`, Actor Attach** (M43, `story_events::raise_actor_attach_story_event`, called where a placed actor
  is stamped): `OnStoryActorAttach(akActor, akLocation)` — the actor in `R1`, the session location in `L1`,
  raised for each actor placed into a loaded cell. Unit-tested; not live-gated.
- **`LOCK` and `IRON`, Fallout 4's player actions** (M43; **engine policy**, no local source documents
  FO4's lock or aim rules). `lockpick.rs`: a lock of level 0-100 (the CK's FO4 `GetLockLevel` tiers; 251-255
  are Barred / Chained / Terminal Only / Inaccessible / Requires Key) is a target and opens for a player
  holding a pick (the misc item whose editor id is `BobbyPin`, or `Lockpick` in older games — checked
  against `Fallout4.esm`); no minigame, the pick is not used up, no tier needs a perk, and it applies only
  on a title whose tree has a `LOCK` root. A picked lock is recorded like a key unlock and raises `LOCK`
  (`OnStoryPickLock`: player in `R1`, the lock's reference in `R2`). `systems/iron_sights.rs`: holding the secondary-attack button (right mouse, `Block`)
  with an equipped weapon that uses ammo is aiming, and the rising edge raises `IRON` (player in `R1`); no
  zoom, spread or camera change. Unit-tested; not live-gated.
- **`ARRT` / `ESJA`, arrest and jail escape** (M43, `jail.rs`). Skyrim's guard dialogue calls
  `akSpeaker.GetCrimeFaction().SendPlayerToJail()` (CK *Creating a Custom Crime Faction*); that native is
  now a VM call (`sendplayertojail`, `abRemoveInventory`/`abRealJail` both default true). It raises `ARRT`
  (`OnStoryArrest`: the speaker as guard in `R1`, the player in `R2`, the location in `L1`; `aiCrime` rides
  `V1` as `-1`, the CK's "none", since the call does not know the crime) and, for a real jail, starts the
  sentence in the jail whose exit is the faction's `JAIL` "Exterior Jail Marker": the faction's crime gold
  over `iCrimeDaysInPrisonMod` days, the gold cleared, witnesses forgiving, the player's inventory moved
  into the faction's `PLCN` container (nothing is taken without one: the goods would vanish), the player
  sent to the cell marker, and `JAIL` raised with the guard in `R1`. `abRealJail = False` stops at `ARRT`.
  A player serving a sentence who reaches an exterior cell after having been seen in the jail interior has
  escaped (`detect_escape`, run by `jail_bed_system`): the sentence ends and `ESJA` is raised with the
  location. **Not modelled:** the escape bounty (`CRVA`'s escape gold — the sentence does not remember its
  faction), the guard-initiated *Arrest* with its force-greet and the crime type, the quest-item hiding of
  a real jail, and a guard arresting an actor other than the player. Unit- and VM-tested; not live-gated.
- **`NVPE`, New Voice Power** (M43, `words_of_power.rs`). CK *New Voice Power*: "whenever the player
  obtains a new Word of Power". `Game.TeachWord` and `Game.UnlockWord` (one `Effect::ObtainWord`: they
  differ only in the shout item `TeachWord` adds, which is not modelled) record the word in the saved
  `KnownWords` (`FORMAT_MAJOR` 120) and raise `NVPE` with the player in `R1` the first time a word is
  new. The word itself is a form the quests never read and no event slot carries. Shouting, dragon
  souls and a shout menu are not modelled; nothing in the engine yet calls these natives except scripts
  (the word walls are the Papyrus `WordWallScript`). Unit- and VM-tested; not live-gated.
- **`DEAD`, Discover Dead Body** (M43, `systems/dead_body.rs`). CK *Dead Body*: "an actor discovers a dead
  body", with the discoverer, the dead actor and the location. No setting or page gives a discovery
  distance (`fCheckDeadBodyTimer` is a Fallout leftover absent from the master), so the one perception model
  the engine has is reused: a living, placed, non-player actor discovers a body it *sees* the way the ambient
  hostility pass sees a target — inside `fSneakMaxDistance` (scaled outdoors) with a clear physics line
  (`actor_sees_corpse`). This is engine policy, not the original's rule: no view cone or detection meter,
  and the player is not a discoverer (its view cone is not modelled). The nearest seeing actor discovers each
  body once, `R1` the discoverer, `R2` the corpse, `L1` the location. Unit-tested; not live-gated.
- **Natural `AHEL`** (M43, `systems/random_conversation.rs`). The CK's *Actor Hello Event*
  is the random-conversation attempt made by an actor "moving past" another: the initiator is
  moving, not in a scene and has picked up a new headtrack target; the target is not the player
  and not in a scene. The engine has no ambient head-tracking, so the new target is the partner
  an attempt picks (the nearest valid actor inside the social radius) when it differs from the
  one last greeted; timers, radius, chance and the same-pair cooldown are the conversation
  settings. An actor is moving when it travelled 10 units since the previous one-second pass
  (the CK gives no speed). `AHEL` is raised on the spot with `R1` the initiator, `R2` the
  partner, `L1` the location — nobody is held or walked. Unit-tested; not live-gated.
- **Also landed (M43, 2026-10-30)**: `LEVL` (new level in `V1`), `ASSU`
  (victim `R1`, attacker `R2`, `L1`, crime flag `V1`), `AIPL` (Player Add Item
  — container `R2` from a loot transfer, `L1`, acquire type `V1`; pickups
  carry no container), `SKIL` (once per skill that gains levels, from
  `Game.AdvanceSkill`), `JAIL` (a sentence starting, gold in `V1`), `CHRR`
  (a script changing a relationship rank: both actors in `R1`/`R2`, old and
  new rank in `V1`/`V2`) and Fallout 4's `LCLD` (the SMEN editor ids —
  `MinRecruitLocationLoadedStub` and kin — name it *Location Loaded*; its
  quests fill one `L1`; raised with `CLOC`, since the engine has one moment for
  the cells loading and the player arriving). A census
  of the event slots the real quests read (`R1`/`R2`/`L1`/`L2` only) picked
  what each carries: `AIPL` quests read the container (`R2`) and location,
  `REMP` the *item reference* (`R2`), `CAST` caster/location (its target is always none), `DEAD`
  the corpse (`R2`), `CHRR` both actors.
- **Gameplay built for the Story Manager's sake (M43)**: `REMP` needed an item
  drop, `CAST` a caster on the cast, `CRFT` crafting — none existed.
  *Drop*: the inventory menu's Drop action (and `inv.drop`) removes one item
  and places a created reference of its base at the player's feet (the same
  mechanism `PlaceAtMe` uses; the pickup path takes it back); `REMP` carries
  that reference in `R2` and is raised once it has spawned
  (`dropped_item_story_event_system`). Live on Skyrim: the drop spawned
  `FF800000` and started `WIRemoveItem05` through `WIPlayerRemoveItemAnyNode`
  (`m43-item-drop.sh`; the radiant nodes roll a random percent, so the gate
  retries). *Cast*: `cast_spell` with a caster raises `CAST` (caster `R1`, location `L1`;
  the CK's `OnStoryCastMagic` page says `akSpellTarget` "always returns none", so `R2` is
  never set); the `magic.cast` console stands in for the player's cast. The
  `CAST` quests are the world-interaction reactions to a spell cast at an
  actor (`WICastMagic…`); there is still no player cast input, so only
  scripts and the console cast. Live: a console cast raised `CAST` (caster,
  target) — no quest started, because `WICastMagic01` requires its `Location`
  alias (`L1`) and the spawn cell of the grid run authors no location, so
  `PlayerLocation` was `None`; a hand-raised `CAST` with a location did not
  start one either (the nodes also gate on cooldown globals and a random roll),
  so the live gate for this producer is the raise itself, not a started quest.
  The crafting page was opened in the running game (`craft open`, the same
  request activation makes) and shows the bench heading, each recipe with
  ingredients held against needed, and a Craft button that is disabled while
  an ingredient is missing. *Crafting* (`crafting.rs`): Skyrim `COBJ`
  recipes (ingredients `CNTO`, yield `NAM1`, bench keyword `BNAM`, perk
  conditions `CTDA`; 592 of 601 install) and the `KWDA` of every `FURN`
  (typed since this change) make a placed furniture a bench when it carries
  a recipe's keyword. Activating a bench ("Craft") opens the native crafting
  page: the recipes the bench offers that the player's conditions allow, each
  with ingredients held against needed and a Craft button; `craft list` /
  `craft make <recipe>` drive the same two halves at the nearest bench. A
  craft consumes the ingredients, adds the yield and raises `CRFT` with the
  bench in `R1`. Live on Skyrim: crafting leather strips at the nearest
  bench started `WICraftItem01` through `WICraftItemNode`
  (`m43-crafting.sh`). A craft at a forge (`CraftingSmithingForge`,
  `CraftingSmithingSkyforge`) raises Smithing by UESP's `3 × item value^0.65 + 25`
  through the `AdvanceSkill` queue, so it can raise `SKIL`. **Improving** (the perk-improved
  yields — they act on tempering, not on the forge craft): a grindstone or armor workbench lists the
  improvements of the items the player holds, raises one held copy's quality (a per-instance `quality` in the
  saved item pool, `FORMAT_MAJOR` 122; a copy out of a stack is split off) and earns UESP's
  `3.8 × Δvalue^0.5 × ΔQ^0.5` XP. The quality is the highest unlocked by the player's Smithing level in UESP's
  table (*Skyrim:Smithing*, "Item Quality": six tiers, with and without the material perk); an improved weapon
  hits harder by the tier's rating bonus. The data supplies the perks: the recipes carry `EPTemperingItemIsEnchanted
  != 1 OR HasPerk(ArcaneBlacksmith)` (condition 659, new in the catalog) and the material perks are
  `Mod Tempering Health` (entry point 76) × 2.0 conditioned on the item's keyword, composed with the item as the
  tab subject. The quality table is UESP's whole 44 levels (levels 7-44 are displayed "Legendary" and need a
  Smithing level above 100, i.e. Fortify Smithing — the level read is the actor value's current value,
  modifiers included). The inventory page shows an improved item's quality, its bonus (`+N damage`, or `+N armor
  rating` — the cuirass column for the biped body slot, bit 2 of the Skyrim `BOD2` flags) and its raised value.
  **Engine policy / not modelled:** the armor-rating bonus has nothing to act on (the engine has no Skyrim armor
  mitigation — it is stored and shown, not applied), the animation, and the "with perk" column is read as "any
  material perk applies". Fallout 4's component-based `COBJ` (`FVPA`)
  and the forge-craft animation are also unmodelled. **Live-gated** by
  [`m43-tempering.sh`](../smoke-tests/m43-tempering.sh) (2026-10-31, release build, Skyrim `HaltedStreamCamp01`'s
  grindstone: `craft benches` found the wheel, `craft list` offered `Improve Steel Sword`, `craft make` improved the
  held sword to Fine, the item count fell by exactly one ingot — no copy — and the list then read "no better
  quality"). That run also found that nothing created the `ItemInstancePool` at boot (the instance machinery was
  dormant outside tests), now initialised with the world.
- **The leftover events and their payloads** (per-mnemonic quest counts,
  2026-10-30, quests reachable under each `SMEN` root — Skyrim: `ESJA` 2,
  `DEAD` 2, `ARRT` 2, `AHEL` 2, `NVPE` 1; FO4: `AOBJ` 16, `AHEL` 9, `OAAT` 4,
  `HACK` 3, `LOCK` 3, `TMEE` 1, `IRON` 1):

  **FO4's event payloads are now a primary source**: the game's own compiled `Scripts\Quest.pex`
  (`Fallout4 - Misc.ba2`, read with `cargo run -p byroredux-pex --example pex_list_fns`) carries every
  `OnStory*` signature, so the `R1`/`R2` slots of these events are no longer guesses:
  `HACK` (`SMEN` editor id `HackComputer`) = `OnStoryHackTerminal(akComputer, abSucceeded)`;
  `TMEE` (`TriggerMineExplosionEvent`) = `OnStoryMineExplosion(akVictim, akAttacker)`;
  `IRON` (`IronSights`) = `OnStoryIronSights(akActor, akWeapon)`; `LOCK` = `OnStoryPickLock(akActor,
  akLock)`; `OAAT` = `OnStoryActorAttach(akActor, akLocation)` (its `SMEN` authors no editor id; the code is
  read from its child quests, `REDeadActorAttach` and kin); `AOBJ` = `OnStoryAttractionObject(akActor,
  akObject, akLocation, abCommanded)` (no editor id either, so the code is inferred from the name pair and
  the CK's attraction system — not the workshop, as first guessed). Every one of the six now has a producer.

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
   (children ordered by the authored `SNAM` chains; since #5405 every
   member of a group is placed — vanilla groups are not always one linked
   list (`DungeonNode` 50 members, several `SNAM == 0` heads), so members
   are replayed in FormID order, inserted after their predecessor, with
   no-predecessor members on top; cycles break at the lowest FormID, and
   a visited bitmap keeps malformed mods from looping dispatch), the `StoryEvent` Pattern-B marker
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
   `StoryManagerEventUnavailable` in `quest.aliases`). #5394: the fill
   is saved (format v35) — each R slot also records its reference FormId
   (PlayerRef `0x14` for the player), and the refresh re-resolves it among
   the resident candidates whenever the session id is gone or names a
   different reference, so a restored SM quest re-fills its event aliases
   after a load or a cell reload. `GetInFaction`
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
   and a quest node without Shares Event that FIRES A QUEST consumes
   the event (#5404: the CK tutorial's "if a node actually manages to
   fire a quest, the event will not go through the rest of the tree";
   the earlier processing-keyed reading let an always-passing node with
   an exhausted pool swallow every event). The
   Phase-1 continue-always placeholder is gone. The live boot set is
   the authored one minus #5380's declines: `CWChangeLocationScenes`
   starts through its cataloged fn-56 gate and then consumes
   (`CRHoldExpansion` below it correctly stays stopped while
   `CWChangeLocationScenes` still has a startable scene — pinned as
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
