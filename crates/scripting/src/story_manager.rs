//! Story Manager dispatch runtime — #5366 Phases 1–2.
//!
//! Folds the parsed `SMBN`/`SMEN`/`SMQN` node map
//! ([`EsmIndex::story_manager_nodes`]) into an [`SmTree`] whose children
//! are ordered by the authored `SNAM` sibling chains (#5385: `SNAM`
//! names the PREVIOUS sibling; the head is the `SNAM == 0` member and
//! the fold follows the inverse edges), then dispatches
//! raised [`StoryEvent`]s: look up the event mnemonic's `SMEN` root,
//! walk children in stack order evaluating each node's CTDA set through
//! the shared M47.1 evaluator, and start a passing quest node's quest
//! through the canonical [`QuestStageState`] lifecycle (the same path
//! Start Game Enabled and Papyrus `Start()` use).
//!
//! Phase-2 additions on top of the Phase-1 walk: the event carries its
//! data as the four positional slots the wire format tags (`R1`/`R2`
//! references, `L1`/`L2` locations — see [`EventDataSlots`]), so a node
//! CTDA with `RunOn::EventData` resolves its tag against the raising
//! event, and a started quest's `FromEvent` (`ALFE`/`ALFD`) aliases
//! fill from those same slots through the P4 alias refresh (the fill is
//! recorded in [`StoryEventAliasFill`] here and consumed by
//! `refresh_scene_actor_bindings`, which owns the binding table).
//!
//! Node policies (Phase 3): traversal order and continuation follow the
//! authored `DNAM` bits — random parents shuffle their child chain, a
//! quest node without `Shares Event` that fires a quest consumes the
//! event once it finishes (#5404), quest pools honor do-all-before-repeating round-robin
//! and per-quest `RNAM` reset windows. (The Phase-1 "continue past a
//! firing node" default was this rule's placeholder until the bits
//! decoded.)
//! - **Subject/Object derive from the slots.** `ConditionContext`'s
//!   subject is R2 when the event has one (KILL killer — the doer),
//!   else R1 (CLOC actor); its target is R1 (KILL victim). This is the
//!   Phase-1 mapping verbatim, now expressed over the corpus-verified
//!   slots.
//!
//! Producers: the engine wires two — `KILL` at the combat death
//! transition (`byroredux/src/combat.rs`: R1 slain, R2 aggressor) and
//! `CLOC` through [`emit_change_location_on_key_change`], fed the
//! session's cell/worldspace identity plus LCTN each frame by an
//! engine-side system. Papyrus `SendStoryEvent` lowers onto the same
//! marker when its producer lands.

use byroredux_core::ecs::resource::Resource;
use byroredux_core::ecs::sparse_set::SparseSetStorage;
use byroredux_core::ecs::storage::{Component, EntityId};
use byroredux_core::ecs::world::World;
use byroredux_plugin::esm::records::condition::ConditionList;
use byroredux_plugin::esm::records::{SmNodeKind, SmNodePolicies, SmQuestLink, SmNodeRecord};
use std::collections::HashMap;

use crate::condition::{evaluate, ConditionContext, EventDataSlots};
use crate::quest_stages::{QuestDefinitionRegistry, QuestFormId, QuestStageState, QuestStatus};
use crate::scene::SceneActorBindings;

/// One raised story event — the ECS replacement for the engine's story
/// event queue entry. Transient marker (**Pattern B**, #2672): it has
/// exactly one owning consumer, `story_manager_dispatch_system`, which
/// snapshots and drains it at its head. Producers attach it to the
/// entity the event most concerns (the slain actor for `KILL`, the
/// entering actor for `CLOC`).
///
/// The payload is the wire format's positional slots (corpus-verified
/// against Skyrim + DLCs, #5366 Phase 2): R1/R2 are the event's two
/// references and L1/L2 its locations. Which thing each slot holds is
/// per-mnemonic and follows the Papyrus `OnStory*` parameter order —
/// `KILL`: R1 victim, R2 killer; `CLOC`: R1 actor, L1 old, L2 new.
#[derive(Debug, Clone, Copy)]
pub struct StoryEvent {
    /// 4-byte event mnemonic (`"KILL"`, `"CLOC"`, …) matching the
    /// `SMEN.ENAM` catalog parsed off the master.
    pub mnemonic: [u8; 4],
    /// `R1` — first event reference (KILL victim, CLOC entering actor).
    pub reference_1: EntityId,
    /// `R2` — second event reference (KILL killer, CAST spell target).
    pub reference_2: Option<EntityId>,
    /// `L1` — first event location (CLOC location left).
    pub location_1: Option<u32>,
    /// `L2` — second event location (CLOC location entered).
    pub location_2: Option<u32>,
    /// M47.2 — a script-raised event's payload (`Keyword.SendStoryEvent`);
    /// `None` for engine-raised events.
    pub scripted: Option<ScriptedStoryEvent>,
}

/// The payload of `Keyword.SendStoryEvent(akLoc, akRef1, akRef2, aiValue1,
/// aiValue2)` (CK *SendStoryEvent - Keyword*) beyond the slots every event
/// shares: the keyword (`K1`), the two integers (`V1` / `V2`), and whether
/// `akRef1` was given — the marker rides `reference_1` either way, so an
/// absent `akRef1` must not read as one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScriptedStoryEvent {
    pub keyword: u32,
    pub values: [i32; 2],
    pub has_reference_1: bool,
}

impl StoryEvent {
    /// The [`EventDataSlots`] view the condition evaluator and the
    /// `FromEvent` alias fill consume.
    pub fn slots(&self) -> EventDataSlots {
        let scripted = self.scripted;
        EventDataSlots {
            reference_1: scripted
                .is_none_or(|data| data.has_reference_1)
                .then_some(self.reference_1),
            reference_2: self.reference_2,
            location_1: self.location_1,
            location_2: self.location_2,
            keyword: scripted.map(|data| data.keyword),
            values: scripted.map_or([0, 0], |data| data.values),
        }
    }
}

impl Component for StoryEvent {
    type Storage = SparseSetStorage<Self>;
}

/// Register the module's component storages. Called from
/// [`crate::register`] — without it every StoryEvent insert is a
/// silent no-op on a fresh world.
pub fn register(world: &mut World) {
    world.register::<StoryEvent>();
    world.init_resource::<PendingScriptStoryEvents>();
    world.init_resource::<PendingScriptKills>();
    world.init_resource::<PendingPlayerXp>();
}

/// Script-raised story events (M47.2, `Keyword.SendStoryEvent`) waiting for
/// [`story_manager_dispatch_system`]. A queue rather than markers: one
/// fragment may raise several in a frame (the College's radiant starter sends
/// one per teacher), and a marker per carrier entity would keep only the
/// last. Drained at the dispatcher's head with the markers; one frame long,
/// so `NOT_SAVED_BY_DESIGN`.
#[derive(Debug, Default)]
pub struct PendingScriptStoryEvents(pub Vec<StoryEvent>);

impl Resource for PendingScriptStoryEvents {}

/// M47.2 — `Effect::Kill` requests, `(victim, killer, silent)`, queued after the
/// quest-state guards drop and drained by the engine's death path the same
/// frame; `NOT_SAVED_BY_DESIGN`. A silent kill (`KillSilent`) raises no KILL
/// story event.
#[derive(Debug, Default)]
pub struct PendingScriptKills(pub Vec<(EntityId, Option<EntityId>, bool)>);

impl Resource for PendingScriptKills {}

/// M47.2 — `Effect::RewardPlayerXp` grants, `(amount, direct)`, queued after
/// the quest-state guards drop and drained by the engine's level roll;
/// `NOT_SAVED_BY_DESIGN`.
#[derive(Debug, Default)]
pub struct PendingPlayerXp(pub Vec<(u32, bool)>);

impl Resource for PendingPlayerXp {}

/// One node of the folded [`SmTree`] arena.
pub struct SmTreeNode {
    pub form_id: u32,
    pub editor_id: String,
    pub kind: SmNodeKind,
    pub event_mnemonic: Option<[u8; 4]>,
    pub conditions: ConditionList,
    /// Authored `SNAM` chain, resolved to arena indices. Sibling order
    /// is the evaluation stack (corpus-verified: #5366 design doc §3).
    /// The NEXT sibling in authored evaluation order — the INVERSE of
    /// `SNAM` (which names the PREVIOUS sibling, #5385): computed by the
    /// tree fold as "the member whose `SNAM` points at me".
    pub next_sibling: Option<usize>,
    /// Chain head among this node's children, computed at build so
    /// dispatch never re-derives ordering.
    pub first_child: Option<usize>,
    /// `SMQN.NNAM` quest pool with `RNAM` reset windows, authored order.
    pub quests: Vec<SmQuestLink>,
    /// `DNAM` node policies (random / do-all-before-repeating /
    /// shares-event), decoded at parse (#5366 Phase 3).
    pub policies: SmNodePolicies,
}

/// The dispatchable Story Manager tree. Built once per load order from
/// [`EsmIndex::story_manager_nodes`] by [`install_story_manager`] /
/// [`build_story_manager_tree`]. `NOT_SAVED_BY_DESIGN`: rederived from
/// the load order every boot, same posture as `NavmeshTile`.
#[derive(Default)]
pub struct SmTree {
    pub nodes: Vec<SmTreeNode>,
    /// Event mnemonic → the `SMEN` root arena index.
    pub roots_by_mnemonic: HashMap<[u8; 4], usize>,
}

impl Resource for SmTree {}

/// Last location key the CLOC producer observed, plus the LCTN it
/// resolved to (the `L1` the next change carries). `NOT_SAVED_BY_DESIGN`:
/// every load — fresh-process or in-process — resets it (#5435,
/// `save_io::reset_story_location_cursor`), so the first post-load frame
/// raises one CLOC with `L1 = None`: arriving in a location by loading is
/// a location change, and the loaded game was never anywhere before it.
#[derive(Default)]
pub struct StoryLocationCursor {
    pub key: Option<u64>,
    pub location: Option<u32>,
}

impl Resource for StoryLocationCursor {}

/// Node-local dispatch state a save must carry (#5366 Phase 3):
/// do-all-before-repeating fired marks and `RNAM` reset timestamps,
/// keyed by node FormID, indexed by pool entry.
///
/// This is the piece that makes a quickload unable to resurrect a
/// radiant the pre-save world already fired: without it, the post-load
/// tree starts with clean cursors and re-fires quests whose reset
/// window or round-robin position the save's world had already spent.
/// Sizes itself to each node's pool at first touch, so a load-order
/// change that reorders a pool simply misaligns (and the fired marks
/// gate re-fires, the safe direction).
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, Default, PartialEq)]
pub struct StoryManagerNodeState {
    /// Per node, per pool entry. Empty until the node first fires.
    pub nodes: HashMap<u32, SmNodeRuntime>,
}

impl Resource for StoryManagerNodeState {}

/// One node's dispatch bookkeeping. Vectors align with the node's quest
/// pool by index.
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SmNodeRuntime {
    /// `true` once the pool entry has been started by this node — the
    /// do-all-before-repeating round-robin mark.
    pub fired: Vec<bool>,
    /// Game-hours timestamp of each pool entry's last start (for the
    /// `RNAM` reset window); `0.0` = never.
    pub last_fire_hours: Vec<f32>,
}

/// Total elapsed game hours, synced each frame by the engine from its
/// canonical clock (`GameTimeRes`). The dispatcher reads it for `RNAM`
/// reset windows. `NOT_SAVED_BY_DESIGN`: derived from the saved game
/// clock every frame.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct StoryClock {
    pub hours: f64,
}

impl Resource for StoryClock {}

/// Selection RNG for random node policies. Seeded fresh per process
/// like the dialogue selector's (`DialogueRandomState` — a plain field
/// behind `resource_mut`, the house shape); `NOT_SAVED_BY_DESIGN` —
/// which radiant a post-load random pick chooses is not state anything
/// reads back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoryManagerRng {
    pub state: u64,
}

impl Default for StoryManagerRng {
    fn default() -> Self {
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0x9E37_79B9_7F4A_7C15);
        Self { state: seed }
    }
}

impl StoryManagerRng {
    /// Next uniform `u64` (SplitMix64). Chosen over a full PRNG crate
    /// dependency for the same reason the dialogue selector rolls its
    /// own: the selection quality a node-pool pick needs is minimal.
    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform `f32` in `[0, 1]`, both ends reachable (24 random bits over
    /// `2^24 - 1`).
    pub fn unit_inclusive(&mut self) -> f32 {
        ((self.next_u64() >> 40) as f32) / ((1u32 << 24) - 1) as f32
    }

    /// Uniform index in `0..len` (widening multiply — unbiased for any
    /// realistic len).
    pub fn pick(&mut self, len: usize) -> usize {
        if len <= 1 {
            return 0;
        }
        (((self.next_u64() >> 32) as u128 * len as u128) >> 32) as usize
    }
}

impl Resource for StoryManagerRng {}

/// One SM-started quest's recorded event data (#5394): the session slots
/// plus the reference slots' portable identity.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub struct StoryEventFill {
    /// The event's slots this session. Not saved — the reference slots
    /// are session `EntityId`s, meaningless to another process.
    #[cfg_attr(feature = "save", serde(skip))]
    pub slots: EventDataSlots,
    /// The R1/R2 slots' global reference FormIds (the actor's
    /// `SceneAliasCandidate::reference_form_id`, or `0x14` for the
    /// player), captured at dispatch: what the alias refresh re-resolves
    /// after a load or a cell reload gave the actor a new id. Location
    /// slots are not carried — no alias consumes them yet (Phase 3+).
    pub reference_forms: [Option<u32>; 2],
}

impl StoryEventFill {
    /// The entity a reference slot names now. The session id wins while it
    /// still carries the recorded identity; otherwise (a fresh-process
    /// load, a despawned or reused id) the recorded FormId is resolved
    /// again. `identity_of` maps an entity to its reference FormId and
    /// `resolve` the reverse — both over the refresh's resident candidates.
    pub fn reference(
        &self,
        slot: byroredux_plugin::esm::records::condition::EventDataSlot,
        identity_of: impl Fn(EntityId) -> Option<u32>,
        resolve: impl Fn(u32) -> Option<EntityId>,
    ) -> Option<EntityId> {
        let form = match slot {
            byroredux_plugin::esm::records::condition::EventDataSlot::Reference1 => self.reference_forms[0],
            byroredux_plugin::esm::records::condition::EventDataSlot::Reference2 => self.reference_forms[1],
            _ => return None,
        };
        match (self.slots.reference(slot), form) {
            (Some(entity), Some(form)) if identity_of(entity) == Some(form) => Some(entity),
            (_, Some(form)) => resolve(form),
            (session, None) => session,
        }
    }
}

/// Event data each SM-started quest's `FromEvent` aliases fill from,
/// keyed by quest — written by the dispatcher's start phase, read by the
/// P4 alias refresh (`refresh_scene_actor_bindings`), which owns the
/// binding table. Latest fire wins (a re-fired radiant re-fills).
/// #5394 — saved: an SM-started quest is restored as running and its
/// starting event is never raised again, so its `FromEvent` aliases can
/// only re-fill from this record. The reference slots persist as
/// FormIds ([`StoryEventFill::reference_forms`]) and resolve back to
/// entities in the refresh; that also re-binds an actor a cell reload
/// gave a new id (#5419's liveness probe still guards the rest).
#[derive(Debug, Default)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub struct StoryEventAliasFill(pub HashMap<QuestFormId, StoryEventFill>);

