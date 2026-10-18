//! Canonical player/actor control state written by Papyrus fragments.
//!
//! Skyrim's startup scripts selectively disable control domains, restrain the
//! player actor, and hand movement to AI. Keeping those as explicit ECS state
//! lets the app input/controller layer consume the same state without hosting
//! Papyrus-specific logic.

use byroredux_core::ecs::resource::Resource;
use byroredux_core::ecs::sparse_set::SparseSetStorage;
use byroredux_core::ecs::storage::{Component, EntityId};
use byroredux_core::ecs::world::World;

/// Control domains selected by `EnablePlayerControls` /
/// `DisablePlayerControls`. A selected domain is assigned the call's enabled
/// value; an unselected domain is left untouched.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub struct PlayerControlSelection {
    pub movement: bool,
    pub fighting: bool,
    pub camera_switching: bool,
    pub looking: bool,
    pub sneaking: bool,
    pub menu: bool,
    pub activation: bool,
    pub journal_tabs: bool,
    pub pov_type: i32,
}

impl PlayerControlSelection {
    /// `Game.EnablePlayerControls` defaults (CK wiki): every domain true.
    /// The two global functions do NOT share a default table —
    /// `DisablePlayerControls` defaults camera/looking/sneaking and the
    /// journal tabs to false (#5651). Compiled calls fill absent
    /// arguments from their own function's table, so the single shared
    /// `PAPYRUS_DEFAULT` this replaced (camera/looking/sneaking from
    /// Enable, journal tabs from Disable) matched neither signature.
    pub const ENABLE_PLAYER_CONTROLS_DEFAULT: Self = Self {
        movement: true,
        fighting: true,
        camera_switching: true,
        looking: true,
        sneaking: true,
        menu: true,
        activation: true,
        journal_tabs: true,
        pov_type: 0,
    };

    /// `Game.DisablePlayerControls` defaults (CK wiki): camera switching,
    /// looking, sneaking and the journal tabs false, everything else true.
    pub const DISABLE_PLAYER_CONTROLS_DEFAULT: Self = Self {
        movement: true,
        fighting: true,
        camera_switching: false,
        looking: false,
        sneaking: false,
        menu: true,
        activation: true,
        journal_tabs: false,
        pov_type: 0,
    };
}

/// Engine-facing state of every player input domain plus cinematic modes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub struct PlayerControlState {
    pub movement_enabled: bool,
    pub fighting_enabled: bool,
    pub camera_switching_enabled: bool,
    pub looking_enabled: bool,
    pub sneaking_enabled: bool,
    pub menu_enabled: bool,
    pub activation_enabled: bool,
    pub journal_tabs_enabled: bool,
    pub disabled_pov_type: i32,
    pub ai_driven: bool,
    pub hud_cart_mode: bool,
}

impl Default for PlayerControlState {
    fn default() -> Self {
        Self {
            movement_enabled: true,
            fighting_enabled: true,
            camera_switching_enabled: true,
            looking_enabled: true,
            sneaking_enabled: true,
            menu_enabled: true,
            activation_enabled: true,
            journal_tabs_enabled: true,
            disabled_pov_type: 0,
            ai_driven: false,
            hud_cart_mode: false,
        }
    }
}

impl Resource for PlayerControlState {}

impl PlayerControlState {
    pub fn apply_selection(&mut self, enabled: bool, selection: PlayerControlSelection) {
        if selection.movement {
            self.movement_enabled = enabled;
        }
        if selection.fighting {
            self.fighting_enabled = enabled;
        }
        if selection.camera_switching {
            self.camera_switching_enabled = enabled;
        }
        if selection.looking {
            self.looking_enabled = enabled;
        }
        if selection.sneaking {
            self.sneaking_enabled = enabled;
        }
        if selection.menu {
            self.menu_enabled = enabled;
        }
        if selection.activation {
            self.activation_enabled = enabled;
        }
        if selection.journal_tabs {
            self.journal_tabs_enabled = enabled;
        }
        self.disabled_pov_type = selection.pov_type;
    }
}

/// Per-actor state written by `Actor.SetRestrained`, `Actor.SetUnconscious`
/// and `Actor.SetGhost`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub struct ActorControlState {
    pub restrained: bool,
    /// #5017 — `Actor.SetUnconscious`, and FO4+'s ACHR "Starts Unconscious".
    /// Per the Creation Kit (`SetUnconscious - Actor`), an unconscious actor
    /// cannot move or "think" (pick packages, yell alarms), cannot be talked
    /// to or pickpocketed, and does not react to what it detects. It does not
    /// turn hostile when attacked, but does once conscious again. Mutually
    /// exclusive with [`Self::restrained`]: see [`Self::set_unconscious`].
    pub unconscious: bool,
    /// M47.2 — `Actor.SetGhost`. The CS / GECK *SetGhost* pages: a ghost
    /// "will not enter combat", "cannot be harmed by a weapon or spell in any
    /// way" and "will not be affected by any new magic effects", but "can
    /// inflict damage", "will be hurt by water damage and some traps", and
    /// "is still able to talk".
    pub ghost: bool,
}

