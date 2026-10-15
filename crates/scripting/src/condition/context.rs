//! #5804 — `ConditionContext` + `EventDataSlots`, split out of
//! `condition.rs` (see that hub's module docs for the overview).

use std::collections::HashMap;

use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::world::World;
use byroredux_plugin::esm::records::condition::{
    event_data_slot, Condition, EventDataSlot, RunOn,
};

use super::eval::resolve_entity_by_global_form_id;
use crate::quest_stages::QuestFormId;
use crate::scene::SceneActorBindings;

/// Per-evaluation context — the abstract Run-On targets a CTDA may
/// name (`Subject` / `Target` / `CombatTarget` / …) resolved to
/// concrete entity ids by the caller. Each consumer
/// (perk dispatcher, AI package head, quest_advance system) fills
/// the slots it knows about.
///
/// `None` slots cause the evaluator to short-circuit `false` when a
/// condition's Run-On references that slot — matches Bethesda's
/// "missing reference → condition fails" contract. The exception is
/// [`RunOn::Subject`], which defaults to `subject` (always Some, since
/// every condition list runs in the context of a subject).
#[derive(Debug, Clone, Copy)]
pub struct ConditionContext<'a> {
    /// The "self" entity — quest target, dialogue speaker, magic
    /// effect caster. Always populated.
    pub subject: EntityId,
    /// The "other" entity — dialogue listener, package target, effect
    /// target. `None` when the context doesn't carry one (e.g., a
    /// quest stage condition with no specific target).
    pub target: Option<EntityId>,
    /// Subject's current combat target. `None` when not in combat or
    /// out of M47.1's scope.
    pub combat_target: Option<EntityId>,
    /// Subject's linked-reference chain head. `None` until M47.0.x
    /// linked-ref wiring lands.
    pub linked_reference: Option<EntityId>,
    /// Owning quest for `RunOn::QuestAlias` resolution. Scene evaluation
    /// supplies this; consumers outside a quest leave it `None`.
    pub quest: Option<QuestFormId>,
    /// Alias bindings resolved so far *in the refresh currently running*,
    /// consulted by `RunOn::QuestAlias` ahead of the committed
    /// [`SceneActorBindings`] table (#2671).
    ///
    /// The alias fill loop accumulates into a local map and commits it only
    /// at the end of the pass, so without this a match-CTDA referencing a
    /// *sibling* alias saw last refresh's binding — or nothing at all on the
    /// first refresh — even when the sibling had already been filled earlier
    /// in the same pass. That made cross-alias conditional fills lag one
    /// refresh and, worse, order-dependent in a way the design does not
    /// intend.
    ///
    /// `None` for every consumer outside that loop, which then reads the
    /// committed table exactly as before.
    pub pending_alias_bindings: Option<&'a HashMap<(QuestFormId, i32), EntityId>>,
    /// The Story Manager event's positional data slots, consulted by
    /// `RunOn::EventData` (#5366 Phase 2). `None` outside SM dispatch —
    /// a quest-stage or dialogue CTDA authored with run-on EventData
    /// fails exactly as before.
    pub event_data: Option<&'a EventDataSlots>,
    /// The location a location-alias fill is testing (M47.2). Location
    /// functions run on [`RunOn::Subject`] read it instead of a reference's
    /// current location — the CK's "if used as a condition to fill a Location
    /// Alias, it will test the locations themselves".
    pub location_subject: Option<u32>,
    /// Location aliases bound so far in the fill pass running, ahead of the
    /// committed [`crate::location::LocationAliasBindings`] — the location
    /// twin of [`Self::pending_alias_bindings`].
    pub pending_location_bindings: Option<&'a HashMap<(QuestFormId, i32), u32>>,
}

/// The Story Manager event's positional data slots (#5366 Phase 2) —
/// the runtime half of [`EventDataSlot`]'s wire tags. R1/R2 hold the
/// event's two references (KILL: victim, killer; CLOC: actor), L1/L2
/// its locations as authored LCTN FormIDs (CLOC: old, new).
///
/// Locations are FormIDs, not entities — the runtime has no
/// location-as-entity yet — so an L-slot tagged by a condition stays
/// unresolved (the condition fails) and an L-slot tagged by a
/// `FromEvent` alias fill stays unbound. Both are documented Phase-3+
/// deferrals, not silent drops: the resolve paths trace when they hit.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct EventDataSlots {
    pub reference_1: Option<EntityId>,
    pub reference_2: Option<EntityId>,
    pub location_1: Option<u32>,
    pub location_2: Option<u32>,
    /// `K1` — a script event's keyword (M47.2, `Keyword.SendStoryEvent`).
    pub keyword: Option<u32>,
    /// `V1` / `V2` — a script event's two integers.
    pub values: [i32; 2],
}

impl EventDataSlots {
    /// The entity an [`EventDataSlot`] tag selects, if that slot is a
    /// populated reference.
    pub fn reference(&self, slot: EventDataSlot) -> Option<EntityId> {
        match slot {
            EventDataSlot::Reference1 => self.reference_1,
            EventDataSlot::Reference2 => self.reference_2,
            EventDataSlot::Location1 | EventDataSlot::Location2 => None,
        }
    }
}

impl<'a> ConditionContext<'a> {
    /// Build a minimal context with only the subject populated. Most
    /// consumers (quest stage gates, dialogue branches) start here
    /// and add `target` per call.
    pub fn for_subject(subject: EntityId) -> Self {
        Self {
            subject,
            target: None,
            combat_target: None,
            linked_reference: None,
            quest: None,
            pending_alias_bindings: None,
            event_data: None,
            location_subject: None,
            pending_location_bindings: None,
        }
    }