impl Resource for StoryEventAliasFill {}

/// Fold the parsed node records into an [`SmTree`].
///
/// #5385 — `SNAM` names the PREVIOUS sibling (xEdit, every game), so
/// children of each parent are ordered by taking the member with no
/// previous (`SNAM == 0` or dangling — the TOP of the CK list) as the
/// head and following the inverse edges (the member whose previous is
/// me). Malformed chains (cycles, headless groups — none exist in any
/// vanilla master, but a mod can author anything) fall back to the
/// lowest-index member as head; the walker's visited bitmap keeps
/// traversal terminating regardless.
pub fn build_story_manager_tree(records: &HashMap<u32, SmNodeRecord>) -> SmTree {
    // Sorted by FormID so the arena order is deterministic across
    // HashMap iteration orders.
    let mut sorted: Vec<&SmNodeRecord> = records.values().collect();
    sorted.sort_by_key(|record| record.form_id);

    let mut tree = SmTree::default();
    let mut by_form_id: HashMap<u32, usize> = HashMap::with_capacity(sorted.len());
    for record in &sorted {
        by_form_id.insert(record.form_id, tree.nodes.len());
        tree.nodes.push(SmTreeNode {
            form_id: record.form_id,
            editor_id: record.editor_id.clone(),
            kind: record.kind,
            event_mnemonic: record.event_mnemonic,
            conditions: record.conditions.clone(),
            next_sibling: None,
            first_child: None,
            quests: record.quests.clone(),
            policies: record.policies,
        });
    }

    // Resolve SNAM (xEdit: the PREVIOUS sibling, #5385) to arena
    // indices; dangling (cross-plugin, not-yet-loaded) targets stay
    // `None` — the member reads as "nothing above me". The evaluation-
    // order NEXT pointer is derived per parent group below (the inverse:
    // the member whose previous is me).

    // Event roots: one SMEN per mnemonic; a duplicate (a conflicting
    // override that lost the merge) keeps the first and warns.
    for (index, node) in tree.nodes.iter().enumerate() {
        if let Some(mnemonic) = node.event_mnemonic {
            match tree.roots_by_mnemonic.entry(mnemonic) {
                std::collections::hash_map::Entry::Occupied(_) => log::warn!(
                    "#5366: duplicate Story Manager event node for '{}' \
                     (form {:#010X}) — keeping the first",
                    String::from_utf8_lossy(&mnemonic),
                    node.form_id
                ),
                std::collections::hash_map::Entry::Vacant(slot) => {
                    slot.insert(index);
                }
            }
        }
    }

    // Children by parent, then each group's chain head. The head is the
    // member no in-group sibling points at. Members are collected in
    // arena (FormID-sorted) order — `records.values()` here would make
    // the cycle fallback `members[0]` (and every order-sensitive walk:
    // do-all round-robin, random pick) HashMap-iteration-dependent,
    // which flaked CI (#5366 P2 tests).
    let mut children_by_parent: HashMap<u32, Vec<usize>> = HashMap::new();
    for record in &sorted {
        if record.parent != 0 && by_form_id.contains_key(&record.parent) {
            let index = by_form_id[&record.form_id];
            children_by_parent.entry(record.parent).or_default().push(index);
        }
    }
    // Deterministic parent order: the chain placement below logs, and
    // `HashMap` iteration would make that log order run-dependent.
    let mut parents: Vec<(u32, Vec<usize>)> = children_by_parent.into_iter().collect();
    parents.sort_unstable_by_key(|(parent, _)| *parent);
    for (parent, members) in parents {
        // #5385 — SNAM names the PREVIOUS sibling, so the chain HEAD is
        // the member with no previous (SNAM == 0 or dangling): the TOP
        // of the CK list.
        //
        // #5405 — a group is NOT always one linked list: vanilla Skyrim
        // has groups with several `SNAM == 0` members and several
        // un-targeted ones (`DungeonNode` 19/50 reached). Following one
        // head's chain silently dropped every other segment (58 members,
        // 41 of them quest nodes, never evaluated). Every member is now
        // placed: [`order_sibling_group`].
        // #5643 — index the group once: a `Vec::contains` per member made
        // this O(g²) before the ordering even started.
        let in_group: std::collections::HashSet<usize> = members.iter().copied().collect();
        let previous: HashMap<usize, Option<usize>> = members
            .iter()
            .map(|&member| {
                (
                    member,
                    by_form_id
                        .get(&records[&tree.nodes[member].form_id].previous_sibling)
                        .copied()
                        .filter(|prev| in_group.contains(prev)),
                )
            })
            .collect();
        let order = order_sibling_group(&members, &previous, &tree.nodes);
        let single_chain = {
            let heads = members.iter().filter(|m| previous[m].is_none()).count();
            let mut claimed = std::collections::HashSet::new();
            let unique = members
                .iter()
                .filter_map(|m| previous[m])
                .all(|prev| claimed.insert(prev));
            heads == 1 && unique
        };
        if !single_chain {
            log::warn!(
                "#5405: Story Manager children of {:#010X} do not form a single SNAM chain \
                 ({} members) — placed deterministically, none dropped",
                parent,
                members.len()
            );
        }
        tree.nodes[by_form_id[&parent]].first_child = order.first().copied();
        for pair in order.windows(2) {
            tree.nodes[pair[0]].next_sibling = Some(pair[1]);
        }
    }

    tree
}

/// Evaluation order for one parent's children, placing EVERY member (#5405).
///
/// `previous[m]` is `m`'s in-group `SNAM` predecessor (`None` = the top of
/// the CK list: `SNAM == 0` or a dangling / out-of-group target). xEdit
/// names the field "Previous Node", i.e. the node a member was inserted
/// after, so members are replayed in FormID (creation) order and each is
/// inserted directly after its predecessor — a later insert after the same
/// predecessor lands above an earlier one — while a member with no
/// predecessor goes to the top. A member whose predecessor has not been
/// placed yet waits for it. For a well-formed single chain this reproduces
/// the chain exactly; for the multi-segment groups real masters ship it
/// keeps every segment instead of only the head's. A cycle (no member is
/// placeable) is broken by appending its lowest-FormID member. `members`
/// are arena indices in FormID order, so the result is deterministic.
fn order_sibling_group(
    members: &[usize],
    previous: &HashMap<usize, Option<usize>>,
    nodes: &[SmTreeNode],
) -> Vec<usize> {
    use std::cmp::Reverse;
    use std::collections::BinaryHeap;

    // #5643 — work in FormID-rank space with a ready heap and a linked
    // list. The earlier "scan `waiting` for the first placeable member,
    // `order.contains` per candidate, `order.iter().position` per insert"
    // shape was O(g³) in the group size; a hostile plugin can author a
    // group of any size. Same placement rule, same result: the next member
    // placed is the lowest-FormID one whose predecessor is already placed.
    let mut ranked: Vec<usize> = members.to_vec();
    ranked.sort_unstable_by_key(|&member| nodes[member].form_id);
    let rank: HashMap<usize, usize> = ranked
        .iter()
        .enumerate()
        .map(|(rank, &member)| (member, rank))
        .collect();
    let count = ranked.len();
    let prev_rank: Vec<Option<usize>> = ranked
        .iter()
        .map(|member| previous[member].map(|prev| rank[&prev]))
        .collect();
    // Members waiting on each predecessor, released when it is placed.
    let mut dependents: Vec<Vec<usize>> = vec![Vec::new(); count];
    let mut ready: BinaryHeap<Reverse<usize>> = BinaryHeap::new();
    for (member, prev) in prev_rank.iter().enumerate() {
        match prev {
            None => ready.push(Reverse(member)),
            Some(prev) => dependents[*prev].push(member),
        }
    }

    // The placed order as a singly linked list over ranks.
    let mut next: Vec<Option<usize>> = vec![None; count];
    let mut head: Option<usize> = None;
    let mut tail: Option<usize> = None;
    let mut placed = vec![false; count];
    let mut placed_count = 0;
    // Lowest rank that might still be unplaced (the cycle fallback).
    let mut lowest_unplaced = 0;
    while placed_count < count {
        let member = loop {
            match ready.pop() {
                Some(Reverse(member)) if !placed[member] => break Some(member),
                Some(_) => continue,
                None => break None,
            }
        };
        let member = match member {
            Some(member) => {
                match prev_rank[member] {
                    // No predecessor: goes to the top.
                    None => {
                        next[member] = head;
                        head = Some(member);
                        if tail.is_none() {
                            tail = Some(member);
                        }
                    }
                    // Directly after its predecessor — a later insert after
                    // the same predecessor lands above an earlier one.
                    Some(prev) => {
                        next[member] = next[prev];
                        next[prev] = Some(member);
                        if tail == Some(prev) {
                            tail = Some(member);
                        }
                    }
                }
                member
            }
            // Cycle: no waiting member's predecessor can ever be placed.
            // Break it by appending the lowest-FormID member that is left.
            None => {
                while placed[lowest_unplaced] {
                    lowest_unplaced += 1;
                }
                let member = lowest_unplaced;
                match tail {
                    Some(tail) => next[tail] = Some(member),
                    None => head = Some(member),
                }
                tail = Some(member);
                member
            }
        };
        placed[member] = true;
        placed_count += 1;
        for &dependent in &dependents[member] {
            if !placed[dependent] {
                ready.push(Reverse(dependent));
            }
        }
    }

    let mut order = Vec::with_capacity(count);
    let mut cursor = head;
    while let Some(member) = cursor {
        order.push(ranked[member]);
        cursor = next[member];
    }
    order
}

/// Build and install the [`SmTree`] (and the CLOC cursor) on the world.
/// Returns the node count for the caller's install log. Call from the
/// load-order install pass where `EsmIndex` is in scope — the sibling
/// of `install_start_game_quests`.
pub fn install_story_manager(world: &mut World, records: &HashMap<u32, SmNodeRecord>) -> usize {
    let tree = build_story_manager_tree(records);
    let count = tree.nodes.len();
    world.insert_resource(tree);
    world.init_resource::<StoryLocationCursor>();
    world.init_resource::<StoryEventAliasFill>();
    // Phase 3 — node-policy state (saved), the game-hours clock the
    // engine syncs from GameTimeRes, and the selection RNG.
    world.init_resource::<StoryManagerNodeState>();
    world.init_resource::<StoryClock>();
    world.init_resource::<StoryManagerRng>();
    count
}

/// Engine-side CLOC producer half: fire a `CLOC` StoryEvent on `actor`
/// exactly when the session's location `key` differs from the cursor's.
///
/// `key` is an opaque process-local identity (the engine hashes the
/// LCTN when the current cell resolves one — Skyrim's granularity —
/// else the cell editor-id / worldspace+grid); `None` means "no
/// location context" (loose-NIF mode, mid-transition window) — the
/// cursor follows along but no event fires for it. `location` is the
/// LCTN FormID the key resolved to when one exists; the previous
/// fire's LCTN rides out as the event's `L1` and `location` becomes
/// its `L2`.
pub fn emit_change_location_on_key_change(
    world: &World,
    key: Option<u64>,
    actor: EntityId,
    location: Option<u32>,
) -> bool {
    // The install pass always creates the cursor alongside the tree; a
    // world without one has no Story Manager installed at all.
    let Some(mut cursor) = world.try_resource_mut::<StoryLocationCursor>() else {
        return false;
    };
    if cursor.key == key {
        return false;
    }
    let previous_location = cursor.location;
    cursor.key = key;
    cursor.location = location;
    // Release the resource write before touching the marker storage so
    // this path never nests two acquisitions.
    drop(cursor);
    if key.is_none() {
        return false;
    }
    if let Some(mut events) = world.query_mut::<StoryEvent>() {
        events.insert(
            actor,
            StoryEvent {
                mnemonic: *b"CLOC",
                reference_1: actor,
                reference_2: None,
                location_1: previous_location,
                location_2: location,
                scripted: None,
            },
        );
        return true;
    }
    false
}

