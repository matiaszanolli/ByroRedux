//! `music.status` — the music controller's observable (Game Music PRD MUS-RPT-001,
//! `docs/engine/music.md` §8).

use super::shared::*;

/// `music.status` — the Hold flag, then each layer's outcome, source record, file and position.
pub(crate) struct MusicStatusCommand;

impl ConsoleCommand for MusicStatusCommand {
    fn name(&self) -> &str {
        "music.status"
    }
    fn description(&self) -> &str {
        "The music: Hold, then each layer (Sting, Script, Battle, Location) with its outcome, source, file and position"
    }
    fn execute(&self, world: &World, _args: &str) -> CommandOutput {
        CommandOutput::lines(crate::music::status_lines(world))
    }
}