    /// Run [`RunOn::Subject`] location functions on `location` itself (a
    /// location-alias fill). See [`Self::location_subject`].
    pub fn with_location_subject(mut self, location: u32) -> Self {
        self.location_subject = Some(location);
        self
    }

    /// Attach the in-progress location-alias bindings. See
    /// [`Self::pending_location_bindings`].
    pub fn with_pending_location_bindings(
        mut self,
        bindings: &'a HashMap<(QuestFormId, i32), u32>,
    ) -> Self {
        self.pending_location_bindings = Some(bindings);
        self
    }

    /// The entity filling the owning quest's reference alias `alias_id`: the
    /// in-progress pass first (#2671), then the committed table.
    pub(crate) fn alias_entity(&self, world: &World, alias_id: i32) -> Option<EntityId> {
        let quest = self.quest?;
        // #2671 — the in-progress pass wins. A sibling alias filled
        // earlier in THIS refresh is not in `SceneActorBindings`
        // yet (the fill loop commits once, at the end), so reading
        // only the committed table made cross-alias conditions see
        // stale data and self-correct a refresh later.
        if let Some(&entity) = self
            .pending_alias_bindings
            .and_then(|pending| pending.get(&(quest, alias_id)))
        {
            return Some(entity);
        }
        let bindings = world.try_resource::<SceneActorBindings>()?;
        bindings.resolve(quest, alias_id)
    }

    /// The location bound to the owning quest's location alias `alias_id`:
    /// the in-progress pass first, then the committed table.
    pub(crate) fn alias_location(&self, world: &World, alias_id: i32) -> Option<u32> {
        let quest = self.quest?;
        if let Some(&location) = self
            .pending_location_bindings
            .and_then(|pending| pending.get(&(quest, alias_id)))
        {
            return Some(location);
        }
        world
            .try_resource::<crate::location::LocationAliasBindings>()?
            .resolve(quest, alias_id)
    }

    /// Attach the in-progress alias-binding map so `RunOn::QuestAlias` can
    /// see siblings filled earlier in the same refresh (#2671). See
    /// [`Self::pending_alias_bindings`].
    pub fn with_pending_alias_bindings(
        mut self,
        bindings: &'a HashMap<(QuestFormId, i32), EntityId>,
    ) -> Self {
        self.pending_alias_bindings = Some(bindings);
        self
    }

    /// Attach an owning quest so `RunOn::QuestAlias` CTDAs can resolve the
    /// alias id carried in `Condition.extra_data_id`.
    pub fn with_quest(mut self, quest: QuestFormId) -> Self {
        self.quest = Some(quest);
        self
    }

    /// Attach the Story Manager event's data slots so `RunOn::EventData`
    /// CTDAs can resolve the R1/R2 tag in `Condition.extra_data_id`
    /// (#5366 Phase 2).
    pub fn with_event_data(mut self, event_data: &'a EventDataSlots) -> Self {
        self.event_data = Some(event_data);
        self
    }

    /// Resolve a [`RunOn`] choice to a concrete EntityId. Returns
    /// `None` when the slot isn't populated or the choice references
    /// data the evaluator doesn't yet plumb (package data, location
    /// event slots).
    pub(crate) fn resolve(&self, run_on: RunOn, condition: &Condition, world: &World) -> Option<EntityId> {
        match run_on {
            RunOn::Subject => Some(self.subject),
            RunOn::Target => self.target,
            RunOn::CombatTarget => self.combat_target,
            RunOn::LinkedReference => self.linked_reference,
            // #2123 — `condition.reference_form_id` goes through the same
            // parse-time global-form-id remap as `GetDistance`'s `param_1`
            // below; `resolve_entity_by_global_form_id` is the shared
            // resolver for that space, not a FormID-pool lookup unique to
            // this arm.
            RunOn::Reference => {
                resolve_entity_by_global_form_id(world, condition.reference_form_id)
            }
            RunOn::QuestAlias => self.alias_entity(world, condition.extra_data_id as i32),
            RunOn::PackageData => {
                log::trace!(
                    "M47.1: RunOn::PackageData (extra_data_id={:08X}) — package \
                     resolver deferred",
                    condition.extra_data_id,
                );
                None
            }
            // #5366 Phase 2 — the tag in `extra_data_id`'s low half names
            // one of the event's positional slots. R-tags resolve to the
            // event's references; L-tags name locations, which are FormIDs
            // rather than entities here, so they stay unresolved (and
            // fail the condition) until a location runtime exists.
            RunOn::EventData => {
                let slots = self.event_data?;
                match event_data_slot(condition.extra_data_id) {
                    Some(slot @ (EventDataSlot::Reference1 | EventDataSlot::Reference2)) => {
                        slots.reference(slot)
                    }
                    Some(slot @ (EventDataSlot::Location1 | EventDataSlot::Location2)) => {
                        log::trace!(
                            "#5366: RunOn::EventData location tag {:?} — no \
                             location-as-entity runtime yet, condition fails",
                            slot,
                        );
                        None
                    }
                    None => {
                        log::debug!(
                            "#5366: RunOn::EventData with unrecognized slot tag \
                             ({:08X}) — condition fails",
                            condition.extra_data_id,
                        );
                        None
                    }
                }
            }
        }
    }
}
