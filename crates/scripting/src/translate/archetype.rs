//! Archetype recognition — the shared types every recognizer uses.
//!
//! A *recognizer* is a free function `fn(&RecognizeCtx) -> Option<Recognized>`
//! that inspects a [`ScriptSource`] (plus its per-instance VMAD-bound
//! properties, once that phase lands) and either declines or extracts a
//! canonical spawn. This is the R5-validated pattern — "detect the shape,
//! extract the constants, populate the component":
//!
//! - **Generic** recognizers match a behavior *family* (e.g. quest-stage
//!   gate) and carry the script's constants as data into one reusable
//!   component + dispatch system. One recognizer covers many scripts.
//! - **Per-script** recognizers match a single named script's signature
//!   and emit its bespoke component — the long tail where the
//!   generalization stops paying for itself.
//!
//! Recognizers are chained in priority order by
//! [`translate_script`](super::translate_script); the first match wins.

use byroredux_core::ecs::{storage::EntityId, world::World};
use byroredux_plugin::esm::reader::GameKind;
use byroredux_plugin::esm::records::script_instance::ScriptInstanceData;

use super::source::ScriptSource;

/// Inputs available to a recognizer: the parsed source plus the two
/// per-instance binding sources a recognizer resolves object/quest refs
/// from — the VMAD-decoded properties and, for alias-attached scripts,
/// the owning quest.
pub struct RecognizeCtx<'a> {
    /// The script in one of its source dialects.
    pub source: &'a ScriptSource<'a>,
    /// The game variant — for recognizers whose shape or constants are
    /// game-conditioned. Per-game *decisions* must consult the
    /// [`super::tables`], not branch ad-hoc.
    pub game: GameKind,
    /// The per-instance VMAD-decoded script attachments + property
    /// bindings for this reference (`None` when the source carries no
    /// VMAD, or before the attach-time wiring supplies it). Recognizers
    /// bind a `Quest Property` / `ObjectReference Property` ref by
    /// looking the property up here.
    pub script_instance: Option<&'a ScriptInstanceData>,
    /// The quest that owns this script's alias, when the script is
    /// attached via a quest `ReferenceAlias` (Papyrus
    /// `Self.GetOwningQuest()`). Resolved by the attach context — it is
    /// the alias→quest ownership, NOT derivable from the AST or a
    /// property. `None` for object/REFR-attached scripts.
    pub owning_quest: Option<u32>,
}

/// The output of a successful recognition: a name (for diagnostics) and a
/// closure that inserts the canonical component(s) onto the script-bearing
/// entity. A boxed closure (not a bare `fn` like [`crate::ScriptSpawnFn`])
/// because recognizers capture the constants they extracted.
pub struct Recognized {
    /// `archetype@editor_id`, surfaced in logs + the `script.*` console.
    pub archetype: String,
    /// Inserts the canonical components onto `entity`. Called once at
    /// REFR spawn / script attach.
    pub spawn: Box<dyn Fn(&mut World, EntityId) + Send + Sync>,
}

impl Recognized {
    /// Convenience constructor.
    pub fn new(
        archetype: impl Into<String>,
        spawn: impl Fn(&mut World, EntityId) + Send + Sync + 'static,
    ) -> Self {
        Self {
            archetype: archetype.into(),
            spawn: Box::new(spawn),
        }
    }
}

/// A recognizer function: declines (`None`) or extracts a canonical spawn.
pub type Recognizer = fn(&RecognizeCtx<'_>) -> Option<Recognized>;