/// System: dispatch every raised [`StoryEvent`] through the [`SmTree`],
/// starting quests via the canonical lifecycle. Registered in
/// `Stage::Update` after `quest_startup_system` so an SM-started quest
/// and a Start Game Enabled quest are indistinguishable to everything
/// downstream (alias refresh, SCEN playback, fragment dispatch).
///
/// Two-phase by necessity, not style: the condition evaluator may READ
/// `QuestStageState` (`GetStage` and friends), so holding its WRITE
/// guard across the walk is a same-thread re-entry the lock tracker
/// rejects. Phase A walks the tree read-only and collects quest-start
/// candidates; phase B takes the write guard and starts them,
/// re-checking `is_started` under it.
///
/// The same rule covers [`StoryManagerRng`]: `GetRandomPercent` draws from it
/// inside `evaluate`, so the walk never holds its write guard — only the
/// shuffle and pool-pick sites take it, for the draw alone (#5628).
///
/// Pattern B: the drain at the head is unconditional — no early return
/// sits between the top of the system and it.
pub fn story_manager_dispatch_system(world: &World) {
    let mut events: Vec<StoryEvent> = world
        .query::<StoryEvent>()
        .map(|query| query.iter().map(|(_, event)| *event).collect())
        .unwrap_or_default();
    crate::scene::playback::drain::<StoryEvent>(world);
    if let Some(mut pending) = world.try_resource_mut::<PendingScriptStoryEvents>() {
        events.append(&mut pending.0);
    }
    if events.is_empty() {
        return;
    }
    let hours = world
        .try_resource::<StoryClock>()
        .map_or(0.0, |clock| clock.hours);
    let outcome = {
        let Some(tree) = world.try_resource::<SmTree>() else {
            return;
        };
        let registry = world.try_resource::<QuestDefinitionRegistry>();
        let aliases = world.try_resource::<crate::scene::SceneQuestAliasRegistry>();
        let stages = world
            .try_resource::<QuestStageState>()
            .map(|stages| stages.statuses());
        let node_state = world.try_resource::<StoryManagerNodeState>();
        let mut candidates = Vec::new();
        let mut wraps = Vec::new();
        for event in &events {
            let mut walk = WalkState {
                event,
                slots: event.slots(),
                registry: registry.as_deref(),
                aliases: aliases.as_deref(),
                stages: stages.as_ref(),
                candidates: &mut candidates,
                node_state: node_state.as_deref(),
                hours,
                consumed: false,
                pending_wraps: Vec::new(),
            };
            dispatch_story_event(world, &tree, &mut walk);
            wraps.append(&mut walk.pending_wraps);
        }
        (candidates, wraps)
    };
    // Phase 2's post-start writes (`StoryEventAliasFill` + the
    // `SceneActorBindings` dirty flag) are deferred past the stages
    // guard: the alias refresh holds `SceneActorBindings` while its
    // conditions read `QuestStageState`, so taking bindings under the
    // stages WRITE here closed the ABBA cycle the CI lock-order lane
    // reported (#5366).
    let mut started_event_fills: Vec<(QuestFormId, EventDataSlots)> = Vec::new();
    // #5406 — phase B consults the definitions for the `Run Once` flag
    // (read guard, taken before `stages` to mirror phase A's
    // registry → stages acquisition order).
    let registry = world.try_resource::<QuestDefinitionRegistry>();
    let Some(mut stages) = world.try_resource_mut::<QuestStageState>() else {
        return;
    };
    let mut started = Vec::new();
    for candidate in &outcome.0 {
        if stages.is_running(candidate.quest) {
            continue;
        }
        // #5406 — the CK resets a quest on start unless it is Run Once
        // (Quest Data Tab: "Run Once: Prevents the Quest from being
        // reset when it starts"). Reset first, so completed and failed
        // radiants can rerun and stopped ones restart without the
        // previous run's stage history; eligibility already excluded
        // terminal Run Once quests. No definition → treat as
        // repeatable.
        let run_once = registry
            .as_deref()
            .is_some_and(|reg| reg.is_run_once(candidate.quest));
        if !run_once && stages.is_started(candidate.quest) {
            stages.reset(candidate.quest);
        }
        // #5406 — honor the refusal: a start that did not happen gets
        // no pool mark, no event fill, no "started quest" log line, and
        // wastes no event (the pool simply does not advance).
        if stages
            .start_quest(candidate.quest, candidate.start_up_stage)
            .is_none()
        {
            continue;
        }
        started.push(candidate);
        started_event_fills.push((candidate.quest, candidate.slots));
        log::info!(
            "#5366 story manager: started quest {:#010X} ('{}') via node '{}' \
             on '{}' event",
            candidate.quest.0,
            candidate.editor_id,
            candidate.node_editor_id,
            String::from_utf8_lossy(&candidate.mnemonic),
        );
    }
    drop(stages);
    // Phase 3 — the pool mark (do-all `fired`, `RNAM` `last_fire_hours`)
    // lands only for quests that actually started, with any do-all wrap
    // applied first. Written after the stages guard is released (#6003):
    // phase A holds `StoryManagerNodeState` while its conditions read
    // `QuestStageState`, so taking the node state under the stages WRITE
    // here would record the opposite edge and close a lock-order cycle.
    if !started.is_empty() {
        if let Some(mut node_state) = world.try_resource_mut::<StoryManagerNodeState>() {
            for candidate in started {
                // A wrap clears the fired set only when its node actually
                // started something this dispatch — otherwise the wrap is
                // re-derived on the next fire.
                if outcome.1.contains(&candidate.node) {
                    if let Some(runtime) = node_state.nodes.get_mut(&candidate.node) {
                        for fired in &mut runtime.fired {
                            *fired = false;
                        }
                    }
                }
                let runtime = node_state.nodes.entry(candidate.node).or_default();
                if runtime.fired.len() <= candidate.pool_index {
                    runtime.fired.resize(candidate.pool_index + 1, false);
                    runtime
                        .last_fire_hours
                        .resize(candidate.pool_index + 1, 0.0);
                }
                runtime.fired[candidate.pool_index] = true;
                runtime.last_fire_hours[candidate.pool_index] = hours as f32;
            }
        }
    }
    // Phase 2 — record the event's slots for this quest's `FromEvent`
    // aliases and mark the binding table dirty so the alias refresh
    // (scheduled right after this system) re-fills with them. One
    // resource write at a time: the fill map first, then bindings.
    if !started_event_fills.is_empty() {
        // #5394 — capture each reference slot's portable identity before
        // the fill write (component and player reads, guards scoped).
        let fills_to_write: Vec<(QuestFormId, StoryEventFill)> = started_event_fills
            .iter()
            .map(|(quest, slots)| {
                let form_of = |entity: Option<EntityId>| entity.and_then(|e| reference_form_of(world, e));
                (
                    *quest,
                    StoryEventFill {
                        slots: *slots,
                        reference_forms: [form_of(slots.reference_1), form_of(slots.reference_2)],
                    },
                )
            })
            .collect();
        if let Some(mut fills) = world.try_resource_mut::<StoryEventAliasFill>() {
            for (quest, fill) in fills_to_write {
                fills.0.insert(quest, fill);
            }
        }
        if let Some(mut bindings) = world.try_resource_mut::<SceneActorBindings>() {
            bindings.request_refresh();
        }
    }
}

/// Bethesda's PlayerRef sentinel (never a placed record) — the portable
/// identity a player-valued event slot persists as (#5394).
pub(crate) const PLAYER_REF_FORM_ID: u32 = crate::PLAYER_REFERENCE;

/// #5394 — an event reference's portable identity: `0x14` for the player
/// (PlayerRef), else the placed reference FormId its alias candidate
/// carries. `None` for an entity with neither (it stays session-only).
fn reference_form_of(world: &World, entity: EntityId) -> Option<u32> {
    if crate::papyrus_demo::PapyrusPlayerEntity::get(world).is_some_and(|player| player == entity)
    {
        return Some(PLAYER_REF_FORM_ID);
    }
    world
        .get::<crate::scene::SceneAliasCandidate>(entity)
        .map(|candidate| candidate.reference_form_id)
        .filter(|form| *form != 0)
}

/// A quest the walk wants started, gathered during the read-only phase.
struct QuestStartCandidate {
    quest: QuestFormId,
    start_up_stage: Option<u16>,
    editor_id: String,
    node_editor_id: String,
    mnemonic: [u8; 4],
    /// The raising event's slots, carried for the `FromEvent` alias fill.
    slots: EventDataSlots,
    /// Owning node FormID + pool index, for the Phase-3 state marks
    /// (do-all `fired`, `RNAM` `last_fire_hours`).
    node: u32,
    pool_index: usize,
}

/// Bundles the per-event walk state so the recursive step stays under
/// the arity threshold and every field's read/write role is visible at
/// the call site.
struct WalkState<'a> {
    event: &'a StoryEvent,
    slots: EventDataSlots,
    registry: Option<&'a QuestDefinitionRegistry>,
    /// M47.2 — the quests' aliases, for the required-`FromEvent`-fill gate.
    aliases: Option<&'a crate::scene::SceneQuestAliasRegistry>,
    /// The started quests' statuses, snapshotted before the walk: its
    /// condition evaluation reads `QuestStageState` itself, and a second
    /// read guard on one thread can deadlock behind a queued writer (#2386).
    stages: Option<&'a HashMap<QuestFormId, QuestStatus>>,
    candidates: &'a mut Vec<QuestStartCandidate>,
    /// Phase 3 — node-policy state (do-all marks, reset timestamps),
    /// read during the walk; writes go through `pending_marks` and are
    /// applied by phase B only for quests it actually starts.
    node_state: Option<&'a StoryManagerNodeState>,
    /// Total game hours for `RNAM` reset windows (`StoryClock`).
    hours: f64,
    /// `true` once a non-sharing quest node that fired a quest consumed
    /// the event (#5404) — every remaining sibling walk stops.
    consumed: bool,
    /// Nodes whose do-all fired set wraps this event (every eligible
    /// entry had run); phase B clears them when applying the new mark.
    pending_wraps: Vec<u32>,
}

fn dispatch_story_event(
    world: &World,
    tree: &SmTree,
    state: &mut WalkState<'_>,
) {
    let Some(&root) = tree.roots_by_mnemonic.get(&state.event.mnemonic) else {
        return;
    };
    // One visited bitmap for the whole event walk: a node is evaluated
    // at most once per event, which bounds malformed sibling cycles and
    // makes re-reachable nodes cheap no-ops.
    let mut visited = vec![false; tree.nodes.len()];
    walk_siblings(
        world,
        tree,
        state,
        tree.nodes[root].first_child,
        // Event roots author no random policy (census: SMEN DNAM = 0),
        // so the top-level chain is stacked by construction.
        false,
        &mut visited,
    );
}

/// Evaluate a sibling chain. Phase-3 semantics (#5366):
///
/// - **Order** — stacked (the default) evaluates the authored `SNAM`
///   order; a random parent (`DNAM & 0x1` on the node whose children
///   this chain is) evaluates the chain in a seeded random permutation
///   ("a Random node will process all its child nodes randomly", SM
///   Event Node — local CK wiki).
/// - **Descend** — a node whose conditions pass is processed (quest
///   pool picked, children walked); one that fails skips its subtree.
/// - **Consume** — a quest node without `Shares Event` (`DNAM & 0x20000`)
///   that FIRES A QUEST consumes the event once it finishes: every
///   remaining sibling walk stops. #5404 — Bethesda's own tutorial
///   (*Bethesda Tutorial Story Manager*, local CK wiki) states the
///   trigger: "without this box, if a node actually manages to fire a
///   quest, the event will not go through the rest of the tree". The
///   earlier reading keyed on the node merely being processed
///   (from the *SM Event Node* page's "as soon as it finishes with that
///   node"), which made an always-passing node whose pool is exhausted
///   (`CWChangeLocationScenes` once its four scenes run) swallow every
///   CLOC. "Fires" here is the node's own pool queueing a start
///   candidate in the read-only phase (eligible = not running, past its
///   `RNAM` window); phase B re-checks `is_started` under the write
///   guard, so a same-event duplicate pick of one quest is the only way
///   a queued candidate fails to start.
fn walk_siblings(
    world: &World,
    tree: &SmTree,
    state: &mut WalkState<'_>,
    head: Option<usize>,
    parent_random: bool,
    visited: &mut [bool],
) {
    // #5643 — an explicit stack, not recursion: the node tree's depth is
    // bounded only by how many nodes a plugin authors, and every level of
    // the old recursion was a native stack frame. Vanilla tops out at 6–7
    // levels; `MAX_WALK_DEPTH` is far above that and far below anything that
    // threatens the thread's stack. Visit order, RNG draw order (a group's
    // chain is built — and shuffled — on entering it) and the consume /
    // choose-one early exits are exactly the recursive walk's.
    struct Frame {
        chain: Vec<usize>,
        next: usize,
        parent_random: bool,
        /// The node whose subtree was just walked, awaiting its post-subtree
        /// consume / choose-one checks: `(index, queued_before, node_fired)`.
        after_subtree: Option<(usize, usize, bool)>,
    }

    /// Build a group's chain from its head (marking it visited) and apply the
    /// random-parent shuffle.
    fn enter_group(
        world: &World,
        tree: &SmTree,
        head: usize,
        parent_random: bool,
        visited: &mut [bool],
    ) -> Frame {
        let mut chain = Vec::new();
        let mut current = Some(head);
        while let Some(index) = current {
            if visited[index] {
                break;
            }
            visited[index] = true;
            chain.push(index);
            current = tree.nodes[index].next_sibling;
        }
        // Random parent policy: same chain, seeded shuffle. A world without
        // the RNG resource keeps authored order (conservative stacked).
        if parent_random {
            // The write guard lives for the shuffle alone: `evaluate` (run
            // for each node below) takes `StoryManagerRng` too, through
            // `GetRandomPercent`, so holding it across the walk is a
            // same-thread re-entry the lock tracker panics on (#5628).
            if let Some(mut rng) = world.try_resource_mut::<StoryManagerRng>() {
                for i in (1..chain.len()).rev() {
                    let j = rng.pick(i + 1);
                    chain.swap(i, j);
                }
            }
        }
        Frame {
            chain,
            next: 0,
            parent_random,
            after_subtree: None,
        }
    }

    let Some(head) = head else { return };
    let mut stack = vec![enter_group(world, tree, head, parent_random, visited)];
    let mut depth_capped = false;
    while let Some(frame) = stack.last_mut() {
        if let Some((index, queued_before, node_fired)) = frame.after_subtree.take() {
            let node = &tree.nodes[index];
            // A non-sharing quest node that actually fires a quest
            // consumes the event — after its own subtree finishes. A
            // processed node whose pool is exhausted (every entry
            // already running, or inside its reset window) fires
            // nothing and lets the event through (#5404).
            if node.kind == SmNodeKind::Quest && !node.policies.shares_event && node_fired {
                state.consumed = true;
                // Consumed: every enclosing group stops too.
                return;
            }
            // Random parent, choose-one: the CK tutorial's "it will
            // choose one of its child nodes randomly" — the chain stops
            // at the first child that queued a start, so a random
            // branch fires one quest per event even when every child
            // shares.
            if frame.parent_random
                && node.kind == SmNodeKind::Quest
                && state.candidates.len() > queued_before
            {
                stack.pop();
                continue;
            }
        }
        if state.consumed {
            return;
        }
        let Some(&index) = frame.chain.get(frame.next) else {
            stack.pop();
            continue;
        };
        frame.next += 1;
        let node = &tree.nodes[index];
        // Subject = the doer: R2 when the event carries one (KILL
        // killer), else R1 (CLOC actor). Target = R1 (KILL victim).
        // `RunOn::EventData` tags resolve through the same slots.
        let mut context = ConditionContext::for_subject(
            state.event.reference_2.unwrap_or(state.event.reference_1),
        )
        .with_event_data(&state.slots);
        context.target = Some(state.event.reference_1);
        // #5380 — a node gated on a condition the evaluator cannot
        // model declines: the M47.1 Unknown→0.0 default was designed
        // for display and selection, and any comparison 0.0 satisfies
        // (`== 0`, `< 1`, `!= 1`) passed vacuously — 40 quest nodes
        // under the live-producer events started quests on unmodeled
        // terms, including both of sm1's original gated boot starts
        // (`WIGreetingNodeSHARES` fn 145 `== 0`). Declining is also
        // the honest answer for the fail-closed half: a node whose
        // gate cannot run neither starts nor consumes.
        let modeled = node.conditions.iter().all(|condition| crate::condition::is_modeled(world, condition));
        if modeled && evaluate(&node.conditions, world, &context) {
            let queued_before = state.candidates.len();
            if node.kind == SmNodeKind::Quest {
                process_quest_node(world, tree, state, index);
            }
            // #5404 — did THIS node's own pool put a quest up for start?
            // Taken before the subtree walk so a child's candidate does
            // not count for the parent.
            let node_fired = state.candidates.len() > queued_before;
            frame.after_subtree = Some((index, queued_before, node_fired));
            if let Some(child) = node.first_child {
                if stack.len() >= MAX_WALK_DEPTH {
                    if !depth_capped {
                        depth_capped = true;
                        log::warn!(
                            "#5643: Story Manager walk reached {MAX_WALK_DEPTH} nested \
                             groups at node {:#010X} — deeper subtrees are skipped",
                            node.form_id
                        );
                    }
                } else {
                    stack.push(enter_group(
                        world,
                        tree,
                        child,
                        node.policies.random,
                        visited,
                    ));
                }
            }
        }
    }
}

/// Deepest nest of sibling groups the Story Manager walk will descend into
/// (#5643). Vanilla's deepest tree is 6–7; a deeper authored chain is a
/// malformed or hostile plugin, and its tail is skipped rather than walked
/// onto the stack.
const MAX_WALK_DEPTH: usize = 64;