impl ActorControlState {
    /// Flag or clear restrained. Flagging it clears unconscious: the
    /// Creation Kit states the two cannot hold at once.
    pub fn set_restrained(&mut self, restrained: bool) {
        self.restrained = restrained;
        if restrained {
            self.unconscious = false;
        }
    }

    /// Flag or clear unconscious. Flagging it clears restrained.
    pub fn set_unconscious(&mut self, unconscious: bool) {
        self.unconscious = unconscious;
        if unconscious {
            self.restrained = false;
        }
    }
}

impl Component for ActorControlState {
    type Storage = SparseSetStorage<Self>;
}

/// An actor the engine has taken out of the AI for now: down after a `PushActorAway`, or running from
/// a `ForceFlee`. Like an unconscious one it neither walks a package nor starts a fight. Present only
/// while the system that put it there owns the actor. `NOT_SAVED_BY_DESIGN`: transient.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AiSuspended;

impl Component for AiSuspended {
    type Storage = SparseSetStorage<Self>;
}

/// Whether `entity` is unconscious (#5017). No `ActorControlState` means
/// conscious. An actor in bleedout counts (M47.2): the CK *Kill* page calls
/// bleedout an essential actor's "temporary 'bleedout' state" of being
/// unconscious.
pub fn is_unconscious(world: &World, entity: EntityId) -> bool {
    world
        .get::<ActorControlState>(entity)
        .is_some_and(|state| state.unconscious)
        || crate::bleedout::is_bleeding_out(world, entity)
}

/// Whether `entity` is a ghost (M47.2, `SetGhost`).
pub fn is_ghost(world: &World, entity: EntityId) -> bool {
    world
        .get::<ActorControlState>(entity)
        .is_some_and(|state| state.ghost)
}

/// Every ghost, collected before a per-frame system takes its own queries
/// (the [`collect_unconscious`] discipline).
pub fn collect_ghosts(world: &World, out: &mut Vec<EntityId>) {
    out.clear();
    if let Some(states) = world.query::<ActorControlState>() {
        out.extend(
            states
                .iter()
                .filter(|(_, state)| state.ghost)
                .map(|(entity, _)| entity),
        );
    }
}

/// Refill `out` with every unconscious entity (bleedout included), taking
/// one guard at a time and dropping each before returning (#5017). Per-frame
/// systems collect this first, so they never take the guards while holding
/// their own queries.
pub fn collect_unconscious(world: &World, out: &mut Vec<EntityId>) {
    out.clear();
    if let Some(states) = world.query::<ActorControlState>() {
        out.extend(
            states
                .iter()
                .filter(|(_, state)| state.unconscious)
                .map(|(entity, _)| entity),
        );
    }
    if let Some(held) = world.query::<AiSuspended>() {
        out.extend(held.iter().map(|(entity, _)| entity).filter(|entity| !out.contains(entity)));
    }
    // `SetActorsAI 0`: "turns off and on the processing of an actor" (GECK).
    for entity in crate::obscript_more::ai_disabled_actors(world) {
        if !out.contains(&entity) {
            out.push(entity);
        }
    }
    if let Some(bleedouts) = world.query::<crate::bleedout::Bleedout>() {
        out.extend(
            bleedouts
                .iter()
                .map(|(entity, _)| entity)
                .filter(|entity| !out.contains(entity))
                .collect::<Vec<_>>(),
        );
    }
}

/// Apply `f` to `entity`'s `ActorControlState`, inserting a default one
/// first. `false` when the storage is not registered.
pub fn update_actor_control(
    world: &World,
    entity: EntityId,
    f: impl FnOnce(&mut ActorControlState),
) -> bool {
    let Some(mut states) = world.query_mut::<ActorControlState>() else {
        return false;
    };
    match states.get_mut(entity) {
        Some(state) => f(state),
        None => {
            let mut state = ActorControlState::default();
            f(&mut state);
            states.insert(entity, state);
        }
    }
    true
}

pub fn register(world: &mut World) {
    world.register::<ActorControlState>();
    world.init_resource::<PlayerControlState>();
}

#[cfg(test)]
mod tests {
    use super::*;

    /// #5017 — the Creation Kit: "flagging the actor as unconscious unflags
    /// it as restrained, and vice versa". Clearing one leaves the other.
    #[test]
    fn unconscious_and_restrained_are_mutually_exclusive() {
        let mut state = ActorControlState::default();
        state.set_restrained(true);
        state.set_unconscious(true);
        assert!(state.unconscious && !state.restrained);
        state.set_restrained(true);
        assert!(state.restrained && !state.unconscious);
        state.set_unconscious(false);
        assert!(state.restrained, "clearing unconscious leaves restrained");
    }

    #[test]
    fn selective_disable_leaves_unselected_domains_untouched() {
        let mut state = PlayerControlState::default();
        state.apply_selection(
            false,
            PlayerControlSelection {
                movement: true,
                fighting: false,
                looking: true,
                ..PlayerControlSelection::DISABLE_PLAYER_CONTROLS_DEFAULT
            },
        );

        assert!(!state.movement_enabled);
        assert!(state.fighting_enabled);
        assert!(!state.looking_enabled);
        assert!(state.journal_tabs_enabled);
    }
}
