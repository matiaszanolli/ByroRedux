//! Story Manager dispatch runtime — #5366 Phase 1.
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
//! Phase-1 semantics decisions, both revisited when `DNAM` decodes
//! (#5366 §5 alignment pass):
//!
//! - **Traversal continues past a firing node.** The CK tutorial
//!   ("Bethesda Tutorial Story Manager", local reference wiki) states
//!   the "Shares Event" flag is the intended default in almost all
//!   circumstances, so until the `DNAM` bit that encodes it is verified
//!   against the corpus, continuing is the conservative majority
//!   behavior. The stop-on-fire variant is one flag read away.
//! - **Condition run-on is Subject/Object only.** The event's subject
//!   slots into `ConditionContext::subject` (killer, entering actor…)
//!   and its object into `target` (victim…). `RunOn::EventData` still
//!   resolves `None` (condition fails) — the per-mnemonic event-data
//!   alias table is Phase 2.
//!
//! Producers: the engine wires two — `KILL` at the combat death
//! transition (`byroredux/src/combat.rs`) and `CLOC` through
//! [`emit_change_location_on_key_change`], fed the session's
//! cell/worldspace identity each frame by an engine-side system.
//! Papyrus `SendStoryEvent` lowers onto the same marker when its
//! producer lands.

use byroredux_core::ecs::resource::Resource;
use byroredux_core::ecs::sparse_set::SparseSetStorage;
use byroredux_core::ecs::storage::{Component, EntityId};
use byroredux_core::ecs::world::World;
use byroredux_plugin::esm::records::condition::ConditionList;
use byroredux_plugin::esm::records::{SmNodeKind, SmNodeRecord};
use std::collections::HashMap;

use crate::condition::{evaluate, ConditionContext};
use crate::quest_stages::{QuestDefinitionRegistry, QuestFormId, QuestStageState};

/// One raised story event — the ECS replacement for the engine's story
/// event queue entry. Transient marker (**Pattern B**, #2672): it has
/// exactly one owning consumer, `story_manager_dispatch_system`, which
/// snapshots and drains it at its head. Producers attach it to the
/// entity the event most concerns (the slain actor for `KILL`, the
/// entering actor for `CLOC`).
#[derive(Debug, Clone, Copy)]
pub struct StoryEvent {
    /// 4-byte event mnemonic (`"KILL"`, `"CLOC"`, …) matching the
    /// `SMEN.ENAM` catalog parsed off the master.
    pub mnemonic: [u8; 4],
    /// Event-data slot 1: killer, caster, entering actor. Conditions
    /// with `RunOn::Subject` evaluate against this entity.
    pub subject: EntityId,
    /// Event-data slot 2: victim, target. `RunOn::Target` evaluates
    /// against this entity; `None` fails such conditions.
    pub object: Option<EntityId>,
    /// Location FormID (LCTN/CELL) when the event carries one.
    pub location: Option<u32>,
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

/// Last location key the CLOC producer observed. `NOT_SAVED_BY_DESIGN`:
/// after a load the first post-load frame fires a fresh CLOC, which is
/// the engine behavior the event describes anyway (you changed location
/// by loading).
#[derive(Default)]
pub struct StoryLocationCursor(pub Option<u64>);

impl Resource for StoryLocationCursor {}

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
        world.insert_resource(StoryLocationCursor(None));
    }
    count
}

/// Engine-side CLOC producer half: fire a `CLOC` StoryEvent on `subject`
/// exactly when the session's location `key` differs from the cursor's.
///
/// `key` is an opaque process-local identity (the engine hashes the
/// cell editor-id / worldspace+grid context); `None` means "no location
/// context" (loose-NIF mode, mid-transition window) — the cursor
/// follows along but no event fires for it.
pub fn emit_change_location_on_key_change(
    world: &World,
    key: Option<u64>,
    subject: EntityId,
    location: Option<u32>,
) -> bool {
    // The install pass always creates the cursor alongside the tree; a
    // world without one has no Story Manager installed at all.
    let Some(mut cursor) = world.try_resource_mut::<StoryLocationCursor>() else {
        return false;
    };
    if cursor.0 == key {
        return false;
    }
    cursor.0 = key;
    // Release the resource write before touching the marker storage so
    // this path never nests two acquisitions.
    drop(cursor);
    if key.is_none() {
        return false;
    }
    if let Some(mut events) = world.query_mut::<StoryEvent>() {
        events.insert(
            subject,
            StoryEvent {
                mnemonic: *b"CLOC",
                subject,
                object: None,
                location,
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
    let Some(tree) = world.try_resource::<SmTree>() else {
        return;
    };
    let Some(mut stages) = world.try_resource_mut::<QuestStageState>() else {
        return;
    };
    for event in &events {
        dispatch_story_event(world, &tree, &mut stages, event);
    }
}

fn dispatch_story_event(
    world: &World,
    tree: &SmTree,
    stages: &mut QuestStageState,
    event: &StoryEvent,
) {
    let Some(&root) = tree.roots_by_mnemonic.get(&event.mnemonic) else {
        return;
    };
    let registry = world.try_resource::<QuestDefinitionRegistry>();
    // One visited bitmap for the whole event walk: a node is evaluated
    // at most once per event, which bounds malformed sibling cycles and
    // makes re-reachable nodes cheap no-ops.
    let mut visited = vec![false; tree.nodes.len()];
    walk_siblings(
        world,
        tree,
        stages,
        event,
        registry.as_deref(),
        tree.nodes[root].first_child,
        &mut visited,
    );
}

/// Evaluate a sibling chain in order. A node that passes runs its quest
/// links (Quest kind) and descends into its children (Branch/Event);
/// one that fails skips its whole subtree. Traversal then continues
/// with the next sibling regardless — the shares-event default
/// documented on the module.
fn walk_siblings(
    world: &World,
    tree: &SmTree,
    stages: &mut QuestStageState,
    event: &StoryEvent,
    registry: Option<&QuestDefinitionRegistry>,
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
        let mut context = ConditionContext::for_subject(event.subject);
        context.target = event.object;
        if evaluate(&node.conditions, world, &context) {
            if node.kind == SmNodeKind::Quest {
                for &quest in &node.quest_links {
                    let quest = QuestFormId(quest);
                    if stages.is_started(quest) {
                        continue;
                    }
                    let start_up =
                        registry.and_then(|registry| registry.start_up_stage(quest));
                    stages.start_quest(quest, start_up);
                    log::info!(
                        "#5366 story manager: started quest {:#010X} ('{}') via node '{}' \
                         on '{}' event",
                        quest.0,
                        registry
                            .and_then(|registry| registry.editor_id(quest))
                            .unwrap_or("?"),
                        node.editor_id,
                        String::from_utf8_lossy(&event.mnemonic),
                    );
                }
            }
            walk_siblings(
                world,
                tree,
                stages,
                event,
                registry,
                node.first_child,
                visited,
            );
        }
        current = continuation;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
                    subject: actor,
                    object: None,
                    location: None,
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
                        subject: killer,
                        object: Some(victim),
                        location: None,
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
                    subject: killer,
                    object: None,
                    location: None,
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
                    subject: actor,
                    object: None,
                    location: None,
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
        world.insert_resource(StoryLocationCursor(None));
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
}