/// Pick and queue one quest from a passing quest node's pool, honoring
/// the Phase-3 policies:
///
/// - Eligibility: not currently started, and past its `RNAM` reset
///   window (`reset_hours > 0` requires `now - last_fire >= window`).
/// - Do-all-before-repeating (`DNAM & 0x10000`): prefer unfired pool
///   entries; when every eligible entry has fired, the round-robin
///   wraps — the selection treats all as fresh and the fired set is
///   cleared when phase B applies the new mark.
/// - Random (`DNAM & 0x1`): uniform pick among the preferred entries;
///   stacked: first in authored order.
fn process_quest_node(world: &World, tree: &SmTree, state: &mut WalkState<'_>, index: usize) {
    let node = &tree.nodes[index];
    let runtime = state
        .node_state
        .and_then(|states| states.nodes.get(&node.form_id));
    let fired = runtime.map(|rt| rt.fired.as_slice()).unwrap_or(&[]);
    let last_fire = runtime
        .map(|rt| rt.last_fire_hours.as_slice())
        .unwrap_or(&[]);

    let eligible: Vec<usize> = (0..node.quests.len())
        .filter(|&i| {
            let quest = QuestFormId(node.quests[i].form_id);
            // Running quests are ineligible; stopped ones may re-fire
            // (the radiant rerun path — `is_started` would block a
            // stopped quest's restart forever).
            let status = state.stages.and_then(|stages| stages.get(&quest)).copied();
            if status == Some(QuestStatus::Running) {
                return false;
            }
            // #5406 — a terminal quest is only eligible for the rerun
            // path phase B opens (reset-on-start for non-Run-Once
            // quests); a Run Once quest that completed or failed stays
            // done forever and must stop jamming stacked pools and
            // winning random picks. Without definitions the quest is
            // treated as repeatable and phase B decides.
            if matches!(status, Some(QuestStatus::Completed | QuestStatus::Failed)) && state
                .registry
                .is_some_and(|registry| registry.is_run_once(quest))
            {
                return false;
            }
            hours_gate_open(
                node.quests[i].reset_hours,
                last_fire.get(i).copied(),
                state.hours,
            ) && event_aliases_fill(state, quest)
        })
        .collect();
    if eligible.is_empty() {
        return;
    }

    let preferred: Vec<usize> = if node.policies.do_all_before_repeating {
        let unfired: Vec<usize> = eligible
            .iter()
            .copied()
            .filter(|&i| !fired.get(i).copied().unwrap_or(false))
            .collect();
        if unfired.is_empty() {
            // Every eligible entry has run — the authored wrap point.
            state.pending_wraps.push(node.form_id);
            eligible
        } else {
            unfired
        }
    } else {
        eligible
    };

    let pick = if node.policies.random {
        // Scoped to the draw, for the same reason as the shuffle in
        // `walk_siblings` (#5628).
        match world.try_resource_mut::<StoryManagerRng>() {
            Some(mut rng) => preferred[rng.pick(preferred.len())],
            None => preferred[0],
        }
    } else {
        preferred[0]
    };

    let quest = QuestFormId(node.quests[pick].form_id);
    state.candidates.push(QuestStartCandidate {
        quest,
        start_up_stage: state
            .registry
            .and_then(|registry| registry.start_up_stage(quest)),
        editor_id: state
            .registry
            .and_then(|registry| registry.editor_id(quest))
            .unwrap_or("?")
            .to_owned(),
        node_editor_id: node.editor_id.clone(),
        mnemonic: state.event.mnemonic,
        slots: state.slots,
        node: node.form_id,
        pool_index: pick,
    });
}

/// M47.2 — the CK *Quest Alias Tab*: an alias not flagged Optional that
/// fails to fill "will cause the quest to fail to start". The decidable case
/// at selection time is a `FromEvent` alias whose event slot is empty — a
/// marriage quest filling its love interest from `R1` cannot start from a
/// script event that carries no `R1`, and the Story Manager walks on to the
/// next node. Slots this runtime does not model never block.
fn event_aliases_fill(state: &WalkState<'_>, quest: QuestFormId) -> bool {
    use byroredux_plugin::esm::records::condition::{event_data_slot, EventDataSlot};
    use byroredux_plugin::esm::records::{AliasFillType, ALIAS_FLAG_OPTIONAL};
    let Some(aliases) = state.aliases.and_then(|registry| registry.aliases_of(quest)) else {
        return true;
    };
    aliases.iter().all(|alias| {
        let Some(AliasFillType::FromEvent { data, .. }) = alias.fill_type else {
            return true;
        };
        if alias.flags.has(ALIAS_FLAG_OPTIONAL) || alias.is_collection {
            return true;
        }
        match event_data_slot(data as u32) {
            Some(EventDataSlot::Reference1) => state.slots.reference_1.is_some(),
            Some(EventDataSlot::Reference2) => state.slots.reference_2.is_some(),
            Some(EventDataSlot::Location1) => state.slots.location_1.is_some(),
            Some(EventDataSlot::Location2) => state.slots.location_2.is_some(),
            None => true,
        }
    })
}

/// `RNAM` reset gate: open when no window is authored, the entry has
/// never fired, or `now` is past `window` hours since the last fire.
fn hours_gate_open(window: f32, last_fire: Option<f32>, now: f64) -> bool {
    if window <= 0.0 {
        return true;
    }
    match last_fire {
        None | Some(0.0) => true,
        Some(last) => now - f64::from(last) >= f64::from(window),
    }
}

#[cfg(test)]
mod tests {
    /// #6003 — phase A holds `StoryManagerNodeState` while its conditions read
    /// `QuestStageState`, so phase B must not take the node state while it holds
    /// the stages write guard (the opposite edge closed a lock-order cycle that
    /// no unit run can observe — the CI lane never installs the node state).
    /// Pin the shape: nothing between the stages acquisition and its drop in
    /// the dispatch system names the node-state resource.
    #[test]
    fn phase_b_never_takes_the_node_state_under_the_stages_guard() {
        let production = byroredux_core::source_scan::production_text(include_str!("story_manager.rs"));
        let system = &production[production
            .find("pub fn story_manager_dispatch_system")
            .expect("dispatch system present")..];
        let held_from = system
            .find("try_resource_mut::<QuestStageState>()")
            .expect("phase B takes the stages write guard");
        let held_to = held_from
            + system[held_from..]
                .find("drop(stages);")
                .expect("phase B drops the stages guard");
        let held = &system[held_from..held_to];
        assert!(
            !held.contains("StoryManagerNodeState"),
            "the node state is taken while the stages write guard is held"
        );
    }

    /// #5394 — the portable identity a recorded event slot persists as:
    /// PlayerRef for the player, the candidate's reference FormId for a
    /// placed actor, nothing for an entity with neither.
    #[test]
    fn reference_form_of_names_the_player_and_placed_references() {
        let mut world = World::new();
        crate::register(&mut world);
        let player = world.spawn();
        world.insert_resource(crate::papyrus_demo::PapyrusPlayerEntity(player));
        let placed = world.spawn();
        world.insert(
            placed,
            crate::scene::SceneAliasCandidate {
                reference_form_id: 0x0001_2345,
                base_form_id: 0x0001_2346,
                linked_refs: Vec::new(),
                location_ref_types: Vec::new(),
            },
        );
        let bare = world.spawn();
        assert_eq!(super::reference_form_of(&world, player), Some(super::PLAYER_REF_FORM_ID));
        assert_eq!(super::reference_form_of(&world, placed), Some(0x0001_2345));
        assert_eq!(super::reference_form_of(&world, bare), None);
    }

    use super::*;
    use byroredux_plugin::esm::records::condition::RunOn;

    /// Components must be registered on a fresh World before storages
    /// resolve (`query_mut` on an unregistered component returns `None`,
    /// which would make every insert below a silent no-op).
    fn setup_world() -> World {
        let mut world = World::new();
        crate::register(&mut world);
        world
    }

    fn branch(form_id: u32, parent: u32, previous_sibling: u32) -> SmNodeRecord {
        SmNodeRecord {
            form_id,
            parent,
            previous_sibling,
            kind: SmNodeKind::Branch,
            ..Default::default()
        }
    }

    fn event_node(form_id: u32, mnemonic: &[u8; 4]) -> SmNodeRecord {
        SmNodeRecord {
            form_id,
            parent: 0,
            kind: SmNodeKind::Event,
            event_mnemonic: Some(*mnemonic),
            ..Default::default()
        }
    }

    fn quest_node(form_id: u32, parent: u32, previous_sibling: u32, quests: &[u32]) -> SmNodeRecord {
        quest_node_full(form_id, parent, previous_sibling, quests, SmNodePolicies::default())
    }

    fn quest_node_full(
        form_id: u32,
        parent: u32,
        previous_sibling: u32,
        quests: &[u32],
        policies: SmNodePolicies,
    ) -> SmNodeRecord {
        SmNodeRecord {
            form_id,
            parent,
            previous_sibling,
            kind: SmNodeKind::Quest,
            quests: quests
                .iter()
                .map(|&form_id| SmQuestLink {
                    form_id,
                    reset_hours: 0.0,
                    ..Default::default()
                })
                .collect(),
            policies,
            ..Default::default()
        }
    }

    fn records(nodes: Vec<SmNodeRecord>) -> HashMap<u32, SmNodeRecord> {
        nodes.into_iter().map(|node| (node.form_id, node)).collect()
    }

    /// The pre-#5643 placement loop, kept verbatim as the oracle for the
    /// linear-time rewrite.
    fn reference_order_sibling_group(
        members: &[usize],
        previous: &HashMap<usize, Option<usize>>,
        nodes: &[SmTreeNode],
    ) -> Vec<usize> {
        let mut order: Vec<usize> = Vec::with_capacity(members.len());
        let mut waiting: Vec<usize> = members.to_vec();
        waiting.sort_unstable_by_key(|&member| nodes[member].form_id);
        while !waiting.is_empty() {
            let placeable = waiting
                .iter()
                .position(|member| previous[member].is_none_or(|prev| order.contains(&prev)));
            match placeable {
                Some(at) => {
                    let member = waiting.remove(at);
                    let slot = match previous[&member] {
                        None => 0,
                        Some(prev) => order
                            .iter()
                            .position(|&placed| placed == prev)
                            .map_or(0, |i| i + 1),
                    };
                    order.insert(slot, member);
                }
                None => order.push(waiting.remove(0)),
            }
        }
        order
    }

    fn bare_node(form_id: u32) -> SmTreeNode {
        SmTreeNode {
            form_id,
            editor_id: String::new(),
            kind: SmNodeKind::Branch,
            event_mnemonic: None,
            conditions: Default::default(),
            next_sibling: None,
            first_child: None,
            quests: Vec::new(),
            policies: Default::default(),
        }
    }

