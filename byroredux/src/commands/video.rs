//! `video.status` / `video.skip` — the movie player's observables (PRD `bink-video-playback`,
//! BIK-RPT-001 / BIK-RPT-003).

use super::shared::*;
use crate::video::{VideoControl, VideoOutcome, VideoStatus};

/// `video.status` — the movie on screen (frame and count, flags) and how the last request ended.
pub(crate) struct VideoStatusCommand;

impl ConsoleCommand for VideoStatusCommand {
    fn name(&self) -> &str {
        "video.status"
    }
    fn description(&self) -> &str {
        "The movie player: the movie playing (frame, flags) and how the last PlayBink request ended"
    }
    fn execute(&self, world: &World, _args: &str) -> CommandOutput {
        let Some(status) = world.try_resource::<VideoStatus>() else {
            return CommandOutput::error("video.status: no movie player");
        };
        let playing = match &status.playing {
            Some(playing) => format!(
                "playing {} frame {}/{} (interruptable={} mute={} pause_music={} letterbox={} new_game={})",
                playing.request.file,
                playing.frame,
                playing.frames,
                playing.request.interruptable,
                playing.request.mute_audio,
                playing.request.pause_music,
                playing.request.letterbox,
                playing.request.new_game
            ),
            None => "not playing".to_owned(),
        };
        // BIK-PRES-005 — what the flags actually hold, read back from the audio world.
        let hold = world.try_resource::<byroredux_audio::AudioWorld>().map(|audio| audio.movie_hold()).unwrap_or_default();
        let playing = if status.playing.is_some() {
            format!("{playing} holding world_muted={} music_paused={}", hold.world_muted, hold.music_paused)
        } else {
            playing
        };
        let last = match &status.last {
            Some((file, VideoOutcome::Played { ended_early })) => format!("last: {file} Played{}", if *ended_early { " (ended early)" } else { "" }),
            Some((file, VideoOutcome::Skipped(why))) => format!("last: {file} Skipped ({why})"),
            Some((file, VideoOutcome::Failed(why))) => format!("last: {file} Failed ({why})"),
            None => "last: none".to_owned(),
        };
        CommandOutput::line(format!("video: {playing}; {last}"))
    }
}

/// `video.skip` — Escape on the playing movie: ends it when its request is interruptable.
pub(crate) struct VideoSkipCommand;

impl ConsoleCommand for VideoSkipCommand {
    fn name(&self) -> &str {
        "video.skip"
    }
    fn description(&self) -> &str {
        "Press Escape on the playing movie: ends it if its PlayBink request is interruptable"
    }
    fn execute(&self, world: &World, _args: &str) -> CommandOutput {
        let Some(mut control) = world.try_resource_mut::<VideoControl>() else {
            return CommandOutput::error("video.skip: no movie player");
        };
        control.skip = true;
        CommandOutput::line("video.skip: Escape sent to the movie player")
    }
}
