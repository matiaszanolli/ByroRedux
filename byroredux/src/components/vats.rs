//! VATS mode state (#6350) — the engine-side targeting-mode resource,
//! mirroring `GetVATSMode`'s five engine states, and the wall-clock
//! escape hatch the overlay/camera run on while the world is frozen.
//!
//! Split from the tuning model ([`byroredux_core::character::VatsModel`],
//! which is *data*: what VATS costs and how chances compute) the way
//! [`crate::components::GameTimeRes`] splits from the weather system:
//! this module is *state* — which mode the engine is in, what the scan
//! sees, what the player queued.

use byroredux_core::ecs::resource::Resource;
use byroredux_core::ecs::EntityId;

/// The engine states `GetVATSMode` exposes to scripts (GECK wiki,
/// function index 0 = none through 4 = playback). The numeric values
/// ARE the script-facing contract — keep them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u8)]
// `MenuMode`/`ScanMode`/`Playback` exist because `GetVATSMode`'s contract
// exposes all five states; their constructors are the Phase-3c scan/queue
// and Phase-4 playback lanes.
#[allow(dead_code, reason = "GetVATSMode contract surface; constructed by the 3c/Phase-4 lanes")]
pub(crate) enum VatsMode {
    #[default]
    None = 0,
    TargetSelect = 1,
    MenuMode = 2,
    ScanMode = 3,
    Playback = 4,
}

impl VatsMode {
    /// The integer `GetVATSMode` returns.
    #[allow(dead_code, reason = "consumed by the GetVATSMode condition function / ObScript native (3d)")]
    pub(crate) fn as_u32(self) -> u32 {
        self as u8 as u32
    }
}


/// One body part of one target, as the targeting scan sees it (Phase 3c
/// fills the list; the overlay and `vats.status` read it).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct VatsTargetPart {
    pub entity: EntityId,
    /// Index into the resolved BPTD part array of the target's body
    /// data.
    pub part_index: usize,
    /// `BPTN` — display name ("Head", "Left Legs").
    pub part_name: String,
    /// Computed to-hit chance, percent.
    pub hit_chance: f32,
    /// The part's AP cost through the equipped weapon (weapon-authored
    /// cost, or the per-anim-type standard).
    pub ap_cost: f32,
}

/// VATS mode state — a `Resource`, transient by design (never saved:
/// the mode dies with the session, like vanilla; the AP pool persists
/// through `ActorValues`).
#[derive(Debug, Default)]
pub(crate) struct VatsState {
    pub mode: VatsMode,
    /// The last targeting scan's flattened results (targets × parts).
    pub targets: Vec<VatsTargetPart>,
    /// Attacks the player queued and confirmed; executed on playback.
    pub queue: Vec<VatsTargetPart>,
    /// AP reserved by the queue (charged on confirm; cancelled queues
    /// drop the reservation).
    pub ap_reserved: f32,
    /// Session-deterministic LCG state for the execution die rolls —
    /// gates and tests rely on replayable outcomes, not wall-clock rng.
    pub roll_state: u32,
}

impl Resource for VatsState {}

impl VatsState {
    /// The world time scale this state publishes: FO3/FNV freeze the
    /// world solid while any VATS mode is active (the FO4 slow-mo
    /// dialect is a follow-up lane and would publish its own scale).
    pub(crate) fn world_time_scale(&self) -> f32 {
        if self.mode == VatsMode::None {
            1.0
        } else {
            0.0
        }
    }

    /// Leave every mode, refunding any un-executed AP reservation.
    /// `refund` applies the refund to the player's AP pool.
    pub(crate) fn exit_all(&mut self) {
        self.mode = VatsMode::None;
        self.targets.clear();
        self.queue.clear();
        // The reservation is dropped rather than refunded here: the
        // caller owns the ActorValues write (and on execution the spend
        // is already committed).
        self.ap_reserved = 0.0;
    }
}

/// #6350 — the wall-clock escape hatch while VATS freezes the world:
/// carries the frame's UN-scaled dt so the overlay and camera keep
/// animating (the `LoadingCoverClock` pattern, `components/render.rs`).
#[derive(Debug, Default)]
pub(crate) struct VatsClock(pub f32);

impl Resource for VatsClock {}