    /// #5643 — the O(g) rewrite places every group exactly where the
    /// O(g³) loop did: single chains, multi-segment groups, self-references
    /// and cycles, over a few thousand seeded random predecessor maps.
    #[test]
    fn sibling_group_order_matches_the_reference_placement() {
        let mut seed = 0x9E37_79B9_7F4A_7C15u64;
        let mut next = move || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed
        };
        for round in 0..3000 {
            let size = 1 + (next() % 24) as usize;
            // Non-contiguous, shuffled-by-id arena indices.
            let nodes: Vec<SmTreeNode> = (0..size + 5).map(|i| bare_node(i as u32 * 3 + 1)).collect();
            let members: Vec<usize> = (0..size).map(|i| i + 2).collect();
            let previous: HashMap<usize, Option<usize>> = members
                .iter()
                .map(|&member| {
                    let prev = match next() % 4 {
                        0 => None,
                        _ => Some(members[(next() as usize) % members.len()]),
                    };
                    (member, prev)
                })
                .collect();
            assert_eq!(
                order_sibling_group(&members, &previous, &nodes),
                reference_order_sibling_group(&members, &previous, &nodes),
                "round {round}: {previous:?}"
            );
        }
    }

    /// #5643 — a plugin-authored group of thousands of siblings builds in
    /// one linear pass; the cubic loop needed ~10¹² steps for this one.
    #[test]
    fn a_huge_sibling_group_builds_and_keeps_every_member() {
        const GROUP: u32 = 20_000;
        let mut nodes = vec![event_node(1, b"KILL")];
        // Child k follows child k+1: the chain runs against FormID order,
        // the placement loop's worst case.
        for k in 0..GROUP {
            let form_id = 100 + k;
            let previous = if k + 1 < GROUP { form_id + 1 } else { 0 };
            nodes.push(branch(form_id, 1, previous));
        }
        let tree = build_story_manager_tree(&records(nodes));
        let root = tree.roots_by_mnemonic[b"KILL"];
        let mut cursor = tree.nodes[root].first_child;
        let mut walked = 0u32;
        let mut expected = 100 + GROUP - 1;
        while let Some(index) = cursor {
            assert_eq!(tree.nodes[index].form_id, expected);
            expected = expected.wrapping_sub(1);
            walked += 1;
            cursor = tree.nodes[index].next_sibling;
        }
        assert_eq!(walked, GROUP);
    }

    /// #5643 — a plugin-authored chain of nested nodes tens of thousands
    /// deep must not recurse the native stack away. The shallow quest still
    /// starts; the walk stops descending at `MAX_WALK_DEPTH`, so a quest
    /// authored beyond it never fires.
    #[test]
    fn a_hugely_nested_tree_walks_without_overflowing_the_stack() {
        const DEPTH: u32 = 50_000;
        let mut nodes = vec![event_node(1, b"KILL")];
        nodes.push(quest_node_full(
            2,
            1,
            0,
            &[0x111],
            SmNodePolicies {
                shares_event: true,
                ..Default::default()
            },
        ));
        // A branch spine hanging off the root: 100 → 101 → … each the only
        // child of the one before, a shallow quest at the top of the spine
        // and a deep quest at its very bottom.
        nodes.push(branch(100, 1, 2));
        for k in 1..DEPTH {
            nodes.push(branch(100 + k, 99 + k, 0));
        }
        nodes.push(quest_node(100 + DEPTH, 99 + DEPTH, 0, &[0xDEE9]));
        let tree = build_story_manager_tree(&records(nodes));

        let mut world = setup_world();
        world.insert_resource(tree);
        world.insert_resource(QuestStageState::default());
        let actor = world.spawn();
        if let Some(mut events) = world.query_mut::<StoryEvent>() {
            events.insert(
                actor,
                StoryEvent {
                    mnemonic: *b"KILL",
                    reference_1: actor,
                    reference_2: None,
                    location_1: None,
                    location_2: None,
                    scripted: None,
                },
            );
        }
        story_manager_dispatch_system(&world);
        let stages = world.try_resource::<QuestStageState>().unwrap();
        assert!(stages.is_started(QuestFormId(0x111)), "the shallow quest fires");
        assert!(
            !stages.is_started(QuestFormId(0xDEE9)),
            "a quest nested past MAX_WALK_DEPTH is skipped, not walked"
        );
    }

    /// Arena order is FormID-sorted and children follow the authored
    /// SNAM chain from the chain head, not insertion order.
    #[test]
    fn children_ordered_by_sibling_chain_from_head() {
        // Event KILL (10) with children 1 → 2 → 3 by SNAM, contributed
        // in shuffled map order, plus a mid-chain parent (2) with its
        // own child (4).
        // #5385 — SNAM is the PREVIOUS sibling: the chain 1 → 2 → 3
        // authors as 1.prev=0 (head), 2.prev=1, 3.prev=2, contributed in
        // shuffled map order.
        let tree = build_story_manager_tree(&records(vec![
            quest_node(3, 10, 2, &[0xAAA]),
            event_node(10, b"KILL"),
            quest_node(1, 10, 0, &[0x111]),
            quest_node(2, 10, 1, &[0x222]),
            quest_node(4, 2, 0, &[0x444]),
        ]));
        let root = tree.roots_by_mnemonic[b"KILL"];
        // Chain: 1 → 2 → 3, resolved by arena (FormID) index.
        let first = tree.nodes[root].first_child.unwrap();
        assert_eq!(tree.nodes[first].form_id, 1);
        let second = tree.nodes[first].next_sibling.unwrap();
        assert_eq!(tree.nodes[second].form_id, 2);
        let third = tree.nodes[second].next_sibling.unwrap();
        assert_eq!(tree.nodes[third].form_id, 3);
        assert!(tree.nodes[third].next_sibling.is_none());
        // Node 2's child chain head is 4.
        assert_eq!(tree.nodes[second].first_child.map(|i| tree.nodes[i].form_id), Some(4));
    }

    /// #5385 — the direction pin, mirroring Bethesda's own tutorial
    /// data: Skyrim's KILL subtree authors `DA08KillFriendNode`
    /// (0x10FAEF) as the CK tree's FIRST child (the tutorial says
    /// "expand the node labeled DA08KillFriendNode", and its SNAM is
    /// 0), with the remaining nine children below it. Under the old
    /// "next" reading the head was the BOTTOM of the list. A chain
    /// authored head-first (each SNAM naming the member ABOVE it, the
    /// DLC-append shape) must evaluate head-first.
    #[test]
    fn snam_previous_semantics_walk_head_first() {
        // KILL with three children authored top-down: 1 (SNAM 0),
        // 2 (SNAM 1), 3 (SNAM 2) — the CK list order.
        let tree = build_story_manager_tree(&records(vec![
            event_node(10, b"KILL"),
            quest_node(1, 10, 0, &[0x111]),
            quest_node(2, 10, 1, &[0x222]),
            quest_node(3, 10, 2, &[0x333]),
        ]));
        let root = tree.roots_by_mnemonic[b"KILL"];
        let head = tree.nodes[root].first_child.unwrap();
        assert_eq!(tree.nodes[head].form_id, 1, "the SNAM==0 member is the head (the top)");
        let second = tree.nodes[head].next_sibling.unwrap();
        let third = tree.nodes[second].next_sibling.unwrap();
        assert_eq!((tree.nodes[second].form_id, tree.nodes[third].form_id), (2, 3));
        assert!(tree.nodes[third].next_sibling.is_none());
    }

    /// #5405 — a group that is NOT a single SNAM chain keeps every member.
    /// Two `SNAM == 0` heads (1, 2), a follower of each (3 after 1, 4 after
    /// 2) and an un-targeted, dangling-predecessor member (5): the old
    /// builder walked one head's chain only and dropped the rest. Replayed
    /// in FormID order (insert after the predecessor, no-predecessor on
    /// top): 1 → [1]; 2 → [2,1]; 3 after 1 → [2,1,3]; 4 after 2 →
    /// [2,4,1,3]; 5 → [5,2,4,1,3].
    #[test]
    fn multi_segment_groups_place_every_member() {
        let tree = build_story_manager_tree(&records(vec![
            event_node(10, b"KILL"),
            quest_node(1, 10, 0, &[0x111]),
            quest_node(2, 10, 0, &[0x222]),
            quest_node(3, 10, 1, &[0x333]),
            quest_node(4, 10, 2, &[0x444]),
            quest_node(5, 10, 0x9999, &[0x555]), // dangling predecessor
        ]));
        let root = tree.roots_by_mnemonic[b"KILL"];
        let mut order = Vec::new();
        let mut at = tree.nodes[root].first_child;
        while let Some(index) = at {
            order.push(tree.nodes[index].form_id);
            at = tree.nodes[index].next_sibling;
        }
        assert_eq!(order, vec![5, 2, 4, 1, 3], "every member placed, deterministically");
    }

    /// #5405 — a well-formed chain authored out of FormID order still
    /// reproduces the chain exactly (the replay defers a member until its
    /// predecessor is placed).
    #[test]
    fn single_chain_survives_reverse_formid_order() {
        // 9 → 7 → 5 (top to bottom): 9.prev=0, 7.prev=9, 5.prev=7.
        let tree = build_story_manager_tree(&records(vec![
            event_node(10, b"KILL"),
            quest_node(5, 10, 7, &[0x1]),
            quest_node(7, 10, 9, &[0x2]),
            quest_node(9, 10, 0, &[0x3]),
        ]));
        let root = tree.roots_by_mnemonic[b"KILL"];
        let mut order = Vec::new();
        let mut at = tree.nodes[root].first_child;
        while let Some(index) = at {
            order.push(tree.nodes[index].form_id);
            at = tree.nodes[index].next_sibling;
        }
        assert_eq!(order, vec![9, 7, 5]);
    }

    /// #5405 real-data floor: on `Skyrim.esm` every SM node whose parent
    /// resolves is reachable from its event root by the dispatch walk's
    /// `first_child` / `next_sibling` links. The pre-fix builder reached
    /// 19/50 of `DungeonNode`'s children and dropped 58 members overall
    /// (41 quest nodes).
    #[test]
    #[ignore = "needs Skyrim SE game data on disk"]
    fn every_skyrim_story_manager_node_is_reachable_from_its_event_root() {
        let data = byroredux_plugin::esm::test_paths::skyrim_se_data_dir();
        let esm = data.join("Skyrim.esm");
        if !esm.is_file() {
            eprintln!("[SM reachability] skipping: no Skyrim.esm at {esm:?}");
            return;
        }
        let bytes = std::fs::read(&esm).expect("read Skyrim.esm");
        let index = byroredux_plugin::esm::parse_esm(&bytes).expect("parse Skyrim.esm");
        let tree = build_story_manager_tree(&index.story_manager_nodes);

        let mut reached = vec![false; tree.nodes.len()];
        let mut stack: Vec<usize> = tree.roots_by_mnemonic.values().copied().collect();
        while let Some(node) = stack.pop() {
            if std::mem::replace(&mut reached[node], true) {
                continue;
            }
            let mut child = tree.nodes[node].first_child;
            while let Some(c) = child {
                if reached[c] {
                    break;
                }
                stack.push(c);
                child = tree.nodes[c].next_sibling;
            }
        }
        let known: std::collections::HashSet<u32> =
            tree.nodes.iter().map(|node| node.form_id).collect();
        let unreached: Vec<String> = tree
            .nodes
            .iter()
            .enumerate()
            .filter(|(i, node)| {
                !reached[*i]
                    && index
                        .story_manager_nodes
                        .get(&node.form_id)
                        .is_some_and(|r| r.parent != 0 && known.contains(&r.parent))
                    && node.event_mnemonic.is_none()
            })
            .map(|(_, node)| format!("{:08X} {}", node.form_id, node.editor_id))
            .collect();
        assert!(
            unreached.is_empty(),
            "{} Story Manager nodes unreachable from their event root: {unreached:#?}",
            unreached.len()
        );
    }

    /// A sibling cycle (2 ⇄ 3, no head) still builds: the lowest-index
    /// member becomes head and the visited bitmap keeps dispatch
    /// terminating.
    #[test]
    fn sibling_cycle_builds_and_dispatch_terminates() {
        let tree = build_story_manager_tree(&records(vec![
            event_node(10, b"KILL"),
            quest_node(2, 10, 3, &[0x222]),
            quest_node(3, 10, 2, &[0x333]),
        ]));
        let root = tree.roots_by_mnemonic[b"KILL"];
        assert!(tree.nodes[root].first_child.is_some());

        let mut world = setup_world();
        world.insert_resource(tree);
        world.insert_resource(QuestStageState::default());
        let actor = world.spawn();
        if let Some(mut events) = world.query_mut::<StoryEvent>() {
            events.insert(
                actor,
                StoryEvent {
                    mnemonic: *b"KILL",
                    reference_1: actor,
                    reference_2: None,
                    location_1: None,
                    location_2: None,
                    scripted: None,
                },
            );
        }
        story_manager_dispatch_system(&world);
        let stages = world.try_resource::<QuestStageState>().unwrap();
        // The chain-head member fired; non-sharing, it consumed the
        // event before the cycle's second member evaluated.
        assert!(stages.is_started(QuestFormId(0x222)));
        assert!(!stages.is_started(QuestFormId(0x333)));
    }

    /// End-to-end: a KILL event walks the tree and starts the linked
    /// quest through the canonical lifecycle; a second identical event
    /// does not restart it; an unknown mnemonic is a no-op.
    #[test]
    fn dispatch_starts_quests_once_and_ignores_unknown_mnemonics() {
        let tree = build_story_manager_tree(&records(vec![
            event_node(10, b"KILL"),
            branch(20, 10, 0),
            quest_node(30, 20, 0, &[0x000F_0A10]),
            quest_node(31, 20, 30, &[0x000F_0A11]),
        ]));
        let mut world = setup_world();
        world.insert_resource(tree);
        world.insert_resource(QuestStageState::default());
        let killer = world.spawn();
        let victim = world.spawn();
        let fire = |world: &World| {
            if let Some(mut events) = world.query_mut::<StoryEvent>() {
                events.insert(
                    victim,
                    StoryEvent {
                        mnemonic: *b"KILL",
                        reference_1: victim,
                        reference_2: Some(killer),
                        location_1: None,
                        location_2: None,
                        scripted: None,
                    },
                );
            }
            story_manager_dispatch_system(world);
        };
        fire(&world);
        {
            // Phase 3: node 30 processed (no conditions) and carries no
            // Shares Event bit — it fires its quest and consumes the
            // event, so sibling 31 never evaluates.
            let stages = world.try_resource::<QuestStageState>().unwrap();
            assert!(stages.is_started(QuestFormId(0x000F_0A10)));
            assert!(!stages.is_started(QuestFormId(0x000F_0A11)));
        }
        // Second identical event: node 30's pool is exhausted (its quest
        // is running), so it fires nothing and — #5404 — no longer
        // consumes; the sibling below gets its turn. Node 30's quest is
        // not restarted.
        fire(&world);
        {
            let stages = world.try_resource::<QuestStageState>().unwrap();
            assert!(stages.is_started(QuestFormId(0x000F_0A10)));
            assert!(stages.is_started(QuestFormId(0x000F_0A11)));
        }
        // The marker drained: nothing left for a second pass.
        let remaining = world
            .query::<StoryEvent>()
            .map(|query| query.iter().count())
            .unwrap_or(0);
        assert_eq!(remaining, 0);

        // Unknown mnemonic: no crash, no quest starts.
        if let Some(mut events) = world.query_mut::<StoryEvent>() {
            events.insert(
                victim,
                StoryEvent {
                    mnemonic: *b"ZZZZ",
                    reference_1: victim,
                    reference_2: Some(killer),
                    location_1: None,
                    location_2: None,
                    scripted: None,
                },
            );
        }
        story_manager_dispatch_system(&world);
        let stages = world.try_resource::<QuestStageState>().unwrap();
        assert!(!stages.is_started(QuestFormId(0x0999)));
    }

    /// #5380 — a node gated on a condition function the evaluator
    /// cannot model declines instead of passing vacuously through the
    /// Unknown→0.0 default. The original shape: fn 145 `== 0`
    /// (`WIGreetingNodeSHARES`, the smoke's boot start) read 0.0 ==
    /// 0.0 and started a quest on nothing. The node must neither
    /// start the quest nor consume the event — and a fully-modeled
    /// sibling below it still gets its turn.
    #[test]
    fn unknown_condition_function_declines_the_node() {
        use byroredux_plugin::esm::records::condition::{ComparisonOp, Condition, ConditionValue};
        let unmodeled = SmNodeRecord {
            form_id: 20,
            parent: 10,
            // Head of the pair: a real sibling follows, and the walk
            // must continue past the declined node onto it.
            previous_sibling: 0,
            kind: SmNodeKind::Quest,
            quests: vec![SmQuestLink { form_id: 0x666, reset_hours: 0.0, ..Default::default() }],
            conditions: vec![Condition {
                function_index: 145, // uncatalogued — the vacuous shape
                comparator: ComparisonOp::Eq,
                comparand: ConditionValue::Literal(0.0),
                ..Default::default()
            }],
            ..Default::default()
        };
        let tree = build_story_manager_tree(&records(vec![
            event_node(10, b"CLOC"),
            unmodeled,
            quest_node(30, 10, 20, &[0x777]),
        ]));
        let mut world = setup_world();
        world.insert_resource(tree);
        world.insert_resource(QuestStageState::default());
        let actor = world.spawn();
        if let Some(mut events) = world.query_mut::<StoryEvent>() {
            events.insert(
                actor,
                StoryEvent {
                    mnemonic: *b"CLOC",
                    reference_1: actor,
                    reference_2: None,
                    location_1: None,
                    location_2: None,
                    scripted: None,
                },
            );
        }
        story_manager_dispatch_system(&world);
        let stages = world.try_resource::<QuestStageState>().unwrap();
        assert!(
            !stages.is_started(QuestFormId(0x666)),
            "a node gated on an uncatalogued function must not start its quest"
        );
        assert!(
            stages.is_started(QuestFormId(0x777)),
            "the event still walks past the declined node to the modeled sibling"
        );
    }

    /// #5380 — `CWChangeLocationScenes`' own gate, now cataloged:
    /// `GetQuestRunning(CWFinale) == 0` evaluates against real quest
    /// state instead of the vacuous 0.0 pass. Not running → 0 == 0 →
    /// starts for the authored reason; running → declines.
    #[test]
    fn get_quest_running_gate_evaluates_real_state() {
        use byroredux_plugin::esm::records::condition::{ComparisonOp, Condition, ConditionValue};
        let gate = |quest_running: bool| {
            let node = SmNodeRecord {
                form_id: 20,
                parent: 10,
                previous_sibling: 0,
                kind: SmNodeKind::Quest,
                quests: vec![SmQuestLink { form_id: 0x888, reset_hours: 0.0, ..Default::default() }],
                conditions: vec![Condition {
                    function_index: 56, // GetQuestRunning
                    comparator: ComparisonOp::Eq,
                    comparand: ConditionValue::Literal(0.0),
                    param_1: 0x000D_1444, // CWFinale
                    ..Default::default()
                }],
                ..Default::default()
            };
            let tree = build_story_manager_tree(&records(vec![event_node(10, b"CLOC"), node]));
            let mut world = setup_world();
            world.insert_resource(tree);
            let mut stages = QuestStageState::default();
            if quest_running {
                stages.start_quest(QuestFormId(0x000D_1444), None);
            }
            world.insert_resource(stages);
            let actor = world.spawn();
            if let Some(mut events) = world.query_mut::<StoryEvent>() {
                events.insert(
                    actor,
                    StoryEvent {
                        mnemonic: *b"CLOC",
                        reference_1: actor,
                        reference_2: None,
                        location_1: None,
                        location_2: None,
                        scripted: None,
                    },
                );
            }
            story_manager_dispatch_system(&world);
            let started = world
                .try_resource::<QuestStageState>()
                .unwrap()
                .is_started(QuestFormId(0x888));
            started
        };
        assert!(
            gate(false),
            "CWFinale not running → GetQuestRunning == 0 passes → the scene starts"
        );
        assert!(
            !gate(true),
            "CWFinale running → the gate declines the node"
        );
    }

    /// #5628: `GetRandomPercent` (CTDA fn 77) draws from `StoryManagerRng`,
    /// which the dispatcher used to hold WRITE across the whole walk — the
    /// first fn-77 evaluation re-entered it and the lock tracker panicked
    /// (vanilla `DA09ChangeLocationSharesEvent` shape: `[56, 77]`). Every
    /// other test world omits the RNG, so the evaluator took its no-RNG
    /// `50.0` branch and never exercised the re-entry.
    #[test]
    fn get_random_percent_gate_dispatches_with_the_rng_installed() {
        use byroredux_plugin::esm::records::condition::{ComparisonOp, Condition, ConditionValue};
        let dispatch = |conditions: Vec<Condition>, da09_running: bool| {
            let node = SmNodeRecord {
                form_id: 20,
                parent: 10,
                previous_sibling: 0,
                kind: SmNodeKind::Quest,
                quests: vec![SmQuestLink { form_id: 0x888, reset_hours: 0.0, ..Default::default() }],
                conditions,
                ..Default::default()
            };
            let tree = build_story_manager_tree(&records(vec![event_node(10, b"CLOC"), node]));
            let mut world = setup_world();
            world.insert_resource(tree);
            world.insert_resource(StoryManagerRng { state: 0x1234_5678 });
            let mut stages = QuestStageState::default();
            if da09_running {
                stages.start_quest(QuestFormId(0x0004_E4E1), None);
            }
            world.insert_resource(stages);
            let actor = world.spawn();
            if let Some(mut events) = world.query_mut::<StoryEvent>() {
                events.insert(
                    actor,
                    StoryEvent {
                        mnemonic: *b"CLOC",
                        reference_1: actor,
                        reference_2: None,
                        location_1: None,
                        location_2: None,
                        scripted: None,
                    },
                );
            }
            story_manager_dispatch_system(&world);
            let started = world
                .try_resource::<QuestStageState>()
                .unwrap()
                .is_started(QuestFormId(0x888));
            started
        };
        let random_percent = |op, comparand| Condition {
            function_index: 77, // GetRandomPercent
            comparator: op,
            comparand: ConditionValue::Literal(comparand),
            ..Default::default()
        };
        // A lone chance gate, the most common radiant shape: any draw in
        // 0..=100 passes `<= 100`, none passes `> 100`.
        assert!(dispatch(vec![random_percent(ComparisonOp::Le, 100.0)], false));
        assert!(!dispatch(vec![random_percent(ComparisonOp::Gt, 100.0)], false));
        // `DA09ChangeLocationSharesEvent`: `GetQuestRunning(DA09) == 1`
        // then the chance gate — the second term only runs once DA09 is up.
        let da09_running = Condition {
            function_index: 56,
            comparator: ComparisonOp::Eq,
            comparand: ConditionValue::Literal(1.0),
            param_1: 0x0004_E4E1,
            ..Default::default()
        };
        assert!(dispatch(
            vec![da09_running, random_percent(ComparisonOp::Le, 100.0)],
            true
        ));
        assert!(!dispatch(
            vec![da09_running, random_percent(ComparisonOp::Le, 100.0)],
            false
        ));
    }

    /// A branch node whose conditions fail gates its whole subtree off.
    /// GetActorValue on a bare test entity is 0.0, so `== 1.0` fails.
    #[test]
    fn failing_branch_conditions_skip_subtree() {
        use byroredux_plugin::esm::records::condition::{ComparisonOp, Condition, ConditionValue};
        let failing = SmNodeRecord {
            form_id: 20,
            parent: 10,
            previous_sibling: 0,
            kind: SmNodeKind::Branch,
            conditions: vec![Condition {
                function_index: 14, // GetActorValue
                comparator: ComparisonOp::Eq,
                comparand: ConditionValue::Literal(1.0),
                param_1: 0x0003_0301,
                ..Default::default()
            }],
            ..Default::default()
        };
        let tree = build_story_manager_tree(&records(vec![
            event_node(10, b"CLOC"),
            failing,
            quest_node(30, 20, 0, &[0x777]),
        ]));
        let mut world = setup_world();
        world.insert_resource(tree);
        world.insert_resource(QuestStageState::default());
        let actor = world.spawn();
        if let Some(mut events) = world.query_mut::<StoryEvent>() {
            events.insert(
                actor,
                StoryEvent {
                    mnemonic: *b"CLOC",
                    reference_1: actor,
                    reference_2: None,
                    location_1: None,
                    location_2: None,
                    scripted: None,
                },
            );
        }
        story_manager_dispatch_system(&world);
        let stages = world.try_resource::<QuestStageState>().unwrap();
        assert!(!stages.is_started(QuestFormId(0x777)));
    }

    /// The CLOC producer dedupes on the location key: first sight fires,
    /// repeats don't, a new key fires again, and a None key (mid
    /// transition) tracks the cursor without firing.
    #[test]
    fn change_location_fires_once_per_key() {
        let mut world = setup_world();
        world.insert_resource(StoryLocationCursor::default());
        let actor = world.spawn();
        assert!(emit_change_location_on_key_change(&world, Some(1), actor, None));
        assert!(!emit_change_location_on_key_change(&world, Some(1), actor, None));
        assert!(!emit_change_location_on_key_change(&world, None, actor, None));
        assert!(emit_change_location_on_key_change(&world, Some(2), actor, None));
        // Second call fired a fresh marker; the dispatcher drains both
        // across two runs (one marker per run by construction).
        story_manager_dispatch_system(&world);
        let remaining = world
            .query::<StoryEvent>()
            .map(|query| query.iter().count())
            .unwrap_or(0);
        assert_eq!(remaining, 0);
    }

