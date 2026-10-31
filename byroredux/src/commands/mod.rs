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
//! - [`depth`] — depth-buffer precision diagnostics (`depth.stats`)
//! - [`assets`] — texture / mesh / skin diagnostics
//!   (`tex.*`, `mesh.*`, `skin.*`)
//! - [`view`] — camera + selection / picking
//!   (`prid`, `cam.*`, `near`, `pick`, `interaction.status`, `input.*`,
//!   `player.status`)
//! - [`quest`] — quest/scene lifecycle, objectives, targets, aliases, and
//!   playback diagnostics (`quest.show`, `quest.aliases`, `quest.start`,
//!   `quest.stop`, `quest.setstage`, `scene.show`)
//! - [`time`] — persistent day/night clock inspection and controls
//!   (`time.show`, `time.set`, `time.scale`, `time.pause`, `time.resume`, `time.advance`)
//! - [`water`] — canonical water render/physics diagnostics
//!   (`water.dump`, `water.contacts`)
//! - [`physics`] — collider column census + simulation counters
//!   (`phys.census`, `phys.stats`)
//! - [`scene`] — scene / lighting / material / script state
//!   (`light.*`, `door.teleport`, `script.activate`, `mat.*`, `ragdoll`)
//! - [`studio`] — Studio material gallery (`studio.games`, `studio.find`,
//!   `studio.add`, `studio.list`, `studio.remove`, `studio.clear`)

mod actor_value;
mod anim_archetype;
mod cndf;
mod bribe_info;
mod anim_event;
mod script_var;
mod assets;
mod condition;
mod depth;
mod env_health;
mod gameplay;
mod hud;
mod vats;
mod physics;
mod quest;
mod ragdoll_status;
mod scene;
mod story_actions;
// #3445 — widened to `pub(crate)` so `studio_host::snapshot` (outside
// `commands`) can reuse `resolve_entity_name`'s canonical `Name ->
// StringPool` acquisition order instead of re-deriving it.
pub(crate) mod shared;
mod studio;
mod terrain;
mod time;
mod view;
mod water;
mod world_info;

use actor_value::*;
use anim_archetype::AnimArchetypeCommand;
use cndf::CndfCommand;
use bribe_info::BribeInfoCommand;
use anim_event::{ActorWalkCommand, AnimEventCommand, AnimPoseCommand, AnimVarCommand};
use script_var::{QuestForceAliasCommand, ScanInfoCommand, ScriptCallCommand, ScriptExecCommand, ScriptQuestCallCommand, ScriptVarCommand};
use assets::*;
use condition::*;
use depth::*;
use env_health::*;
use gameplay::*;
use hud::*;
use vats::*;
use physics::*;
use quest::*;
use scene::*;
use shared::*;
use studio::*;
use terrain::{
    TerrainBlendCommand, TerrainHeightCommand, TerrainParallaxCommand, TerrainSoftenCommand,
};
use time::*;
use view::*;
use water::*;
use world_info::*;

