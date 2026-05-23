//! Script event marker components.
//!
//! Events are transient components: added when something happens,
//! processed by script systems during the frame, then removed by
//! the cleanup system at the end of the frame.
//!
//! This is the ECS replacement for Papyrus's event queue. Instead of
//! enqueueing events in a VM dispatcher (which adds latency), events
//! are immediate component mutations visible to all systems in the
//! same frame.

use byroredux_core::ecs::sparse_set::SparseSetStorage;
use byroredux_core::ecs::storage::{Component, EntityId};
use byroredux_core::string::FixedString;

/// Fired when an entity is activated by another entity (e.g., player uses a door).
/// Replaces Papyrus `OnActivate`.
#[derive(Debug, Clone, Copy)]
pub struct ActivateEvent {
    pub activator: EntityId,
}

impl Component for ActivateEvent {
    type Storage = SparseSetStorage<Self>;
}

/// Fired when an entity is hit in combat.
/// Replaces Papyrus `OnHit`.
#[derive(Debug, Clone, Copy)]
pub struct HitEvent {
    pub aggressor: EntityId,
    pub source: EntityId,
    pub projectile: EntityId,
    pub power_attack: bool,
    pub sneak_attack: bool,
    pub bash_attack: bool,
    pub blocked: bool,
}

impl Component for HitEvent {
    type Storage = SparseSetStorage<Self>;
}

/// Fired when a timer expires. Added by the timer tick system.
/// Replaces Papyrus `OnTimer`.
#[derive(Debug, Clone, Copy)]
pub struct TimerExpired {
    pub timer_id: u32,
}

impl Component for TimerExpired {
    type Storage = SparseSetStorage<Self>;
}

/// A single text key event crossed during animation playback.
///
/// `label` is an interned `FixedString` (#231 / SI-04) — resolve via
/// `world.resource::<StringPool>().resolve(event.label)` to recover
/// the original `&str`. Carrying the symbol instead of an owned
/// `String` removes the per-fire allocation in
/// `byroredux::systems::animation_system`.
#[derive(Debug, Clone, Copy)]
pub struct AnimationTextKeyEvent {
    /// The text key label from the NIF (e.g., "hit", "sound: wpn_swing").
    pub label: FixedString,
    /// The clip time at which this event was defined.
    pub time: f32,
}

/// Fired when animation text keys are crossed during playback.
///
/// Text keys are timed markers in .kf files (e.g., "hit", "sound: wpn_swing",
/// "FootLeft", "FootRight", "start", "end"). They fire each time the
/// animation's local time crosses the key's timestamp, including on loop.
/// Multiple keys can fire in a single frame, so this holds a Vec.
///
/// Systems can query for this component to trigger sounds, hit detection,
/// footstep effects, or state transitions.
#[derive(Debug, Clone)]
pub struct AnimationTextKeyEvents(pub Vec<AnimationTextKeyEvent>);

impl Component for AnimationTextKeyEvents {
    type Storage = SparseSetStorage<Self>;
}

/// M47.0 Phase 5 — fired when an entity enters a trigger volume.
/// Replaces Papyrus `OnTriggerEnter` (Skyrim+) / `OnTrigger` (FO3/FNV).
///
/// The marker lands on the TRIGGER VOLUME entity (the activator with
/// `XPRM` primitive bounds and no MODL), not the entering entity.
/// Papyrus's `akActionRef` parameter is captured here as `triggerer`.
///
/// Emit site: Rapier sensor-collision callback (Phase 5 follow-up
/// when Rapier sensor support lands in `byroredux_physics`). For now
/// this marker is structurally available — scripts can declare
/// `query::<OnTriggerEnterEvent>()` and the storage is registered —
/// but no emit site means it never fires from the engine itself.
/// Tests can synthesize via `world.query_mut::<OnTriggerEnterEvent>()
/// .insert(trigger_entity, OnTriggerEnterEvent { triggerer })`.
#[derive(Debug, Clone, Copy)]
pub struct OnTriggerEnterEvent {
    /// The entity that crossed into the trigger volume — Papyrus's
    /// `akActionRef` parameter. Typically the player; could be an
    /// NPC if the trigger covers a patrol path.
    pub triggerer: EntityId,
}

impl Component for OnTriggerEnterEvent {
    type Storage = SparseSetStorage<Self>;
}

/// M47.0 Phase 5 — fired when an entity is spawned into a cell that
/// just loaded. Replaces Papyrus `OnCellLoad` (the script-attached
/// entity's first-tick initialization hook).
///
/// Lifecycle: emitted by the cell loader on every newly-spawned REFR
/// that carries a script. Drained by `event_cleanup_system` at end-
/// of-frame so each script sees exactly one `OnCellLoad` invocation.
///
/// Distinct from `ActivateEvent`: `OnCellLoad` fires unconditionally
/// at spawn time regardless of player action, whereas `ActivateEvent`
/// fires on explicit use-key interaction.
///
/// **Emit site status (Phase 5)**: defined here; the cell-loader emit
/// site fires from `attach_script_for_refr` in `byroredux/src/
/// cell_loader/references.rs` after the script's state component
/// lands. Script systems see `OnCellLoadEvent` on the very same frame
/// the REFR spawned — equivalent to Papyrus's `OnLoad` semantics.
#[derive(Debug, Clone, Copy)]
pub struct OnCellLoadEvent;

impl Component for OnCellLoadEvent {
    type Storage = SparseSetStorage<Self>;
}

/// M47.0 Phase 5 — fired when an actor equips an item. Replaces
/// Papyrus `OnEquip` (Skyrim+) / `OnEquipped` (FO3/FNV).
///
/// Lands on the EQUIPPED ITEM entity (when items are instance
/// entities — M41 Phase 2's ItemInstance path) or on the WEARER
/// (when the wearer-side hook is preferred — common Papyrus idiom).
/// The Bethesda Papyrus contract attaches the script to the item
/// extends; mirror that here by emitting on the item.
///
/// **Emit site status (Phase 5)**: defined here; the M41 equip
/// pipeline emit site lands in a follow-up commit (touches
/// `byroredux/src/cell_loader.rs::build_npc_equip_state` + the
/// per-NPC outfit-resolve walk). The marker is structurally
/// queryable now; scripts can declare the storage and the engine
/// will start firing it once the M41 hook lands.
#[derive(Debug, Clone, Copy)]
pub struct OnEquipEvent {
    /// The wearer that just equipped this item — Papyrus's
    /// `akActor` parameter. Typically the NPC actor; the player on
    /// first-person equips.
    pub wearer: EntityId,
}

impl Component for OnEquipEvent {
    type Storage = SparseSetStorage<Self>;
}