/// The R-slot tag a CTDA tail (or ALFD) carries — helper so the tests
/// below read as the wire format does.
#[cfg(test)]
fn tag(bytes: &[u8; 2]) -> u32 {
    u32::from(bytes[0]) | (u32::from(bytes[1]) << 8)
}

/// M47.2 — a script event (`Keyword.SendStoryEvent`): an `SCPT` node gated
/// on `GetEventData GetIsID K1 == keyword` starts only for its keyword, and a
/// `GetValue V1` gate reads the event's first integer.
#[test]
fn script_event_keyword_and_value_gate_the_start() {
    use byroredux_plugin::esm::records::condition::{ComparisonOp, Condition, ConditionValue};
    const KEYWORD: u32 = 0x0004_1E8B;
    let event_data = |function: u32, member: &[u8; 2], param_2: u32, value: f32| Condition {
        function_index: 576,
        comparator: ComparisonOp::Eq,
        comparand: ConditionValue::Literal(value),
        param_1: function | (tag(member) << 16),
        param_2,
        ..Default::default()
    };
    let node = SmNodeRecord {
        form_id: 30,
        parent: 10,
        previous_sibling: 0,
        kind: SmNodeKind::Quest,
        quests: vec![SmQuestLink { form_id: 0x666, reset_hours: 0.0, ..Default::default() }],
        conditions: vec![event_data(0, b"K1", KEYWORD, 1.0), event_data(2, b"V1", 0, 7.0)],
        ..Default::default()
    };
    let tree = build_story_manager_tree(&records(vec![event_node(10, b"SCPT"), node]));
    let mut world = setup_world();
    world.insert_resource(tree);
    world.insert_resource(QuestStageState::default());
    world.insert_resource(StoryEventAliasFill::default());
    let carrier = world.spawn();
    let raise = |world: &World, keyword: u32, value: i32| {
        if let Some(mut events) = world.query_mut::<StoryEvent>() {
            events.insert(
                carrier,
                StoryEvent {
                    mnemonic: *b"SCPT",
                    reference_1: carrier,
                    reference_2: None,
                    location_1: None,
                    location_2: None,
                    scripted: Some(ScriptedStoryEvent {
                        keyword,
                        values: [value, 0],
                        has_reference_1: false,
                    }),
                },
            );
        }
        story_manager_dispatch_system(world);
    };
    let started = |world: &World| {
        world
            .try_resource::<QuestStageState>()
            .is_some_and(|stages| stages.is_started(QuestFormId(0x666)))
    };
    raise(&world, 0x0004_1E8C, 7);
    assert!(!started(&world), "another keyword");
    raise(&world, KEYWORD, 3);
    assert!(!started(&world), "the right keyword, the wrong value");
    raise(&world, KEYWORD, 7);
    assert!(started(&world));
}

/// M47.2 — several script events raised in one frame all dispatch (the
/// College's radiant starter sends one per teacher).
#[test]
fn queued_script_events_in_one_frame_all_dispatch() {
    use byroredux_plugin::esm::records::condition::{ComparisonOp, Condition, ConditionValue};
    let keyword_node = |form_id, previous_sibling, keyword, quest| SmNodeRecord {
        form_id,
        parent: 10,
        previous_sibling,
        kind: SmNodeKind::Quest,
        quests: vec![SmQuestLink { form_id: quest, reset_hours: 0.0, ..Default::default() }],
        conditions: vec![Condition {
            function_index: 576,
            comparator: ComparisonOp::Eq,
            comparand: ConditionValue::Literal(1.0),
            param_1: tag(b"K1") << 16,
            param_2: keyword,
            ..Default::default()
        }],
        ..Default::default()
    };
    let tree = build_story_manager_tree(&records(vec![
        event_node(10, b"SCPT"),
        keyword_node(20, 0, 0xA1, 0x901),
        keyword_node(30, 20, 0xA2, 0x902),
    ]));
    let mut world = setup_world();
    world.insert_resource(tree);
    world.insert_resource(QuestStageState::default());
    world.insert_resource(StoryEventAliasFill::default());
    let carrier = world.spawn();
    let event = |keyword| StoryEvent {
        mnemonic: *b"SCPT",
        reference_1: carrier,
        reference_2: None,
        location_1: None,
        location_2: None,
        scripted: Some(ScriptedStoryEvent {
            keyword,
            values: [0, 0],
            has_reference_1: false,
        }),
    };
    world
        .resource_mut::<PendingScriptStoryEvents>()
        .0
        .extend([event(0xA1), event(0xA2)]);
    story_manager_dispatch_system(&world);
    let stages = world.resource::<QuestStageState>();
    assert!(stages.is_started(QuestFormId(0x901)));
    assert!(stages.is_started(QuestFormId(0x902)));
    drop(stages);
    assert!(world.resource::<PendingScriptStoryEvents>().0.is_empty(), "drained");
}

/// M47.2 — a quest whose non-optional alias fills From Event R1 cannot start
/// from an event without R1 (the CK: an unfilled required alias fails the
/// start), so the Story Manager walks on: the unconditioned node above does
/// not swallow every script event. An optional alias does not block.
#[test]
fn a_required_from_event_alias_without_its_slot_fails_the_start() {
    use byroredux_plugin::esm::records::{
        AliasFillType, AliasFlags, QuestAlias, QustRecord, ALIAS_FLAG_OPTIONAL,
    };
    let quest_node = |form_id, previous_sibling, quest| SmNodeRecord {
        form_id,
        parent: 10,
        previous_sibling,
        kind: SmNodeKind::Quest,
        quests: vec![SmQuestLink { form_id: quest, reset_hours: 0.0, ..Default::default() }],
        ..Default::default()
    };
    // Marriage-shaped node first (no conditions), the intended node second.
    let tree = build_story_manager_tree(&records(vec![
        event_node(10, b"SCPT"),
        quest_node(20, 0, 0x777),
        quest_node(30, 20, 0x888),
    ]));
    let mut world = setup_world();
    world.insert_resource(tree);
    world.insert_resource(QuestStageState::default());
    world.insert_resource(StoryEventAliasFill::default());
    let from_r1 = |flags| QuestAlias {
        alias_id: 0,
        fill_type: Some(AliasFillType::FromEvent {
            event_type: *b"SCPT",
            data: tag(b"R1") as i32,
        }),
        flags: AliasFlags(flags),
        ..Default::default()
    };
    crate::install_scene_quest_aliases(
        &mut world,
        [QustRecord {
            form_id: 0x777,
            aliases: vec![from_r1(0)],
            ..Default::default()
        }],
    );
    let carrier = world.spawn();
    let raise = |world: &World| {
        if let Some(mut events) = world.query_mut::<StoryEvent>() {
            events.insert(
                carrier,
                StoryEvent {
                    mnemonic: *b"SCPT",
                    reference_1: carrier,
                    reference_2: None,
                    location_1: None,
                    location_2: None,
                    scripted: Some(ScriptedStoryEvent {
                        keyword: 1,
                        values: [0, 0],
                        has_reference_1: false,
                    }),
                },
            );
        }
        story_manager_dispatch_system(world);
    };
    let started = |world: &World, quest| {
        world
            .try_resource::<QuestStageState>()
            .is_some_and(|stages| stages.is_started(QuestFormId(quest)))
    };
    raise(&world);
    assert!(!started(&world, 0x777), "R1 required, the event has none");
    assert!(started(&world, 0x888), "the walk went on to the next node");

    // Optional: the alias may stay empty, and the first node fires.
    crate::install_scene_quest_aliases(
        &mut world,
        [QustRecord {
            form_id: 0x777,
            aliases: vec![from_r1(ALIAS_FLAG_OPTIONAL)],
            ..Default::default()
        }],
    );
    world.resource_mut::<QuestStageState>().stop(QuestFormId(0x888));
    raise(&world);
    assert!(started(&world, 0x777));
}

