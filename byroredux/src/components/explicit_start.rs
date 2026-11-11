//! The explicit-start pin: a launch that names its starting location
//! (`--cell` / `--grid`) keeps the player there while the session boots.

use byroredux_core::ecs::Resource;

/// While `true`, scripted player relocation is ignored.
///
/// A launch without a named location is a new game, and the Start Game
/// Enabled quests run as authored: MQ101's start-up fragment moves the player
/// to the Helgen cart. A launch that names a cell or a grid cell asked to be
/// somewhere, and that move would carry the player off to Helgen on the first
/// frame. The pin holds until the player takes control (the first key or mouse
/// press), after which scripted moves apply as in any play session. A
/// benchmark or a debug-server session never presses a window key, so its pin
/// stays. Session state, never saved.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ExplicitStartPin(pub(crate) bool);

impl Resource for ExplicitStartPin {}

impl ExplicitStartPin {
    pub(crate) fn active(world: &byroredux_core::ecs::World) -> bool {
        world.try_resource::<Self>().is_some_and(|pin| pin.0)
    }

    /// The player took control: scripted moves apply from now on.
    pub(crate) fn release(world: &byroredux_core::ecs::World) {
        if let Some(mut pin) = world.try_resource_mut::<Self>() {
            pin.0 = false;
        }
    }
}

/// Whether this session began as a new game: the launch named no location
/// (or passed `--new-game`) and is not restoring a save.
///
/// Unlike [`ExplicitStartPin`] this never changes after boot, so a cell load
/// late in the session still knows how the session began. It gates the
/// opening quests an original executable starts by itself on a new game
/// (`install_engine_root_quests`), which must not run over a session that
/// started in a named cell or from a save. Session state, never saved.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct NewGameStart(pub(crate) bool);

impl Resource for NewGameStart {}

impl NewGameStart {
    pub(crate) fn active(world: &byroredux_core::ecs::World) -> bool {
        world.try_resource::<Self>().is_some_and(|start| start.0)
    }
}
