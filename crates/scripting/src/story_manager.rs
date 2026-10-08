//! Story Manager dispatch runtime — #5366 Phases 1–2.
//!
//! Folds the parsed `SMBN`/`SMEN`/`SMQN` node map
//! ([`EsmIndex::story_manager_nodes`]) into an [`SmTree`] whose children
//! are ordered by the authored `SNAM` sibling chains, then dispatches
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
//! processed quest node without `Shares Event` consumes the event once
//! it finishes, quest pools honor do-all-before-repeating round-robin
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
use crate::quest_stages::{QuestDefinitionRegistry, QuestFormId, QuestStageState};
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
}

impl StoryEvent {
    /// The [`EventDataSlots`] view the condition evaluator and the
    /// `FromEvent` alias fill consume.
    pub fn slots(&self) -> EventDataSlots {
        EventDataSlots {
            reference_1: Some(self.reference_1),
            reference_2: self.reference_2,
            location_1: self.location_1,
            location_2: self.location_2,
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
}

/// One node of the folded [`SmTree`] arena.
pub struct SmTreeNode {
    pub form_id: u32,
    pub editor_id: String,
    pub kind: SmNodeKind,
    pub event_mnemonic: Option<[u8; 4]>,
    pub conditions: ConditionList,
    /// Authored `SNAM` chain, resolved to arena indices. Sibling order
    /// is the evaluation stack (corpus-verified: #5366 design doc §3).
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
/// after a load the first post-load frame fires a fresh CLOC, which is
/// the engine behavior the event describes anyway (you changed location
/// by loading).
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

/// Event data each SM-started quest's `FromEvent` aliases fill from,
/// keyed by quest — written by the dispatcher's start phase, read by the
/// P4 alias refresh (`refresh_scene_actor_bindings`), which owns the
/// binding table. Latest fire wins (a re-fired radiant re-fills); an
/// entry stays for the session so an alias refresh after a cell reload
/// can still fill, matching how world-candidate fills behave.
/// `NOT_SAVED_BY_DESIGN`: after a load the quests restart through fresh
/// events, which rewrite their entries.
#[derive(Debug, Default)]
pub struct StoryEventAliasFill(pub HashMap<QuestFormId, EventDataSlots>);

impl Resource for StoryEventAliasFill {}

/// Fold the parsed node records into an [`SmTree`].
///
/// Children of each parent are ordered by following the authored
/// `SNAM` sibling chain from its head — the sibling that no in-group
/// sibling points *at*. Malformed chains (cycles, headless groups —
/// none exist in any vanilla master, but a mod can author anything)
/// fall back to the lowest-index member as head; the walker's visited
/// bitmap keeps traversal terminating regardless.
pub fn build_story_manager_tree(records: &HashMap<u32, SmNodeRecord>) -> SmTree {
    // Sorted by FormID so the arena order is deterministic across
    // HashMap iteration orders.
    let mut sorted: Vec<&SmNodeRecord> = records.values().collect();
    sorted.sort_by_key(|record| record.form_id);

    let mut tree = SmTree::default();
    let mut by_form_id: HashMap<u32, usize> = HashMap::with_capacity(sorted.len());
    for record in sorted {
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

    // Resolve sibling pointers to arena indices; dangling (cross-plugin,
    // not-yet-loaded) targets stay `None` — the chain simply ends there.
    for record in records.values() {
        let Some(&index) = by_form_id.get(&record.form_id) else {
            continue;
        };
        tree.nodes[index].next_sibling = by_form_id.get(&record.next_sibling).copied();
    }

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
    // member no in-group sibling points at.
    let mut children_by_parent: HashMap<u32, Vec<usize>> = HashMap::new();
    for record in records.values() {
        if record.parent != 0 && by_form_id.contains_key(&record.parent) {
            let index = by_form_id[&record.form_id];
            children_by_parent.entry(record.parent).or_default().push(index);
        }
    }
    for (parent, members) in children_by_parent {
        let mut targeted = vec![false; members.len()];
        for &member in &members {
            if let Some(sibling) = tree.nodes[member].next_sibling {
                if let Some(position) = members.iter().position(|&m| m == sibling) {
                    targeted[position] = true;
                }
            }
        }
        let head = members
            .iter()
            .zip(&targeted)
            .find(|(_, targeted)| !**targeted)
            .map(|(member, _)| *member)
            .unwrap_or(members[0]);
        tree.nodes[by_form_id[&parent]].first_child = Some(head);
    }

    tree
}

/// Build and install the [`SmTree`] (and the CLOC cursor) on the world.
/// Returns the node count for the caller's install log. Call from the
/// load-order install pass where `EsmIndex` is in scope — the sibling
/// of `install_start_game_quests`.
pub fn install_story_manager(world: &mut World, records: &HashMap<u32, SmNodeRecord>) -> usize {
    let tree = build_story_manager_tree(records);
    let count = tree.nodes.len();
    world.insert_resource(tree);
    if world.try_resource::<StoryLocationCursor>().is_none() {
        world.insert_resource(StoryLocationCursor::default());
    }
    if world.try_resource::<StoryEventAliasFill>().is_none() {
        world.insert_resource(StoryEventAliasFill::default());
    }
    // Phase 3 — node-policy state (saved), the game-hours clock the
    // engine syncs from GameTimeRes, and the selection RNG.
    if world.try_resource::<StoryManagerNodeState>().is_none() {
        world.insert_resource(StoryManagerNodeState::default());
    }
    if world.try_resource::<StoryClock>().is_none() {
        world.insert_resource(StoryClock::default());
    }
    if world.try_resource::<StoryManagerRng>().is_none() {
        world.insert_resource(StoryManagerRng::default());
    }
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
/// Pattern B: the drain at the head is unconditional — no early return
/// sits between the top of the system and it.
pub fn story_manager_dispatch_system(world: &World) {
    let events: Vec<StoryEvent> = world
        .query::<StoryEvent>()
        .map(|query| query.iter().map(|(_, event)| *event).collect())
        .unwrap_or_default();
    crate::scene::playback::drain::<StoryEvent>(world);
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
        let stages = world.try_resource::<QuestStageState>();
        let node_state = world.try_resource::<StoryManagerNodeState>();
        let mut rng = world.try_resource_mut::<StoryManagerRng>();
        let mut candidates = Vec::new();
        let mut wraps = Vec::new();
        for event in &events {
            let mut walk = WalkState {
                event,
                slots: event.slots(),
                registry: registry.as_deref(),
                stages: stages.as_deref(),
                candidates: &mut candidates,
                node_state: node_state.as_deref(),
                rng: rng.as_deref_mut(),
                hours,
                consumed: false,
                pending_wraps: Vec::new(),
            };
            dispatch_story_event(world, &tree, &mut walk);
            wraps.append(&mut walk.pending_wraps);
        }
        (candidates, wraps)
    };
    let Some(mut stages) = world.try_resource_mut::<QuestStageState>() else {
        return;
    };
    for candidate in &outcome.0 {
        if stages.is_running(candidate.quest) {
            continue;
        }
        stages.start_quest(candidate.quest, candidate.start_up_stage);
        // Phase 3 — the pool mark (do-all `fired`, `RNAM`
        // `last_fire_hours`) lands only for quests that actually
        // started, with any do-all wrap applied first.
        if let Some(mut node_state) = world.try_resource_mut::<StoryManagerNodeState>() {
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
        // Phase 2 — record the event's slots for this quest's `FromEvent`
        // aliases and mark the binding table dirty so the alias refresh
        // (scheduled right after this system) re-fills with them. One
        // resource write at a time: the fill map first, then bindings.
        if let Some(mut fills) = world.try_resource_mut::<StoryEventAliasFill>() {
            fills.0.insert(candidate.quest, candidate.slots);
        }
        if let Some(mut bindings) = world.try_resource_mut::<SceneActorBindings>() {
            bindings.request_refresh();
        }
        log::info!(
            "#5366 story manager: started quest {:#010X} ('{}') via node '{}' \
             on '{}' event",
            candidate.quest.0,
            candidate.editor_id,
            candidate.node_editor_id,
            String::from_utf8_lossy(&candidate.mnemonic),
        );
    }
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
    stages: Option<&'a QuestStageState>,
    candidates: &'a mut Vec<QuestStartCandidate>,
    /// Phase 3 — node-policy state (do-all marks, reset timestamps),
    /// read during the walk; writes go through `pending_marks` and are
    /// applied by phase B only for quests it actually starts.
    node_state: Option<&'a StoryManagerNodeState>,
    rng: Option<&'a mut StoryManagerRng>,
    /// Total game hours for `RNAM` reset windows (`StoryClock`).
    hours: f64,
    /// `true` once a processed non-sharing quest node consumed the
    /// event — every remaining sibling walk stops.
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
/// - **Consume** — a PROCESSED quest node without `Shares Event`
///   (`DNAM & 0x20000`) consumes the event once it finishes: every
///   remaining sibling walk stops. Per the CK rule text ("the event
///   will be consumed and the Story Manager will stop as soon as it
///   finishes with that node") this keys on the node being processed,
///   not on a quest actually starting — the wiki's own compatibility
///   warning ("higher on the list than any quest node that doesn't
///   [share]") is about placement, not firing.
fn walk_siblings(
    world: &World,
    tree: &SmTree,
    state: &mut WalkState<'_>,
    head: Option<usize>,
    parent_random: bool,
    visited: &mut [bool],
) {
    let Some(head) = head else { return };
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
        if let Some(rng) = state.rng.as_deref_mut() {
            for i in (1..chain.len()).rev() {
                let j = rng.pick(i + 1);
                chain.swap(i, j);
            }
        }
    }
    for index in chain {
        if state.consumed {
            return;
        }
        let node = &tree.nodes[index];
        // Subject = the doer: R2 when the event carries one (KILL
        // killer), else R1 (CLOC actor). Target = R1 (KILL victim).
        // `RunOn::EventData` tags resolve through the same slots.
        let mut context = ConditionContext::for_subject(
            state.event.reference_2.unwrap_or(state.event.reference_1),
        )
        .with_event_data(&state.slots);
        context.target = Some(state.event.reference_1);
        if evaluate(&node.conditions, world, &context) {
            let queued_before = state.candidates.len();
            if node.kind == SmNodeKind::Quest {
                process_quest_node(tree, state, index);
            }
            walk_siblings(world, tree, state, node.first_child, node.policies.random, visited);
            // A processed non-sharing quest node consumes the event —
            // after its own subtree finishes.
            if node.kind == SmNodeKind::Quest && !node.policies.shares_event {
                state.consumed = true;
                return;
            }
            // Random parent, choose-one: the CK tutorial's "it will
            // choose one of its child nodes randomly" — the chain stops
            // at the first child that queued a start, so a random
            // branch fires one quest per event even when every child
            // shares.
            if parent_random
                && node.kind == SmNodeKind::Quest
                && state.candidates.len() > queued_before
            {
                return;
            }
        }
    }
}

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
fn process_quest_node(tree: &SmTree, state: &mut WalkState<'_>, index: usize) {
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
            if state.stages.is_some_and(|stages| stages.is_running(quest)) {
                return false;
            }
            hours_gate_open(
                node.quests[i].reset_hours,
                last_fire.get(i).copied(),
                state.hours,
            )
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
        match state.rng.as_deref_mut() {
            Some(rng) => preferred[rng.pick(preferred.len())],
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

    fn branch(form_id: u32, parent: u32, next_sibling: u32) -> SmNodeRecord {
        SmNodeRecord {
            form_id,
            parent,
            next_sibling,
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

    fn quest_node(form_id: u32, parent: u32, next_sibling: u32, quests: &[u32]) -> SmNodeRecord {
        quest_node_full(form_id, parent, next_sibling, quests, SmNodePolicies::default())
    }

    fn quest_node_full(
        form_id: u32,
        parent: u32,
        next_sibling: u32,
        quests: &[u32],
        policies: SmNodePolicies,
    ) -> SmNodeRecord {
        SmNodeRecord {
            form_id,
            parent,
            next_sibling,
            kind: SmNodeKind::Quest,
            quests: quests
                .iter()
                .map(|&form_id| SmQuestLink {
                    form_id,
                    reset_hours: 0.0,
                })
                .collect(),
            policies,
            ..Default::default()
        }
    }

    fn records(nodes: Vec<SmNodeRecord>) -> HashMap<u32, SmNodeRecord> {
        nodes.into_iter().map(|node| (node.form_id, node)).collect()
    }

    /// Arena order is FormID-sorted and children follow the authored
    /// SNAM chain from the chain head, not insertion order.
    #[test]
    fn children_ordered_by_sibling_chain_from_head() {
        // Event KILL (10) with children 1 → 2 → 3 by SNAM, contributed
        // in shuffled map order, plus a mid-chain parent (2) with its
        // own child (4).
        let tree = build_story_manager_tree(&records(vec![
            quest_node(3, 10, 0, &[0xAAA]),
            event_node(10, b"KILL"),
            quest_node(1, 10, 2, &[0x111]),
            quest_node(2, 10, 3, &[0x222]),
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
            quest_node(30, 20, 31, &[0x000F_0A10]),
            quest_node(31, 20, 0, &[0x000F_0A11]),
        ]));
        let mut world = setup_world();
        world.insert_resource(tree);
        world.insert_resource(QuestStageState::default());
        let killer = world.spawn();
        let victim = world.spawn();
        for _ in 0..2 {
            if let Some(mut events) = world.query_mut::<StoryEvent>() {
                events.insert(
                    victim,
                    StoryEvent {
                        mnemonic: *b"KILL",
                        reference_1: victim,
                        reference_2: Some(killer),
                        location_1: None,
                        location_2: None,
                    },
                );
            }
            story_manager_dispatch_system(&world);
        }
        {
            // Phase 3: node 30 processed (no conditions) and carries no
            // Shares Event bit — it consumes the event after starting
            // its quest, so sibling 31 never evaluates.
            let stages = world.try_resource::<QuestStageState>().unwrap();
            assert!(stages.is_started(QuestFormId(0x000F_0A10)));
            assert!(!stages.is_started(QuestFormId(0x000F_0A11)));
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
                },
            );
        }
        story_manager_dispatch_system(&world);
        let stages = world.try_resource::<QuestStageState>().unwrap();
        assert!(!stages.is_started(QuestFormId(0x0999)));
    }

    /// A branch node whose conditions fail gates its whole subtree off.
    /// GetActorValue on a bare test entity is 0.0, so `== 1.0` fails.
    #[test]
    fn failing_branch_conditions_skip_subtree() {
        use byroredux_plugin::esm::records::condition::{ComparisonOp, Condition, ConditionValue};
        let failing = SmNodeRecord {
            form_id: 20,
            parent: 10,
            next_sibling: 0,
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
        next_sibling: 0,
        kind: SmNodeKind::Quest,
        quests: vec![SmQuestLink { form_id: 0x555, reset_hours: 0.0 }],
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
        .and_then(|fills| fills.0.get(&QuestFormId(0x555)).copied());
    assert_eq!(
        slots,
        Some(crate::condition::EventDataSlots {
            reference_1: Some(victim),
            reference_2: Some(player_killer),
            location_1: None,
            location_2: None,
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
        next_sibling: 0,
        kind: SmNodeKind::Quest,
        quests: vec![SmQuestLink { form_id: 0x556, reset_hours: 0.0 }],
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
                },
            );
        }
        story_manager_dispatch_system(&world);
        let stages = world.try_resource::<QuestStageState>().unwrap();
        assert!(stages.is_started(QuestFormId(0x111)));
        assert!(stages.is_started(QuestFormId(0x222)));
    }

    /// Phase 3 — a processed non-sharing node consumes the event even
    /// when its pool starts nothing (every entry already running): the
    /// CK rule keys on processing, not on starting.
    #[test]
    fn processed_non_sharing_node_consumes_even_without_a_start() {
        let tree = build_story_manager_tree(&records(vec![
            event_node(10, b"KILL"),
            quest_node(30, 10, 31, &[0x111]),
            quest_node(31, 10, 0, &[0x222]),
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
                },
            );
        }
        story_manager_dispatch_system(&world);
        let stages = world.try_resource::<QuestStageState>().unwrap();
        assert!(
            !stages.is_started(QuestFormId(0x222)),
            "node 30 processed without sharing — the event died there"
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
            next_sibling: 0,
            kind: SmNodeKind::Quest,
            quests: vec![
                SmQuestLink {
                    form_id: 0x111,
                    reset_hours: 48.0,
                },
                SmQuestLink {
                    form_id: 0x222,
                    reset_hours: 0.0,
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
                next_sibling: 0,
                kind: SmNodeKind::Branch,
                policies: random,
                ..Default::default()
            },
            quest_node_full(30, 20, 31, &[0x111], random),
            quest_node_full(31, 20, 0, &[0x222], random),
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
}
