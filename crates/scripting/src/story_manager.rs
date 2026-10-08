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
//! Phase-1 semantics decisions, both revisited when `DNAM` decodes
//! (#5366 §5 alignment pass):
//!
//! - **Traversal continues past a firing node.** The CK tutorial
//!   ("Bethesda Tutorial Story Manager", local reference wiki) states
//!   the "Shares Event" flag is the intended default in almost all
//!   circumstances, so until the `DNAM` bit that encodes it is verified
//!   against the corpus, continuing is the conservative majority
//!   behavior. The stop-on-fire variant is one flag read away.
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
use byroredux_plugin::esm::records::{SmNodeKind, SmNodeRecord};
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
    /// `SMQN.NNAM` quest links in authored order.
    pub quest_links: Vec<u32>,
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
            quest_links: record.quest_links.clone(),
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
    let candidates = {
        let Some(tree) = world.try_resource::<SmTree>() else {
            return;
        };
        let registry = world.try_resource::<QuestDefinitionRegistry>();
        let stages = world.try_resource::<QuestStageState>();
        let mut candidates = Vec::new();
        for event in &events {
            dispatch_story_event(
                world,
                &tree,
                &mut WalkState {
                    event,
                    slots: event.slots(),
                    registry: registry.as_deref(),
                    stages: stages.as_deref(),
                    candidates: &mut candidates,
                },
            );
        }
        candidates
    };
    let Some(mut stages) = world.try_resource_mut::<QuestStageState>() else {
        return;
    };
    for candidate in candidates {
        if stages.is_started(candidate.quest) {
            continue;
        }
        stages.start_quest(candidate.quest, candidate.start_up_stage);
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
    walk_siblings(world, tree, state, tree.nodes[root].first_child, &mut visited);
}

/// Evaluate a sibling chain in order. A node that passes collects its
/// quest links (Quest kind) and descends into its children
/// (Branch/Event); one that fails skips its whole subtree. Traversal
/// then continues with the next sibling regardless — the shares-event
/// default documented on the module.
fn walk_siblings(
    world: &World,
    tree: &SmTree,
    state: &mut WalkState<'_>,
    head: Option<usize>,
    visited: &mut [bool],
) {
    let mut current = head;
    while let Some(index) = current {
        if visited[index] {
            break;
        }
        visited[index] = true;
        let node = &tree.nodes[index];
        let continuation = node.next_sibling;
        // Subject = the doer: R2 when the event carries one (KILL
        // killer), else R1 (CLOC actor). Target = R1 (KILL victim).
        // `RunOn::EventData` tags resolve through the same slots.
        let mut context = ConditionContext::for_subject(
            state.event.reference_2.unwrap_or(state.event.reference_1),
        )
        .with_event_data(&state.slots);
        context.target = Some(state.event.reference_1);
        if evaluate(&node.conditions, world, &context) {
            if node.kind == SmNodeKind::Quest {
                for &quest in &node.quest_links {
                    let quest = QuestFormId(quest);
                    if state.stages.is_some_and(|stages| stages.is_started(quest)) {
                        continue;
                    }
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
                    });
                }
            }
            walk_siblings(world, tree, state, node.first_child, visited);
        }
        current = continuation;
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
        SmNodeRecord {
            form_id,
            parent,
            next_sibling,
            kind: SmNodeKind::Quest,
            quest_links: quests.to_vec(),
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
        // Both members of the cycle still fired exactly once.
        assert!(stages.is_started(QuestFormId(0x222)));
        assert!(stages.is_started(QuestFormId(0x333)));
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
            // Shares-event default: BOTH sibling quest nodes fired.
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
        quest_links: vec![0x555],
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
        quest_links: vec![0x556],
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
}
