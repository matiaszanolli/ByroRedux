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
    /// Damage the producer resolved for this hit, before the target's own
    /// defenses (see [`Self::blocked`]).
    ///
    /// #2980 — carried on the event rather than recomputed at consumption.
    /// Papyrus's `OnHit` has no such parameter because its VM could always
    /// re-interrogate the aggressor's equipment; here the producer is the
    /// only party that knows what the hit was worth. A scripted producer has
    /// no `EquippedWeapon` to recompute from at all, and a consumer that
    /// recomputed would silently diverge the moment anything wrote
    /// `EquippedWeapon` between production and consumption.
    pub damage: f32,
    pub power_attack: bool,
    pub sneak_attack: bool,
    pub bash_attack: bool,
    pub blocked: bool,
}

impl Component for HitEvent {
    type Storage = SparseSetStorage<Self>;
}

/// Fired when an actor enters or re-enters a water surface. The event lands
/// on the water-plane entity so audio, gameplay and presentation systems can
/// consume the same source interaction without inventing a second queue.
#[derive(Debug, Clone, Copy)]
pub struct SplashEvent {
    pub actor: EntityId,
    pub intensity: f32,
    pub position: [f32; 3],
}

impl Component for SplashEvent {
    type Storage = SparseSetStorage<Self>;
}

/// One-frame surface disturbance marker while an actor is interacting with a
/// water plane. Unlike [`SplashEvent`], this may recur while the actor remains
/// near the surface and is suitable for ripples or looping surface audio.
#[derive(Debug, Clone, Copy)]
pub struct RippleEvent {
    pub actor: EntityId,
    pub intensity: f32,
    pub position: [f32; 3],
}

impl Component for RippleEvent {
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
/// Emit site: `trigger_detection_system` (M47.2) fires this on the
/// volume entity when the player crosses into it. Drained by
/// `event_cleanup_system` at end-of-frame so each crossing is seen for
/// exactly one frame (without the drain a re-evaluating consumer such as
/// `quest_advance_system` would re-fire every frame).
/// Tests can synthesize via `world.query_mut::<OnTriggerEnterEvent>()
/// .insert(trigger_entity, OnTriggerEnterEvent { triggerers: vec![triggerer] })`.
#[derive(Debug, Clone)]
pub struct OnTriggerEnterEvent {
    /// Every entity that crossed into the trigger volume this frame — one
    /// Papyrus `akActionRef` delivery per entry. Convoys can move several
    /// attached actors across the same volume in a single tick.
    pub triggerers: Vec<EntityId>,
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

/// One real equipment-state transition on a wearer.
///
/// Inventory items are stack rows keyed by authored FormID, not fabricated ECS
/// entities. `equipped=false` covers explicit unequips and a prior item that
/// was fully displaced by a new equip.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct EquipmentChange {
    pub item_form_id: u32,
    pub equipped: bool,
}

/// One-frame ordered batch of equipment transitions on the wearer entity.
///
/// Batching prevents multiple changes in one frame—such as an old weapon
/// unequip followed by a new weapon equip—from overwriting one another in a
/// sparse component slot.
#[derive(Debug, Clone, Default, Eq, PartialEq)]
pub struct EquipmentEventBatch(pub Vec<EquipmentChange>);

impl Component for EquipmentEventBatch {
    type Storage = SparseSetStorage<Self>;
}
