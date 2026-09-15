//! Studio material-gallery commands.
//!
//! `studio.games`, `studio.find`, `studio.add`, `studio.list`,
//! `studio.remove`, `studio.clear`, `studio.overlay`. The gallery ones route
//! through
//! [`crate::studio_host::apply_gallery_command`] — the same typed
//! `StudioCommand` path the egui panel uses — so a scripted `byro-dbg`
//! session reproduces exactly what a click does.

use super::shared::*;
use byroredux_sdk::studio::{AssetId, StudioCommand};

/// Rows `studio.find` prints; the full match count is always reported.
const FIND_ROWS: usize = 25;

fn gallery(world: &World, name: &str, command: StudioCommand, queued: String) -> CommandOutput {
    match crate::studio_host::apply_gallery_command(world, command) {
        Ok(()) => CommandOutput::line(queued),
        Err(error) => CommandOutput::line(format!("{name}: {error}")),
    }
}

/// `studio.games` — installed titles the gallery can import from.
pub(crate) struct StudioGamesCommand;
impl ConsoleCommand for StudioGamesCommand {
    fn name(&self) -> &str {
        "studio.games"
    }

    fn description(&self) -> &str {
        "List the installed games the Studio gallery can import from"
    }

    fn execute(&self, world: &World, _args: &str) -> CommandOutput {
        let Some(games) = crate::studio_host::installed_game_list(world) else {
            return CommandOutput::line(
                "studio.games: no Studio document is open (launch with --studio)",
            );
        };
        let mut lines = vec![format!("{} installed game(s):", games.len())];
        lines.extend(games.into_iter().map(|(key, name, open)| {
            format!("  {key:10} {name}{}", if open { "  [open]" } else { "" })
        }));
        CommandOutput::lines(lines)
    }
}

/// `studio.find <game> [filter…]` — open a game's archives and list matches.
pub(crate) struct StudioFindCommand;
impl ConsoleCommand for StudioFindCommand {
    fn name(&self) -> &str {
        "studio.find"
    }

    fn description(&self) -> &str {
        "List a game's importable meshes matching every term (usage: studio.find <game> [filter...])"
    }

    fn execute(&self, world: &World, args: &str) -> CommandOutput {
        let mut parts = args.trim().splitn(2, char::is_whitespace);
        let Some(game) = parts.next().filter(|game| !game.is_empty()) else {
            return CommandOutput::line("usage: studio.find <game> [filter...]");
        };
        let filter = parts.next().unwrap_or("").trim().to_owned();
        let command = StudioCommand::BrowseCatalog {
            game: game.to_owned(),
            filter,
        };
        if let Err(error) = crate::studio_host::apply_gallery_command(world, command) {
            return CommandOutput::line(format!("studio.find: {error}"));
        }
        let Some(snapshot) = crate::studio_host::snapshot(world) else {
            return CommandOutput::line("studio.find: no Studio document is open");
        };
        let page = &snapshot.catalog.page;
        let mut lines = vec![format!(
            "{} of {} meshes match",
            page.total_matches, snapshot.catalog.total_assets
        )];
        lines.extend(
            page.matches
                .iter()
                .take(FIND_ROWS)
                .map(|path| format!("  {path}")),
        );
        if page.total_matches > FIND_ROWS {
            lines.push(format!("  … {} more", page.total_matches - FIND_ROWS));
        }
        CommandOutput::lines(lines)
    }
}

/// `studio.add <game> <path>` — place one asset in the room.
pub(crate) struct StudioAddCommand;
impl ConsoleCommand for StudioAddCommand {
    fn name(&self) -> &str {
        "studio.add"
    }

    fn description(&self) -> &str {
        "Add one asset to the Studio room (usage: studio.add <game> <archive path>)"
    }

