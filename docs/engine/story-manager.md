# Story Manager event dispatch

**Status**: Phase 0 + Phase 1 **landed 2026-10-07** (#5366): the
`SMBN`/`SMEN`/`SMQN` decode (`crates/plugin/src/esm/records/misc/story_manager.rs`
→ `EsmIndex.story_manager_nodes`), the `SmTree` fold + `StoryEvent`
dispatcher (`crates/scripting/src/story_manager.rs`), and two producers
(`KILL` at the combat death site, `CLOC` from the cell-loader location
contexts). Phases 2–4 remain open. This document remains the design
authority; it is the scoping pass for the Story Manager half of M43's
open scope ("Story Manager event payloads and search") and the M47.2
row's "ESM-native event dispatch / Story Manager" item.

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
| `DNAM` u32 | all | flags (stacked/random, shares-event, …) | **[open]** — observed values: 0x0, 0x1, 0x10001, 0x20000, 0x20001, 0x30001, 0x50001, 0x60000, 0x70001 (combinatorial: a low bit + 0x10000/0x20000/0x40000 bits) |
| `XNAM` u32 | all | small int (0/1/2; 0 dominant) | **[open]** |
| `QNAM` u32 | SMQN | small int 0–14 | **[open]** (candidate: repeat/priority; do not guess) |
| `SNAM` u32 | SMBN/SMEN | when present, points at another node — see §5 | **[open]** (distinct from the sibling use on SMQN) |
| FO4+: `HNAM`, repeated `NNAM` array, `RNAM` | SMQN | FO4 quest nodes can carry a *pool* of candidate quests (the 16-`NNAM` `MinutemenRecruitmentPostMin02` node) | **[open]** — FO4/SF dialect |
| SF: `MNAM` | SMQN | — | **[open]** |

A census artifact worth recording so the Phase-0 test floors aren't
misread: FO4 and Starfield keep their DIAL/INFO inside per-QUST child
groups (their top-level group lists have no `DIAL`), so a naive
records-at-group-top count sees "1 QUST" in those masters. The engine's
real walker recurses (`records/grup_walker.rs`); the Rust-side floors
will measure the true counts, and the Python census's FO4/SF
quest-resolution rates are meaningless for that reason (§4 note).

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

1. **`DNAM` flag bits.** Hypothesis from the CK tutorial's node
   settings: stacked-vs-random, shares-event, do-all-before-repeating.
   Method: cross the observed value set against the CK wiki's Story
   Manager node documentation, then confirm each bit by finding a node
   whose editor ID / in-game behavior pins it (the tutorial's own
   `DA08KillFriendNode` example is a good candidate).
2. **`XNAM` / `QNAM` ints.** Small observed domains (0–2, 0–14).
   Method: correlate with CK-exposed node properties on named nodes.
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

One new transient marker family mirroring `events.rs`:

```rust
pub struct StoryEvent {
    pub mnemonic: SmEventMnemonic,   // per-game table (§3.1)
    pub subject: EntityId,           // event data, slot 1 (killer, actor…)
    pub object: EntityId,            // slot 2 (victim, target…)
    pub location: Option<FormId>,    // LCTN where the game carries one
    pub extra: SmallVec<[FormId; 4]>,
}
```

`story_manager_system` (post-combat/post-transition stage, pre-cleanup):
for each `StoryEvent`, look up the mnemonic's root, walk children in
sibling order evaluating each node's `CTDA` set through the M47.1
evaluator extended with run-on targets `EventSubject` / `EventObject` /
`EventLocation`. A passing SMQN starts its quest via the existing quest
lifecycle and fills aliases from the event data through the P4 alias
machinery. Traversal honors the §5-settled flag bits (shares-event →
continue; random → pick one child; do-all-before-repeating → node-local
round-robin state). `SendStoryEvent` (already catalogued in the SKSE
compatibility surface) lowers onto the same marker — that is the
`SCPT` event node's producer.

### 6.4 Producer inventory (what can fire today)

Phasing follows this, not the catalog order — start where the engine
already has the surface:

- **Ready-ish**: `KILL` (combat death exists, M41/#4414),
  `CLOC` (cell-transition steppers), `AHEL`-adjacent activation
  (P0), `LEVL` (ActorValues/setav), `SCPT` (Papyrus/SKSE-compat
  SendStoryEvent).
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
   (§5). Gate note: the *unit* gates landed (synthetic tree dispatch,
   sibling ordering, cycle termination, failing-branch gating,
   producer dedup); the *live* Skyrim smoke (an SM-started quest
   demonstrably starting in-engine) is still owed and belongs with
   Phase 2's condition work, where a node conditioned on event data can
   be told apart from an unconditional start.
3. **Phase 2 — conditions on event data + alias fill.** Evaluator
   run-on extension + P4 alias integration. Gate: a quest whose node
   conditions on event data (the tutorial's killer-conditions shape)
   starts only for matching events.
4. **Phase 3 — node policies + persistence.** Random/shares/repeat
   flags, save/restore of node state, the F5→door→F9 soak leg extended
   to an SM-fired quest.
5. **Phase 4 — FO4/SF dialect + breadth producers.** `NNAM` pools,
   `HNAM`/`RNAM`/`MNAM`, remaining producers as their subsystems land.
   Gate: FO4 Minutemen-recruitment-style radiant (a 16-quest pool node)
   cycling different quests across fires.

## 8. What this document does NOT decide

- The `DNAM`/`XNAM`/`QNAM` semantics (§5's job; nothing here relies on
  the hypothesis being right).
- Whether `StoryEvent` payloads grow a typed per-mnemonic enum or stay
  slot-based — decide at Phase 1 with two producers live.
- The pre-Creation equivalents: Oblivion/FO3/FNV quest autostart is
  script-side (SCDA quest scripts / result scripts), already M47.3/M43
  scope; no SM runtime is retrofitted for them.
- Mod-tooling surfaces (viewing/editing the tree) — M50 territory.
