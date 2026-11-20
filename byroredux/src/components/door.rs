//! In-cell door runtime data (PRD door-runtime, M66).
//!
//! The open and closed state is the existing [`TwoStateActivator`](byroredux_scripting::TwoStateActivator) (DRT-G-005);
//! this component carries what the record and the model author and the motion's bookkeeping, and no state of its own
//! beyond `posed`, which remembers which state the door's pose shows.

use byroredux_core::ecs::sparse_set::SparseSetStorage;
use byroredux_core::ecs::storage::Component;
use byroredux_plugin::esm::cell::{DoorData, DoorFlags};

/// How a door model moves between its two states (design §3). Only the authored-sequence class exists so far.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DoorMotion {
    /// The model's `Open` and `Close` `NiControllerSequence` clips, as registry handles.
    Sequence { open: u32, close: u32 },
}

/// A placed in-cell door.
#[derive(Debug, Clone)]
pub(crate) struct Door {
    pub(crate) motion: DoorMotion,
    /// The base record's flag byte and sounds (DRT-REC-001, -002).
    pub(crate) data: DoorData,
    /// The flags, translated for the game the record came from.
    #[expect(dead_code, reason = "door motion and activation flag consumers are tracked in #7059")]
    pub(crate) flags: DoorFlags,
    /// `Open by Default` on the placement (DRT-REC-004): the state the door returns to.
    #[expect(dead_code, reason = "door reset and restore are tracked in #7059")]
    pub(crate) default_open: bool,
    /// Seconds of the playing clip still to run; `0` when the door is at rest.
    pub(crate) remaining: f32,
    /// The state the door's pose shows, `None` until the first tick sets it. A mismatch with the door state while at
    /// rest (a state restored from a save, a script that set it) snaps the pose rather than animating it.
    pub(crate) posed: Option<bool>,
}

impl Component for Door {
    type Storage = SparseSetStorage<Self>;
}