/// #5366 Phase 2 gate shape — the tutorial's killer conditions: a KILL
/// node gated on `GetIsID(player)` run on EventData R2 starts only when
/// the event's killer IS the player. Same node, same world, two events:
/// matching starts, non-matching does not.
#[test]
fn event_data_killer_condition_gates_the_start() {
    use byroredux_plugin::esm::records::condition::{ComparisonOp, Condition, ConditionValue};
    use byroredux_plugin::esm::records::{AliasFillType, QustRecord};

    const PLAYER_BASE: u32 = 0x0000_0007;
    let killer_node = SmNodeRecord {
        form_id: 30,
        parent: 10,
        previous_sibling: 0,
        kind: SmNodeKind::Quest,
        quests: vec![SmQuestLink { form_id: 0x555, reset_hours: 0.0, ..Default::default() }],
        conditions: vec![Condition {
            function_index: 72, // GetIsID
            comparator: ComparisonOp::Eq,
            comparand: ConditionValue::Literal(1.0),
            param_1: PLAYER_BASE,
            run_on: RunOn::EventData,
            extra_data_id: tag(b"R2"),
            ..Default::default()
        }],
        ..Default::default()
    };
    let tree = build_story_manager_tree(&records(vec![
        event_node(10, b"KILL"),
        killer_node,
    ]));
    let mut world = setup_world();
    world.insert_resource(tree);
    world.insert_resource(QuestStageState::default());
    world.insert_resource(StoryEventAliasFill::default());

    let victim = world.spawn();
    let player_killer = world.spawn();
    let other_killer = world.spawn();
    // #5419 — the refresh's FromEvent arm probes liveness, so the
    // event's references model live actors (a transform each).
    for entity in [victim, player_killer, other_killer] {
        world.insert(
            entity,
            byroredux_core::ecs::components::GlobalTransform::default(),
        );
    }
    world.insert(
        player_killer,
        crate::scene::SceneAliasCandidate {
            reference_form_id: 0x14,
            base_form_id: PLAYER_BASE,
            linked_refs: Vec::new(),
            location_ref_types: Vec::new(),
        },
    );
    world.insert(
        other_killer,
        crate::scene::SceneAliasCandidate {
            reference_form_id: 0x99,
            base_form_id: 0x0004_0A10,
            linked_refs: Vec::new(),
            location_ref_types: Vec::new(),
        },
    );

    // Non-matching event first: an NPC killer must NOT start the quest.
    if let Some(mut events) = world.query_mut::<StoryEvent>() {
        events.insert(
            victim,
            StoryEvent {
                mnemonic: *b"KILL",
                reference_1: victim,
                reference_2: Some(other_killer),
                location_1: None,
                location_2: None,
                scripted: None,
            },
        );
    }
    story_manager_dispatch_system(&world);
    assert!(
        !world
            .try_resource::<QuestStageState>()
            .is_some_and(|stages| stages.is_started(QuestFormId(0x555))),
        "NPC killer — GetIsID(player) on R2 fails, quest stays stopped"
    );

    // Matching event: the player killer starts it.
    if let Some(mut events) = world.query_mut::<StoryEvent>() {
        events.insert(
            victim,
            StoryEvent {
                mnemonic: *b"KILL",
                reference_1: victim,
                reference_2: Some(player_killer),
                location_1: None,
                location_2: None,
                scripted: None,
            },
        );
    }
    story_manager_dispatch_system(&world);
    assert!(
        world
            .try_resource::<QuestStageState>()
            .is_some_and(|stages| stages.is_started(QuestFormId(0x555))),
        "player killer — GetIsID(player) on R2 passes, quest starts"
    );

    // Phase 2's other half: the started quest's event data was recorded
    // for its FromEvent aliases, and the alias refresh was requested.
    let slots = world
        .try_resource::<StoryEventAliasFill>()
        .and_then(|fills| fills.0.get(&QuestFormId(0x555)).map(|fill| fill.slots));
    assert_eq!(
        slots,
        Some(crate::condition::EventDataSlots {
            reference_1: Some(victim),
            reference_2: Some(player_killer),
            location_1: None,
            location_2: None,
            ..Default::default()
        }),
        "the raising event's slots are recorded for FromEvent alias fills"
    );
    assert!(
        world
            .try_resource::<crate::scene::SceneActorBindings>()
            .is_some_and(|bindings| bindings.is_dirty()),
        "quest start requests an alias refresh so FromEvent fills land same-frame"
    );

    // And the recorded event feeds a FromEvent alias through the real
    // refresh (the full Phase-2 loop: event → conditions → start → fill).
    crate::scene::install_scene_quest_aliases(
        &mut world,
        [QustRecord {
            form_id: 0x555,
            aliases: vec![byroredux_plugin::esm::records::QuestAlias {
                alias_id: 4,
                fill_type: Some(AliasFillType::FromEvent {
                    event_type: *b"KILL",
                    data: tag(b"R1") as i32,
                }),
                ..Default::default()
            }],
            ..Default::default()
        }],
    );
    crate::scene::refresh_scene_actor_bindings(&world);
    assert_eq!(
        world
            .try_resource::<crate::scene::SceneActorBindings>()
            .and_then(|bindings| bindings.resolve(QuestFormId(0x555), 4)),
        Some(victim),
        "FromEvent R1 alias binds the event's victim"
    );
}

