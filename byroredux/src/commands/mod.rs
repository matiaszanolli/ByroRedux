//! Console commands for the engine's built-in command system.
//!
//! Split into per-domain submodules (#1323 / TD9-NEW-03) — the file
//! crossed the 2000-LOC ceiling as a flat collection of independent
//! `impl ConsoleCommand` structs. Each command is self-contained; the
//! only coupling is the formatting/lookup helpers and the external-type
//! import prelude, both re-exported from [`shared`].
//!
//! - [`world_info`] — engine / world / memory introspection
//!   (`help`, `stats`, `entities`, `systems`, `sys.accesses`, `mem.frag`,
//!   `ctx.scratch`, `world.owners`, `r.health`)
//! - [`env_health`] — environment-value gate over the live lighting/sky
//!   resources (`env.health`)
//! - [`gameplay`] — inventory/equipment and persistent-settings diagnostics
//!   (`inventory.status`, `settings.status`)
//! - [`assets`] — texture / mesh / skin diagnostics
//!   (`tex.*`, `mesh.*`, `skin.*`)
//! - [`view`] — camera + selection / picking
//!   (`prid`, `cam.*`, `near`, `pick`, `interaction.status`, `input.*`,
//!   `player.status`)
//! - [`quest`] — quest lifecycle, objectives, targets, and alias diagnostics
//!   (`quest.show`, `quest.aliases`, `quest.start`, `quest.stop`, `quest.setstage`)
//! - [`time`] — persistent day/night clock inspection and controls
//!   (`time.show`, `time.set`, `time.scale`, `time.pause`, `time.resume`, `time.advance`)
//! - [`water`] — canonical water render/physics diagnostics
//!   (`water.dump`, `water.contacts`)
//! - [`scene`] — scene / lighting / material / script state
//!   (`light.*`, `door.teleport`, `script.activate`, `mat.*`, `ragdoll`)

mod actor_value;
mod assets;
mod condition;
mod env_health;
mod gameplay;
mod quest;
mod scene;
mod shared;
mod time;
mod view;
mod water;
mod world_info;

use actor_value::*;
use assets::*;
use condition::*;
use env_health::*;
use gameplay::*;
use quest::*;
use scene::*;
use shared::*;
use time::*;
use view::*;
use water::*;
use world_info::*;

pub(crate) fn build_command_registry() -> CommandRegistry {
    let mut registry = CommandRegistry::new();
    registry.register(HelpCommand);
    registry.register(CondCommand);
    registry.register(SetAvCommand);
    registry.register(ModAvCommand);
    registry.register(QuestShowCommand);
    registry.register(QuestAliasesCommand);
    registry.register(QuestStartCommand);
    registry.register(QuestStopCommand);
    registry.register(QuestSetStageCommand);
    registry.register(TimeShowCommand);
    registry.register(TimeSetCommand);
    registry.register(TimeScaleCommand);
    registry.register(TimePauseCommand);
    registry.register(TimeResumeCommand);
    registry.register(TimeAdvanceCommand);
    registry.register(StatsCommand);
    registry.register(EntitiesCommand);
    registry.register(SystemsCommand);
    registry.register(TexMissingCommand);
    registry.register(TexLoadedCommand);
    registry.register(MeshInfoCommand);
    registry.register(MeshCacheCommand);
    registry.register(CtxScratchCommand);
    registry.register(CtxUpscalerCommand);
    registry.register(UpscalerSwitchCommand);
    registry.register(SkinCoverageCommand);
    registry.register(PridCommand);
    registry.register(CamWhereCommand);
    registry.register(NearCommand);
    registry.register(PickCommand);
    registry.register(CamPosCommand);
    registry.register(CamTpCommand);
    registry.register(InteractionStatusCommand);
    registry.register(CombatStatusCommand);
    registry.register(CombatApproachCommand);
    registry.register(InputPressCommand);
    registry.register(InputHoldCommand);
    registry.register(InputLookCommand);
    registry.register(PlayerStatusCommand);
    registry.register(InventoryStatusCommand);
    registry.register(SettingsStatusCommand);
    registry.register(WaterDumpCommand);
    registry.register(WaterContactsCommand);
    registry.register(DoorTeleportCommand);
    registry.register(SysAccessesCommand);
    registry.register(SkinListCommand);
    registry.register(SkinDumpCommand);
    registry.register(MemFragCommand);
    registry.register(WorldOwnersCommand);
    registry.register(RenderHealthCommand);
    registry.register(RtIntegrityCommand);
    registry.register(RenderDebugCommand);
    registry.register(EnvHealthCommand);
    registry.register(LightDumpCommand);
    registry.register(LightAttenCommand);
    registry.register(ScriptActivateCommand);
    registry.register(MatListCommand);
    registry.register(MatDumpCommand);
    registry.register(MatSetCommand);
    registry.register(RagdollCommand);
    // M45 — save/load (the matching `SaveRegistry` + `SaveState`
    // resources are installed alongside the command registry).
    registry.register(crate::save_io::SaveCommand);
    registry.register(crate::save_io::SaveInfoCommand);
    registry.register(crate::save_io::LoadCommand);
    registry
}

#[cfg(test)]
#[path = "../commands_tests.rs"]
mod tests;