    fn execute(&self, world: &World, args: &str) -> CommandOutput {
        let mut parts = args.trim().splitn(2, char::is_whitespace);
        let (Some(game), Some(path)) = (parts.next(), parts.next().map(str::trim)) else {
            return CommandOutput::line("usage: studio.add <game> <archive path>");
        };
        if game.is_empty() || path.is_empty() {
            return CommandOutput::line("usage: studio.add <game> <archive path>");
        }
        gallery(
            world,
            self.name(),
            StudioCommand::AddAsset {
                game: game.to_owned(),
                path: path.to_owned(),
            },
            format!("queued {game} · {path} (placed next frame; see studio.list)"),
        )
    }
}

/// `studio.list` — placed assets with IDs and world-space envelopes.
pub(crate) struct StudioListCommand;
impl ConsoleCommand for StudioListCommand {
    fn name(&self) -> &str {
        "studio.list"
    }

    fn description(&self) -> &str {
        "List the assets placed in the Studio room with their bounds"
    }

    fn execute(&self, world: &World, _args: &str) -> CommandOutput {
        let Some(snapshot) = crate::studio_host::snapshot(world) else {
            return CommandOutput::line(
                "studio.list: no Studio document is open (launch with --studio)",
            );
        };
        let bounds = crate::studio_host::placed_asset_bounds(world);
        let mut lines = vec![format!("{} asset(s) in the room:", snapshot.assets.len())];
        for asset in &snapshot.assets {
            let envelope = bounds
                .iter()
                .find(|(id, _)| *id == asset.id)
                .map(|(_, b)| {
                    format!(
                        " min=({:.1}, {:.1}, {:.1}) max=({:.1}, {:.1}, {:.1})",
                        b.min[0], b.min[1], b.min[2], b.max[0], b.max[1], b.max[2]
                    )
                })
                .unwrap_or_default();
            lines.push(format!(
                "  #{} {} · {} — {} objects{}",
                asset.id.get(),
                asset.game,
                asset.path,
                asset.object_count,
                envelope
            ));
        }
        if let Some(status) = snapshot.status {
            lines.push(format!("last: {status}"));
        }
        CommandOutput::lines(lines)
    }
}

/// `studio.remove <id>` — take one asset out of the room.
pub(crate) struct StudioRemoveCommand;
impl ConsoleCommand for StudioRemoveCommand {
    fn name(&self) -> &str {
        "studio.remove"
    }

    fn description(&self) -> &str {
        "Remove one asset from the Studio room (usage: studio.remove <id from studio.list>)"
    }

    fn execute(&self, world: &World, args: &str) -> CommandOutput {
        let Some(asset) = args.trim().parse::<u64>().ok().and_then(AssetId::new) else {
            return CommandOutput::line("usage: studio.remove <id from studio.list>");
        };
        gallery(
            world,
            self.name(),
            StudioCommand::RemoveAsset(asset),
            format!("queued removal of #{}", asset.get()),
        )
    }
}

/// `studio.overlay <on|off>` — show/hide the debug overlay for clean captures.
pub(crate) struct StudioOverlayCommand;
impl ConsoleCommand for StudioOverlayCommand {
    fn name(&self) -> &str {
        "studio.overlay"
    }

    fn description(&self) -> &str {
        "Show or hide the Studio window for screenshots (usage: studio.overlay <on|off>)"
    }

    fn execute(&self, world: &World, args: &str) -> CommandOutput {
        let visible = match args.trim() {
            "on" => true,
            "off" => false,
            _ => return CommandOutput::line("usage: studio.overlay <on|off>"),
        };
        match crate::studio_host::request_overlay(world, visible) {
            Ok(()) => CommandOutput::line(format!(
                "overlay {} from next frame",
                if visible { "shown" } else { "hidden" }
            )),
            Err(error) => CommandOutput::line(format!("studio.overlay: {error}")),
        }
    }
}

/// `studio.clear` — empty the room.
pub(crate) struct StudioClearCommand;
impl ConsoleCommand for StudioClearCommand {
    fn name(&self) -> &str {
        "studio.clear"
    }

    fn description(&self) -> &str {
        "Remove every asset from the Studio room"
    }

    fn execute(&self, world: &World, _args: &str) -> CommandOutput {
        gallery(
            world,
            self.name(),
            StudioCommand::ClearAssets,
            "queued clear".to_owned(),
        )
    }
}