pub(crate) fn build_command_registry() -> CommandRegistry {
    let mut registry = CommandRegistry::new();
    registry.register(HelpCommand);
    registry.register(HudOnCommand);
    registry.register(HudOffCommand);
    registry.register(HudValuesCommand);
    registry.register(HudHeadingCommand);
    registry.register(HudStatusCommand);
    registry.register(HudDebugCommand);
    registry.register(VatsEnterCommand);
    registry.register(VatsScanCommand);
    registry.register(VatsStatusCommand);
    registry.register(VatsQueueCommand);
    registry.register(VatsConfirmCommand);
    registry.register(VatsExitCommand);
    registry.register(CondCommand);
    registry.register(DepthStatsCommand);
    registry.register(SetAvCommand);
    registry.register(AnimArchetypeCommand);
    registry.register(CndfCommand);
    registry.register(BribeInfoCommand);
    registry.register(AnimEventCommand);
    registry.register(AnimVarCommand);
    registry.register(AnimPoseCommand);
    registry.register(ActorWalkCommand);
    registry.register(ScriptVarCommand);
    registry.register(ScriptCallCommand);
    registry.register(ScriptExecCommand);
    registry.register(QuestForceAliasCommand);
    registry.register(ScanInfoCommand);
    registry.register(ScriptQuestCallCommand);
    registry.register(ModAvCommand);
    registry.register(QuestShowCommand);
    registry.register(QuestEffectsCommand);
    registry.register(ObScriptObjectsCommand);
    registry.register(QuestAliasesCommand);
    registry.register(DialogueStatusCommand);
    registry.register(DialogueEndCommand);
    registry.register(DialogueSelectCommand);
    registry.register(UiAnswerCommand);
    registry.register(DialogueForceGreetCommand);
    registry.register(SmEventCommand);
    registry.register(SceneShowCommand);
    registry.register(QuestStartCommand);
    registry.register(QuestStopCommand);
    registry.register(TerminalSelectCommand);
    registry.register(PerkRunCommand);
    registry.register(QuestSetStageCommand);
    registry.register(TimeShowCommand);
    registry.register(TimeSetCommand);
    registry.register(TimeScaleCommand);
    registry.register(TimePauseCommand);
    registry.register(TimeResumeCommand);
    registry.register(TimeAdvanceCommand);
    registry.register(StatsCommand);
    registry.register(EntitiesCommand);
    registry.register(LoadScreenCensusCommand);
    registry.register(SystemsCommand);
    registry.register(SdkCompatCommand);
    registry.register(TexMissingCommand);
    registry.register(TexLoadedCommand);
    registry.register(TexDumpCommand);
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
    registry.register(PlayerViewCommand);
    registry.register(PlayerBodyCommand);
    registry.register(InvAddCommand);
    registry.register(LevelUpCommand);
    registry.register(InvEquipCommand);
    registry.register(InventoryStatusCommand);
    registry.register(MineStatusCommand);
    registry.register(AttractionStatusCommand);
    registry.register(ArmorStatusCommand);
    registry.register(story_actions::LockStatusCommand);
    registry.register(story_actions::LockOpenCommand);
    registry.register(story_actions::WordTeachCommand);
    registry.register(story_actions::JailSendCommand);
    registry.register(story_actions::JailEscapeCommand);
    registry.register(story_actions::StartReleaseCommand);
    registry.register(story_actions::PlaceAtCommand);
    registry.register(story_actions::SmNodesCommand);
    registry.register(story_actions::ConvStatusCommand);
    registry.register(story_actions::EntityMoveCommand);
    registry.register(HardcoreCommand);
    registry.register(SettingsStatusCommand);
    registry.register(WaterDumpCommand);
    registry.register(WaterContactsCommand);
    registry.register(DoorTeleportCommand);
    registry.register(quest::WeatherForceCommand);
    registry.register(quest::ObScriptCmdCommand);
    registry.register(quest::JailServeCommand);
    registry.register(quest::MiscStatCommand);
    registry.register(quest::ScriptRequestsCommand);
    registry.register(quest::ChallengeStatusCommand);
    registry.register(gameplay::InvUseCommand);
    registry.register(gameplay::InvDropCommand);
    registry.register(gameplay::CraftCommand);
    registry.register(gameplay::MagicCastCommand);
    registry.register(gameplay::MagicEffectsCommand);
    registry.register(gameplay::ActorReviveCommand);
    registry.register(gameplay::ActorLookCommand);
    registry.register(SysAccessesCommand);
    registry.register(EngineQuitCommand);
    registry.register(SkinListCommand);
    registry.register(SkinDumpCommand);
    registry.register(MemFragCommand);
    registry.register(WorldOwnersCommand);
    registry.register(RenderHealthCommand);
    registry.register(RtIntegrityCommand);
    registry.register(ShadowMasksCommand);
    registry.register(LodCoverageCommand);
    registry.register(TerrainSeamsCommand);
    registry.register(TerrainHeightCommand);
    registry.register(TerrainBlendCommand);
    registry.register(TerrainSoftenCommand);
    registry.register(TerrainParallaxCommand);
    registry.register(CellOwnersCommand);
    registry.register(RenderDebugCommand);
    registry.register(RenderDebugFlagsCommand);
    registry.register(ShadowFilterCommand);
    registry.register(EnvHealthCommand);
    registry.register(SkyDumpCommand);
    registry.register(LightDumpCommand);
    registry.register(LightAttenCommand);
    // Stage 1 (RENDERING-PLAN.md) — color-pipeline live controls.
    registry.register(ExposureCommand);
    registry.register(TonemapCommand);
    registry.register(ScriptActivateCommand);
    registry.register(MatListCommand);
    registry.register(MatDumpCommand);
    registry.register(MatSetCommand);
    registry.register(RagdollCommand);
    registry.register(ragdoll_status::RagdollStatusCommand);
    registry.register(ragdoll_status::NpcAppearanceCommand);
    registry.register(PhysCensusCommand);
    registry.register(PhysStatsCommand);
    registry.register(StudioGamesCommand);
    registry.register(StudioFindCommand);
    registry.register(StudioAddCommand);
    registry.register(StudioListCommand);
    registry.register(StudioRemoveCommand);
    registry.register(StudioClearCommand);
    registry.register(StudioOverlayCommand);
    // M45 — save/load (the matching `SaveRegistry` + `SaveState`
    // resources are installed alongside the command registry).
    registry.register(crate::save_io::SaveCommand);
    registry.register(crate::save_io::SaveInfoCommand);
    registry.register(crate::save_io::LoadCommand);
    registry
}

#[cfg(test)]
mod debug_cli_doc_tests {
    use super::build_command_registry;

    #[test]
    fn every_registered_command_is_named_in_debug_cli_docs() {
        let docs = include_str!("../../../docs/engine/debug-cli.md");
        let registry = build_command_registry();
        let commands = registry.list();
        assert!(
            docs.contains(&format!(
                "Current registered commands (**{}**)",
                commands.len()
            )),
            "debug-cli.md command count must match the live registry ({})",
            commands.len()
        );
        let missing: Vec<_> = commands
            .iter()
            .filter(|(name, _)| !docs.contains(name))
            .collect();
        assert!(
            missing.is_empty(),
            "commands missing from debug-cli.md: {missing:?}"
        );
    }
}

#[cfg(test)]
#[path = "../commands_tests.rs"]
mod tests;
