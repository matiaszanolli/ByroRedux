//! Console commands for the engine's built-in command system.
//!
//! Split into per-domain submodules (#1323 / TD9-NEW-03) — the file
//! crossed the 2000-LOC ceiling as a flat collection of independent
//! `impl ConsoleCommand` structs. Each command is self-contained; the
//! only coupling is the formatting/lookup helpers and the external-type
//! import prelude, both re-exported from [`shared`].
//!
//! - [`world_info`] — engine / world / memory introspection
//!   (`help`, `stats`, `entities`, `systems`, `sys.accesses`, `mem.frag`, `ctx.scratch`)
//! - [`assets`] — texture / mesh / skin diagnostics
//!   (`tex.*`, `mesh.*`, `skin.*`)
//! - [`view`] — camera + selection / picking
//!   (`prid`, `cam.*`, `near`, `pick`)
//! - [`scene`] — scene / lighting / material / script state
//!   (`light.*`, `door.teleport`, `script.activate`, `mat.*`, `ragdoll`)

mod assets;
mod scene;
mod shared;
mod view;
mod world_info;

use assets::*;
use scene::*;
use shared::*;
use view::*;
use world_info::*;

pub(crate) fn build_command_registry() -> CommandRegistry {
    let mut registry = CommandRegistry::new();
    registry.register(HelpCommand);
    registry.register(StatsCommand);
    registry.register(EntitiesCommand);
    registry.register(SystemsCommand);
    registry.register(TexMissingCommand);
    registry.register(TexLoadedCommand);
    registry.register(MeshInfoCommand);
    registry.register(MeshCacheCommand);
    registry.register(CtxScratchCommand);
    registry.register(SkinCoverageCommand);
    registry.register(PridCommand);
    registry.register(CamWhereCommand);
    registry.register(NearCommand);
    registry.register(PickCommand);
    registry.register(CamPosCommand);
    registry.register(CamTpCommand);
    registry.register(DoorTeleportCommand);
    registry.register(SysAccessesCommand);
    registry.register(SkinListCommand);
    registry.register(SkinDumpCommand);
    registry.register(MemFragCommand);
    registry.register(LightDumpCommand);
    registry.register(LightAttenCommand);
    registry.register(ScriptActivateCommand);
    registry.register(MatListCommand);
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