/// An R1-tagged condition reads the victim slot (WIKill06's `Victim`
/// alias shape mirrored on the condition side), and an L-tagged one
/// fails cleanly — no location-as-entity runtime yet.
#[test]
fn event_data_r1_resolves_and_location_tags_fail_cleanly() {
    use byroredux_plugin::esm::records::condition::{ComparisonOp, Condition, ConditionValue};
    let node = |extra: u32| SmNodeRecord {
        form_id: 30,
        parent: 10,
        previous_sibling: 0,
        kind: SmNodeKind::Quest,
        quests: vec![SmQuestLink { form_id: 0x556, reset_hours: 0.0, ..Default::default() }],
        conditions: vec![Condition {
            function_index: 72, // GetIsID
            comparator: ComparisonOp::Eq,
            comparand: ConditionValue::Literal(1.0),
            param_1: 0xDEAD,
            run_on: RunOn::EventData,
            extra_data_id: extra,
            ..Default::default()
        }],
        ..Default::default()
    };
    let victim = |world: &mut World, base: u32| {
        let entity = world.spawn();
        world.insert(
            entity,
            crate::scene::SceneAliasCandidate {
                reference_form_id: base,
                base_form_id: base,
                linked_refs: Vec::new(),
                location_ref_types: Vec::new(),
            },
        );
        entity
    };

    // R1 + matching base → fires.
    let mut world = setup_world();
    world.insert_resource(build_story_manager_tree(&records(vec![
        event_node(10, b"KILL"),
        node(tag(b"R1")),
    ])));
    world.insert_resource(QuestStageState::default());
    let target = victim(&mut world, 0xDEAD);
    if let Some(mut events) = world.query_mut::<StoryEvent>() {
        events.insert(
            target,
            StoryEvent {
                mnemonic: *b"KILL",
                reference_1: target,
                reference_2: None,
                location_1: None,
                location_2: None,
                scripted: None,
            },
        );
    }
    story_manager_dispatch_system(&world);
    assert!(world
        .try_resource::<QuestStageState>()
        .is_some_and(|stages| stages.is_started(QuestFormId(0x556))));

    // L2 tag on the same matching base → no entity to run on → no fire.
    let mut world = setup_world();
    world.insert_resource(build_story_manager_tree(&records(vec![
        event_node(10, b"CLOC"),
        node(tag(b"L2")),
    ])));
    world.insert_resource(QuestStageState::default());
    let actor = victim(&mut world, 0xDEAD);
    if let Some(mut events) = world.query_mut::<StoryEvent>() {
        events.insert(
            actor,
            StoryEvent {
                mnemonic: *b"CLOC",
                reference_1: actor,
                reference_2: None,
                location_1: None,
                location_2: Some(0x18A56),
                scripted: None,
            },
        );
    }
    story_manager_dispatch_system(&world);
    assert!(!world
        .try_resource::<QuestStageState>()
        .is_some_and(|stages| stages.is_started(QuestFormId(0x556))));

    // Unrecognized tag bytes → no fire, no crash.
    let mut world = setup_world();
    world.insert_resource(build_story_manager_tree(&records(vec![
        event_node(10, b"KILL"),
        node(0x00FF_00FF),
    ])));
    world.insert_resource(QuestStageState::default());
    let actor = victim(&mut world, 0xDEAD);
    if let Some(mut events) = world.query_mut::<StoryEvent>() {
        events.insert(
            actor,
            StoryEvent {
                mnemonic: *b"KILL",
                reference_1: actor,
                reference_2: None,
                location_1: None,
                location_2: None,
                scripted: None,
            },
        );
    }
    story_manager_dispatch_system(&world);
    assert!(!world
        .try_resource::<QuestStageState>()
        .is_some_and(|stages| stages.is_started(QuestFormId(0x556))));
}
    /// #5366 Phase 2 gate on real authored content — the KILL-subtree
    /// node `MGSuspension` (SMQN → QUST 0x0005B5DC) with its actual
    /// authored CTDAs: GetIsRace≠X on R2, GetIsID(player) on R2,
    /// GetInFaction on R2 and R1, GetStage-family == 0 on Subject.
    /// Same node, same world, three events: the matching killer starts
    /// the quest; a non-player killer and a non-faction victim each
    /// keep it stopped. `#[ignore]`'d like the plugin crate's floors —
    /// needs the Skyrim SE master on disk.
    #[test]
    #[ignore = "needs Skyrim SE game data on disk"]
    fn mgsuspension_kill_gate_on_real_skyrim_content() {
        use byroredux_core::ecs::components::FactionRanks;
        let data = byroredux_plugin::esm::test_paths::skyrim_se_data_dir();
        let esm = data.join("Skyrim.esm");
        if !esm.is_file() {
            eprintln!("[MGSuspension gate] skipping: no Skyrim.esm at {esm:?}");
            return;
        }
        let bytes = std::fs::read(&esm).expect("read Skyrim.esm");
        let index = byroredux_plugin::esm::parse_esm(&bytes).expect("parse Skyrim.esm");
        assert!(
            index.story_manager_nodes.len() >= 560,
            "the Phase-0 floor guard also guards this test's setup"
        );

        const MGSUSPENSION: u32 = 0x0005_B5DC;
        const COLLEGE_FACTION: u32 = 0x0001_F259;
        let killer_in_college = |world: &mut World, player: bool| {
            let entity = world.spawn();
            world.insert(
                entity,
                crate::scene::SceneAliasCandidate {
                    reference_form_id: if player { 0x14 } else { 0x9999 },
                    base_form_id: if player { 0x0000_0007 } else { 0x0004_0A10 },
                    linked_refs: Vec::new(),
                    location_ref_types: Vec::new(),
                },
            );
            if player {
                world.insert(entity, FactionRanks::from_pairs([(COLLEGE_FACTION, 0)]));
            }
            entity
        };
        let victim_in_college = |world: &mut World, member: bool| {
            let entity = world.spawn();
            if member {
                world.insert(entity, FactionRanks::from_pairs([(COLLEGE_FACTION, 0)]));
            }
            entity
        };
        let world_with_tree = || {
            let mut world = setup_world();
            let count = install_story_manager(&mut world, &index.story_manager_nodes);
            assert!(count >= 560);
            world.insert_resource(QuestStageState::default());
            world
        };
        let fire_kill = |world: &World, victim: EntityId, killer: EntityId| {
            if let Some(mut events) = world.query_mut::<StoryEvent>() {
                events.insert(
                    victim,
                    StoryEvent {
                        mnemonic: *b"KILL",
                        reference_1: victim,
                        reference_2: Some(killer),
                        location_1: None,
                        location_2: None,
                        scripted: None,
                    },
                );
            }
            story_manager_dispatch_system(world);
        };
        let started = |world: &World| {
            world
                .try_resource::<QuestStageState>()
                .is_some_and(|stages| stages.is_started(QuestFormId(MGSUSPENSION)))
        };

        // Negative first: an NPC killer (not the player) must not start it.
        let mut world = world_with_tree();
        let victim = victim_in_college(&mut world, true);
        let killer = killer_in_college(&mut world, false);
        fire_kill(&world, victim, killer);
        assert!(
            !started(&world),
            "NPC killer — GetIsID(player) on R2 fails against the authored CTDA"
        );

        // Negative: player killer, victim outside the faction.
        let mut world = world_with_tree();
        let victim = victim_in_college(&mut world, false);
        let killer = killer_in_college(&mut world, true);
        fire_kill(&world, victim, killer);
        assert!(
            !started(&world),
            "victim not in the faction — GetInFaction on R1 fails"
        );

        // Matching: player-in-college kills a college member.
        let mut world = world_with_tree();
        let victim = victim_in_college(&mut world, true);
        let killer = killer_in_college(&mut world, true);
        fire_kill(&world, victim, killer);
        assert!(
            started(&world),
            "the matching event starts MGSuspension through the real authored \
             conditions — the Phase-2 gate"
        );
        assert!(
            world
                .try_resource::<StoryEventAliasFill>()
                .is_some_and(|fills| fills.0.contains_key(&QuestFormId(MGSUSPENSION))),
            "the start records its event slots for FromEvent alias fills"
        );
    }
    /// Phase 3 — a sharing quest node passes the event on: both
    /// siblings fire. The consume of the previous test is this test's
    /// control group (same tree, one bit different).
    #[test]
    fn sharing_quest_node_passes_the_event_on() {
        let shares = SmNodePolicies {
            shares_event: true,
            ..Default::default()
        };
        let tree = build_story_manager_tree(&records(vec![
            event_node(10, b"KILL"),
            branch(20, 10, 0),
            quest_node_full(30, 20, 31, &[0x111], shares),
            quest_node_full(31, 20, 0, &[0x222], shares),
        ]));
        let mut world = setup_world();
        world.insert_resource(tree);
        world.insert_resource(QuestStageState::default());
        world.insert_resource(StoryEventAliasFill::default());
        let actor = world.spawn();
        if let Some(mut events) = world.query_mut::<StoryEvent>() {
            events.insert(
                actor,
                StoryEvent {
                    mnemonic: *b"KILL",
                    reference_1: actor,
                    reference_2: None,
                    location_1: None,
                    location_2: None,
                    scripted: None,
                },
            );
        }
        story_manager_dispatch_system(&world);
        let stages = world.try_resource::<QuestStageState>().unwrap();
        assert!(stages.is_started(QuestFormId(0x111)));
        assert!(stages.is_started(QuestFormId(0x222)));
    }

    /// #5404 — a non-sharing node that fires nothing (every pool entry
    /// already running) does NOT consume the event: the sibling below it
    /// still fires. Bethesda's tutorial keys consumption on a node
    /// "actually manag[ing] to fire a quest". Pre-fix `CWChangeLocationScenes`
    /// swallowed every CLOC once its four scenes were running.
    #[test]
    fn non_sharing_node_with_an_exhausted_pool_lets_the_event_through() {
        let tree = build_story_manager_tree(&records(vec![
            event_node(10, b"KILL"),
            quest_node(30, 10, 0, &[0x111]),
            quest_node(31, 10, 30, &[0x222]),
        ]));
        let mut world = setup_world();
        world.insert_resource(tree);
        // 0x111 already running: node 30 processes, starts nothing.
        let mut stages = QuestStageState::default();
        stages.start_quest(QuestFormId(0x111), None);
        world.insert_resource(stages);
        world.insert_resource(StoryEventAliasFill::default());
        let actor = world.spawn();
        if let Some(mut events) = world.query_mut::<StoryEvent>() {
            events.insert(
                actor,
                StoryEvent {
                    mnemonic: *b"KILL",
                    reference_1: actor,
                    reference_2: None,
                    location_1: None,
                    location_2: None,
                    scripted: None,
                },
            );
        }
        story_manager_dispatch_system(&world);
        let stages = world.try_resource::<QuestStageState>().unwrap();
        assert!(
            stages.is_started(QuestFormId(0x222)),
            "node 30 fired nothing, so it must not consume — node 31 fires"
        );
    }

    /// Phase 3 — do-all-before-repeating: across fires the pool cycles
    /// through every quest before repeating one, and the fired marks
    /// land in the persisted node state.
    #[test]
    fn do_all_before_repeating_round_robins_the_pool() {
        let do_all = SmNodePolicies {
            do_all_before_repeating: true,
            shares_event: true,
            ..Default::default()
        };
        let tree = build_story_manager_tree(&records(vec![
            event_node(10, b"KILL"),
            quest_node_full(30, 10, 0, &[0x111, 0x222, 0x333], do_all),
        ]));
        let mut world = setup_world();
        world.insert_resource(tree);
        world.insert_resource(QuestStageState::default());
        world.insert_resource(StoryManagerNodeState::default());
        world.insert_resource(StoryEventAliasFill::default());

        let started: Vec<u32> = (0..3)
            .map(|_| {
                {
                    let mut stages = world.try_resource_mut::<QuestStageState>().unwrap();
                    for q in [0x111, 0x222, 0x333] {
                        stages.stop(QuestFormId(q));
                    }
                }
                let actor = world.spawn();
                if let Some(mut events) = world.query_mut::<StoryEvent>() {
                    events.insert(
                        actor,
                        StoryEvent {
                            mnemonic: *b"KILL",
                            reference_1: actor,
                            reference_2: None,
                            location_1: None,
                            location_2: None,
                            scripted: None,
                        },
                    );
                }
                story_manager_dispatch_system(&world);
                let stages = world.try_resource::<QuestStageState>().unwrap();
                [0x111, 0x222, 0x333]
                    .into_iter()
                    .find(|q| stages.is_running(QuestFormId(*q)))
                    .expect("each fire starts exactly one pool quest")
            })
            .collect();
        assert_eq!(
            started,
            vec![0x111, 0x222, 0x333],
            "stacked do-all visits the pool in order, never repeating"
        );

        // The marks persist in the node state resource.
        {
            let state = world.try_resource::<StoryManagerNodeState>().unwrap();
            let runtime = state.nodes.get(&30).expect("state recorded for node 30");
            assert_eq!(runtime.fired, vec![true, true, true]);
        }

        // Fourth fire (after stopping the quests so all are eligible
        // again): the pool wrapped — every entry re-eligible, marks
        // cleared by the wrap, first picked again.
        {
            let mut stages = world.try_resource_mut::<QuestStageState>().unwrap();
            for q in [0x111, 0x222, 0x333] {
                stages.stop(QuestFormId(q));
            }
        }
        let actor = world.spawn();
        if let Some(mut events) = world.query_mut::<StoryEvent>() {
            events.insert(
                actor,
                StoryEvent {
                    mnemonic: *b"KILL",
                    reference_1: actor,
                    reference_2: None,
                    location_1: None,
                    location_2: None,
                    scripted: None,
                },
            );
        }
        story_manager_dispatch_system(&world);
        let state = world.try_resource::<StoryManagerNodeState>().unwrap();
        let runtime = state.nodes.get(&30).unwrap();
        assert_eq!(
            runtime.fired,
            vec![true, false, false],
            "the wrap cleared the round-robin before the new mark"
        );
    }

    /// Phase 3 — `RNAM` reset window: a fired pool entry is blocked
    /// inside its window and eligible again once the game clock passes
    /// it.
    #[test]
    fn rnam_reset_window_gates_refires() {
        let node = SmNodeRecord {
            form_id: 30,
            parent: 10,
            previous_sibling: 0,
            kind: SmNodeKind::Quest,
            quests: vec![
                SmQuestLink {
                    form_id: 0x111,
                    reset_hours: 48.0,
                    ..Default::default()
                },
                SmQuestLink {
                    form_id: 0x222,
                    reset_hours: 0.0,
                    ..Default::default()
                },
            ],
            policies: SmNodePolicies {
                shares_event: true,
                ..Default::default()
            },
            ..Default::default()
        };
        let tree = build_story_manager_tree(&records(vec![
            event_node(10, b"KILL"),
            node,
        ]));
        let mut world = setup_world();
        world.insert_resource(tree);
        world.insert_resource(QuestStageState::default());
        world.insert_resource(StoryManagerNodeState::default());
        world.insert_resource(StoryEventAliasFill::default());
        world.insert_resource(StoryClock { hours: 100.0 });
        let fire = |world: &mut World| {
            let actor = world.spawn();
            if let Some(mut events) = world.query_mut::<StoryEvent>() {
                events.insert(
                    actor,
                    StoryEvent {
                        mnemonic: *b"KILL",
                        reference_1: actor,
                        reference_2: None,
                        location_1: None,
                        location_2: None,
                        scripted: None,
                    },
                );
            }
            story_manager_dispatch_system(world);
        };

        // Stop 0x111 between fires so eligibility is the reset window's,
        // not is_started's.
        let stop = |world: &World| {
            let mut stages = world.try_resource_mut::<QuestStageState>().unwrap();
            stages.stop(QuestFormId(0x111));
            stages.stop(QuestFormId(0x222));
        };

        fire(&mut world); // starts 0x111 (first eligible)
        stop(&world);
        // 10 hours later: inside 0x111's 48h window → 0x222 instead.
        world.insert_resource(StoryClock { hours: 110.0 });
        fire(&mut world);
        {
            let state = world.try_resource::<StoryManagerNodeState>().unwrap();
            assert_eq!(
                state.nodes[&30].last_fire_hours,
                vec![100.0, 110.0],
                "both entries carry their fire timestamps"
            );
        }
        stop(&world);
        // 50 hours after 0x111's fire: window passed → 0x111 again.
        world.insert_resource(StoryClock { hours: 150.0 });
        fire(&mut world);
        {
            let state = world.try_resource::<StoryManagerNodeState>().unwrap();
            assert_eq!(state.nodes[&30].last_fire_hours, vec![150.0, 110.0]);
        }
    }

    /// #5406 — phase B honors `start_quest`'s refusal and applies the
    /// CK's reset-on-start rule. A COMPLETED Run Once radiant is
    /// ineligible, so the stacked pool's second entry gets the pick
    /// instead of the pool jamming on its first slot forever; a
    /// COMPLETED repeatable (non-Run-Once) radiant is reset and reruns
    /// from a clean stage history instead of resuming the finished run.
    #[test]
    fn completed_radiants_unjam_run_once_and_rerun_repeatable() {
        use byroredux_plugin::esm::records::QustRecord;
        let tree = build_story_manager_tree(&records(vec![
            event_node(10, b"KILL"),
            quest_node_full(30, 10, 0, &[0x111, 0x222], Default::default()),
        ]));
        let mut world = setup_world();
        world.insert_resource(tree);
        world.insert_resource(QuestStageState::default());
        world.insert_resource(StoryManagerNodeState::default());
        world.insert_resource(StoryEventAliasFill::default());
        world.insert_resource(QuestDefinitionRegistry::default());
        // 0x111 is authored Run Once (DNAM bit 8); 0x222 is a
        // repeatable radiant. Neither needs the Start Game Enabled bit
        // — the definitions registry is populated from every record.
        crate::quest_stages::install_start_game_quests(
            &mut world,
            [
                QustRecord {
                    form_id: 0x111,
                    editor_id: "WIOnce".into(),
                    quest_flags: 0x0100,
                    ..Default::default()
                },
                QustRecord {
                    form_id: 0x222,
                    editor_id: "WILoop".into(),
                    ..Default::default()
                },
            ],
        );

        // A played-out session: both pool entries already completed.
        {
            let mut stages = world.try_resource_mut::<QuestStageState>().unwrap();
            for quest in [0x111, 0x222] {
                stages.start_quest(QuestFormId(quest), None);
                stages.set_stage(QuestFormId(quest), 20);
                stages.complete(QuestFormId(quest));
            }
        }

        let fire = |world: &mut World| {
            let actor = world.spawn();
            if let Some(mut events) = world.query_mut::<StoryEvent>() {
                events.insert(
                    actor,
                    StoryEvent {
                        mnemonic: *b"KILL",
                        reference_1: actor,
                        reference_2: None,
                        location_1: None,
                        location_2: None,
                        scripted: None,
                    },
                );
            }
            story_manager_dispatch_system(world);
        };
        fire(&mut world);
        {
            let stages = world.try_resource::<QuestStageState>().unwrap();
            assert!(
                !stages.is_running(QuestFormId(0x111)) && stages.is_completed(QuestFormId(0x111)),
                "the completed Run Once entry stays done — pre-fix it was logged as started"
            );
            assert!(
                stages.is_running(QuestFormId(0x222)),
                "the completed repeatable entry got the pick, not a jam"
            );
            assert_eq!(
                stages.get_stage(QuestFormId(0x222)),
                0,
                "the rerun starts from a reset history, not the finished run's stage 20"
            );
        }
        // Only the quest that actually started consumed its pool mark.
        {
            let state = world.try_resource::<StoryManagerNodeState>().unwrap();
            assert_eq!(state.nodes[&30].fired, vec![false, true]);
        }

        // The rerun completes again; the next fire repeats the same
        // shape (0x111 ineligible forever, 0x222 rerunning).
        {
            let mut stages = world.try_resource_mut::<QuestStageState>().unwrap();
            stages.stop(QuestFormId(0x222));
            stages.complete(QuestFormId(0x222));
        }
        fire(&mut world);
        {
            let stages = world.try_resource::<QuestStageState>().unwrap();
            assert!(
                stages.is_running(QuestFormId(0x222)),
                "a completed repeatable reruns on the next event"
            );
            assert!(stages.is_completed(QuestFormId(0x111)));
        }
    }

    /// Phase 3 — a random parent visits its whole child chain (seeded
    /// shuffle changes order, never membership), and a random quest
    /// node picks uniformly from its pool: across many fires every pool
    /// entry is picked.
    #[test]
    fn random_parent_and_pool_visit_everything() {
        let random = SmNodePolicies {
            random: true,
            shares_event: true,
            ..Default::default()
        };
        // Random branch with two condition-free quest children.
        let tree = build_story_manager_tree(&records(vec![
            event_node(10, b"KILL"),
            SmNodeRecord {
                form_id: 20,
                parent: 10,
                previous_sibling: 0,
                kind: SmNodeKind::Branch,
                policies: random,
                ..Default::default()
            },
            quest_node_full(30, 20, 0, &[0x111], random),
            quest_node_full(31, 20, 30, &[0x222], random),
        ]));
        let mut world = setup_world();
        world.insert_resource(tree);
        world.insert_resource(QuestStageState::default());
        world.insert_resource(StoryManagerNodeState::default());
        world.insert_resource(StoryEventAliasFill::default());
        world.insert_resource(StoryManagerRng::default());

        let mut seen_first = std::collections::HashSet::new();
        for _ in 0..40 {
            let mut stages = world.try_resource_mut::<QuestStageState>().unwrap();
            stages.stop(QuestFormId(0x111));
            stages.stop(QuestFormId(0x222));
            drop(stages);
            let actor = world.spawn();
            if let Some(mut events) = world.query_mut::<StoryEvent>() {
                events.insert(
                    actor,
                    StoryEvent {
                        mnemonic: *b"KILL",
                        reference_1: actor,
                        reference_2: None,
                        location_1: None,
                        location_2: None,
                        scripted: None,
                    },
                );
            }
            story_manager_dispatch_system(&world);
            let stages = world.try_resource::<QuestStageState>().unwrap();
            assert!(
                stages.is_running(QuestFormId(0x111)) ^ stages.is_running(QuestFormId(0x222)),
                "exactly one child fires per event"
            );
            if stages.is_running(QuestFormId(0x111)) {
                seen_first.insert(0);
            } else {
                seen_first.insert(1);
            }
        }
        assert_eq!(
            seen_first.len(),
            2,
            "the seeded shuffle/pick reaches both children across fires"
        );
    }
    /// #5366 Phase 4 gate — the FO4 radiant-pool shape on real authored
    /// content: `WorkshopAttackNode` (SMQN 0x00250E6C, under the `SCPT`
    /// root) carries a 15-quest pool with random +
    /// do-all-before-repeating — the same policy word as the
    /// Minutemen-recruitment node the design doc names — and a
    /// condition-free path from its event root (corpus-checked; the
    /// Minutemen node itself gates on fn 576/56/77, all outside the
    /// catalog, so the gate runs on this sibling shape). Across fires
    /// it must cycle every pool quest before repeating one.
    /// `#[ignore]`d like the MGSuspension gate — needs Fallout4.esm.
    #[test]
    #[ignore = "needs FO4 game data on disk"]
    fn fo4_radiant_pool_cycles_on_real_content() {
        let data = byroredux_plugin::esm::test_paths::fo4_data_dir();
        let esm = data.join("Fallout4.esm");
        if !esm.is_file() {
            eprintln!("[FO4 pool gate] skipping: no Fallout4.esm at {esm:?}");
            return;
        }
        let bytes = std::fs::read(&esm).expect("read Fallout4.esm");
        let index = byroredux_plugin::esm::parse_esm(&bytes).expect("parse Fallout4.esm");

        const NODE: u32 = 0x0017_39B1;
        const SUPER_MUTANT_FACTION: u32 = 0x0005_8305;
        let record = index
            .story_manager_nodes
            .get(&NODE)
            .expect("SuperMutantConversationQuests must exist in Fallout4.esm");
        assert_eq!(record.editor_id, "SuperMutantConversationQuests");
        assert!(record.policies.random && record.policies.shares_event);
        assert!(
            record.quests.len() >= 4,
            "the super-mutant conversation pool (census: 4): {}",
            record.quests.len()
        );
        let pool: Vec<u32> = record.quests.iter().map(|q| q.form_id).collect();

        let mut world = setup_world();
        install_story_manager(&mut world, &index.story_manager_nodes);
        world.insert_resource(QuestStageState::default());
        // Fixed seed: the random pool pick is deterministic, so the
        // all-entries-cycled assertion below is not a flake.
        world.insert_resource(StoryManagerRng { state: 0x1234_5678 });
        let actor_a = world.spawn();
        let actor_b = world.spawn();
        for actor in [actor_a, actor_b] {
            world.insert(
                actor,
                byroredux_core::ecs::components::FactionRanks::from_pairs([(
                    SUPER_MUTANT_FACTION,
                    0,
                )]),
            );
        }

        let fire_adia = |world: &mut World| -> u32 {
            if let Some(mut events) = world.query_mut::<StoryEvent>() {
                events.insert(
                    actor_a,
                    StoryEvent {
                        mnemonic: *b"ADIA",
                        reference_1: actor_a,
                        reference_2: Some(actor_b),
                        location_1: None,
                        location_2: None,
                        scripted: None,
                    },
                );
            }
            story_manager_dispatch_system(world);
            // One pool quest runs per fire; stop it so the next fire is
            // eligible again, and report which one ran.
            let mut stages = world.try_resource_mut::<QuestStageState>().unwrap();
            let started = pool
                .iter()
                .copied()
                .find(|q| stages.is_running(QuestFormId(*q)))
                .expect("each ADIA fire starts exactly one pool quest");
            stages.stop(QuestFormId(started));
            started
        };

        // Without the faction, the R1/R2 GetInFaction gate must refuse.
        let outsider_a = world.spawn();
        let outsider_b = world.spawn();
        if let Some(mut events) = world.query_mut::<StoryEvent>() {
            events.insert(
                outsider_a,
                StoryEvent {
                    mnemonic: *b"ADIA",
                    reference_1: outsider_a,
                    reference_2: Some(outsider_b),
                    location_1: None,
                    location_2: None,
                    scripted: None,
                },
            );
        }
        story_manager_dispatch_system(&world);
        {
            let stages = world.try_resource::<QuestStageState>().unwrap();
            assert!(
                pool.iter().all(|q| !stages.is_running(QuestFormId(*q))),
                "actors outside the authored faction — the FO4 event-data gate refuses"
            );
        }

        // With it: random selection cycles every pool quest across fires.
        let mut fired: Vec<u32> = Vec::new();
        for _ in 0..24 {
            fired.push(fire_adia(&mut world));
        }
        let distinct = fired.iter().copied().collect::<std::collections::HashSet<_>>();
        assert_eq!(
            distinct.len(),
            pool.len(),
            "the radiant pool cycles different quests across fires (seeded): \
             started {fired:#X?} of pool {pool:#X?}"
        );
    }
}
