//! The movie player (PRD `bink-video-playback`, #6855): carries out the `PlayBink` requests scripts
//! raise (`byroredux_scripting::obscript_more::PendingVideos`) with the system FFmpeg
//! (`byroredux_video`, BIK-DEC-005).
//!
//! A movie plays full-screen through the same overlay quad the loading cover and the HUD use, its
//! audio on the scripted-music track, and holds the game still while it runs (`dt` is pinned to 0.0
//! like the loading cover, BIK-PRES-002). Escape ends an interruptable movie (BIK-PRES-004); so does
//! `video.skip`, which the headless gates drive (BIK-RPT-003). A movie the build cannot decode — no
//! FFmpeg, no file, a stream FFmpeg rejects — is reported and skipped, never a stall (§3 outcomes).
//!
//! Not yet carried out: the mute-audio and pause-music flags (BIK-PRES-005) — the movie's own sound
//! replaces the scripted-music layer, the world's sound keeps playing — and the Papyrus
//! `Game.PlayBink` native (BIK-REQ-002).

use std::path::{Path, PathBuf};
use std::time::Instant;

use byroredux_core::ecs::{Resource, World};
use byroredux_renderer::VulkanContext;
use byroredux_scripting::obscript_more::{PendingVideos, VideoRequest};
use byroredux_video::{Movie, Playhead};

/// How a movie request ended (PRD §3).
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum VideoOutcome {
    /// Every frame shown, or ended early by Escape / `video.skip` on an interruptable request.
    Played { ended_early: bool },
    /// This build cannot decode it (no FFmpeg).
    Skipped(String),
    /// The file is missing or did not decode.
    Failed(String),
}

/// The movie being shown, as `video.status` reports it.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PlayingVideo {
    pub(crate) request: VideoRequest,
    pub(crate) frame: u64,
    pub(crate) frames: u64,
}

/// What the player is doing and how the last request ended (BIK-RPT-001). `NOT_SAVED_BY_DESIGN`:
/// session telemetry, a load starts none.
#[derive(Debug, Default)]
pub(crate) struct VideoStatus {
    pub(crate) playing: Option<PlayingVideo>,
    pub(crate) last: Option<(String, VideoOutcome)>,
}

impl Resource for VideoStatus {}

/// Escape and `video.skip` raise `skip`; the player consumes it. `NOT_SAVED_BY_DESIGN`: consumed the
/// frame it is raised.
#[derive(Debug, Default)]
pub(crate) struct VideoControl {
    pub(crate) skip: bool,
}

impl Resource for VideoControl {}

struct Active {
    movie: Movie,
    request: VideoRequest,
    /// When the first frame was shown: the movie clock starts there, with its audio.
    started: Option<Instant>,
    finished: bool,
}

/// The movie player the app owns.
#[derive(Default)]
pub(crate) struct VideoPlayer {
    active: Option<Active>,
    /// The overlay texture frames are uploaded into, kept between movies.
    texture: Option<u32>,
}

impl VideoPlayer {
    /// Whether a movie is on screen: the game is held still, input goes to the movie.
    pub(crate) fn active(&self) -> bool {
        self.active.is_some()
    }

    /// Raise a skip request (Escape pressed during a movie).
    pub(crate) fn request_skip(world: &World) {
        if let Some(mut control) = world.try_resource_mut::<VideoControl>() {
            control.skip = true;
        }
    }

    /// Once a frame: end the movie that finished or was skipped, then start the next queued request
    /// when idle. `window` is the swapchain extent the frames are decoded to fill.
    pub(crate) fn step(&mut self, world: &World, window: Option<(u32, u32)>) {
        let skipped = world.try_resource_mut::<VideoControl>().is_some_and(|mut control| std::mem::take(&mut control.skip));
        if let Some(active) = &self.active {
            if active.finished {
                self.finish(world, VideoOutcome::Played { ended_early: false });
            } else if skipped && active.request.interruptable {
                self.finish(world, VideoOutcome::Played { ended_early: true });
            } else if skipped {
                log::info!("video: Escape ignored — '{}' is not interruptable", active.request.file);
            }
        }
        while self.active.is_none() {
            let Some(request) = world
                .try_resource_mut::<PendingVideos>()
                .and_then(|mut pending| (!pending.0.is_empty()).then(|| pending.0.remove(0)))
            else {
                break;
            };
            self.start(world, request, window);
        }
    }

    fn start(&mut self, world: &World, request: VideoRequest, window: Option<(u32, u32)>) {
        let note = |world: &World, outcome: VideoOutcome, request: &VideoRequest| {
            log::info!("video: '{}' {outcome:?}", request.file);
            if let Some(mut status) = world.try_resource_mut::<VideoStatus>() {
                status.last = Some((request.file.clone(), outcome));
            }
        };
        let Some(window) = window else {
            return note(world, VideoOutcome::Skipped("there is no window to show it in".into()), &request);
        };
        if !byroredux_video::ffmpeg_available() {
            return note(world, VideoOutcome::Skipped("FFmpeg is not installed".into()), &request);
        }
        let Some(path) = movie_path(&request.file) else {
            return note(world, VideoOutcome::Failed("the file is not under the data folder's Video".into()), &request);
        };
        let movie = match Movie::open(&path, window, request.letterbox) {
            Ok(movie) => movie,
            Err(error) => return note(world, VideoOutcome::Failed(error.to_string()), &request),
        };
        let info = movie.info();
        let frames = (info.duration_secs * info.fps).round().max(0.0) as u64;
        log::info!(
            "video: playing '{}' ({}x{} at {:.2} fps, {:.1} s, {} audio track(s)) interruptable={} mute={} pause_music={} letterbox={}",
            request.file,
            info.width,
            info.height,
            info.fps,
            info.duration_secs,
            info.audio_tracks,
            request.interruptable,
            request.mute_audio,
            request.pause_music,
            request.letterbox
        );
        if let Some(mut status) = world.try_resource_mut::<VideoStatus>() {
            status.playing = Some(PlayingVideo { request: request.clone(), frame: 0, frames });
        }
        self.active = Some(Active { movie, request, started: None, finished: false });
    }

    fn finish(&mut self, world: &World, outcome: VideoOutcome) {
        let Some(active) = self.active.take() else {
            return;
        };
        if active.movie.audio_path().is_some() {
            if let Some(mut audio) = world.try_resource_mut::<byroredux_audio::AudioWorld>() {
                audio.stop_script_music(0.0);
            }
        }
        log::info!("video: '{}' {outcome:?} after {} frame(s)", active.request.file, active.movie.frames_shown());
        if let Some(mut status) = world.try_resource_mut::<VideoStatus>() {
            status.playing = None;
            status.last = Some((active.request.file.clone(), outcome));
        }
    }

    /// Once a frame while drawing: the overlay texture to composite over the world, with the frame
    /// the movie clock wants uploaded into it. `None` when no movie is on screen.
    pub(crate) fn tick(&mut self, ctx: &mut VulkanContext, world: &World) -> Option<u32> {
        let Self { active, texture } = self;
        let active = active.as_mut()?;
        let audio_path: Option<PathBuf> = if active.started.is_none() { active.movie.audio_path().map(Path::to_path_buf) } else { None };
        let (width, height) = active.movie.size();
        let elapsed = active.started.map_or(0.0, |started| started.elapsed().as_secs_f64());
        match active.movie.advance(elapsed) {
            Playhead::Waiting => *texture,
            Playhead::Finished => {
                active.finished = true;
                *texture
            }
            Playhead::Frame { index, pixels, new } => {
                if active.started.is_none() {
                    active.started = Some(Instant::now());
                    if let Some(path) = audio_path {
                        play_audio(world, &path);
                    }
                }
                if new {
                    let upload_ctx = byroredux_renderer::upload_ctx!(ctx);
                    let uploaded = match *texture {
                        Some(handle) => ctx.texture_registry.update_rgba(upload_ctx, handle, width, height, pixels).map(|()| handle),
                        None => ctx.texture_registry.register_rgba(upload_ctx, width, height, pixels),
                    };
                    match uploaded {
                        Ok(handle) => *texture = Some(handle),
                        Err(error) => log::error!("video: frame {index} upload failed: {error:#}"),
                    }
                    if let Some(mut status) = world.try_resource_mut::<VideoStatus>() {
                        if let Some(playing) = &mut status.playing {
                            playing.frame = index;
                        }
                    }
                }
                *texture
            }
        }
    }
}

/// The movie's soundtrack on the scripted-music track, from the start.
fn play_audio(world: &World, wav: &Path) {
    match byroredux_audio::load_streaming_sound_from_file(wav) {
        Ok(sound) => {
            if let Some(mut audio) = world.try_resource_mut::<byroredux_audio::AudioWorld>() {
                audio.play_script_music(sound, 1.0, 0.0, false);
            }
        }
        Err(error) => log::warn!("video: its soundtrack could not be opened: {error}"),
    }
}

/// `file` under the data folder's `Video`, matching case-insensitively. A leading `..\` climbs out of
/// `Video` (the GECK's `PlayBink "..\sound\videos\x.bik"`).
fn movie_path(file: &str) -> Option<PathBuf> {
    let (_, data) = crate::cli_args::esm_discovery_root(&crate::cli_args::effective_args()).ok()?;
    resolve_under(&data.join("Video"), file)
}

fn resolve_under(video_root: &Path, file: &str) -> Option<PathBuf> {
    let mut root = video_root.to_path_buf();
    let mut rest = file;
    while let Some(trimmed) = rest.strip_prefix("..\\").or_else(|| rest.strip_prefix("../")) {
        root = root.parent()?.to_path_buf();
        rest = trimmed;
    }
    crate::systems::script_requests::find_case_insensitive(&root, rest).filter(|path| path.is_file())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The GECK's `PlayBink` paths: relative to `Video`, any case, and `..\` out of it.
    #[test]
    fn a_movie_resolves_under_video_in_any_case_and_a_dotdot_climbs_out() {
        let dir = tempfile::tempdir().unwrap();
        let video = dir.path().join("Video");
        std::fs::create_dir_all(video.join("Mymod")).unwrap();
        std::fs::create_dir_all(dir.path().join("sound").join("videos")).unwrap();
        std::fs::write(video.join("FNVIntro.bik"), b"x").unwrap();
        std::fs::write(video.join("Mymod").join("MyNew.bik"), b"x").unwrap();
        std::fs::write(dir.path().join("sound").join("videos").join("Other.bik"), b"x").unwrap();

        assert_eq!(resolve_under(&video, "fnvintro.BIK"), Some(video.join("FNVIntro.bik")));
        assert_eq!(resolve_under(&video, "Mymod\\MyNew.bik"), Some(video.join("Mymod").join("MyNew.bik")));
        assert_eq!(resolve_under(&video, "..\\sound\\videos\\Other.bik"), Some(dir.path().join("sound").join("videos").join("Other.bik")));
        assert_eq!(resolve_under(&video, "Missing.bik"), None);
        assert_eq!(resolve_under(&video, "Mymod"), None, "a folder is no movie");
    }

    /// A request with nothing to decode it is reported and dropped; one that cannot start never
    /// leaves the player holding the game.
    #[test]
    fn a_request_without_a_window_is_skipped_and_reported() {
        let mut world = World::new();
        world.insert_resource(PendingVideos(vec![VideoRequest {
            file: "FNVIntro.bik".into(),
            interruptable: true,
            mute_audio: true,
            pause_music: false,
            letterbox: true,
        }]));
        world.insert_resource(VideoStatus::default());
        world.insert_resource(VideoControl::default());
        let mut player = VideoPlayer::default();
        player.step(&world, None);
        assert!(!player.active(), "nothing to hold the game for");
        assert!(world.resource::<PendingVideos>().0.is_empty(), "the request was consumed");
        let status = world.resource::<VideoStatus>();
        assert_eq!(status.last.as_ref().map(|(file, _)| file.as_str()), Some("FNVIntro.bik"));
        assert!(matches!(status.last, Some((_, VideoOutcome::Skipped(_)))));
    }

    /// Escape only ends an interruptable movie, and a skip raised with no movie is not left armed.
    #[test]
    fn a_skip_ends_only_an_interruptable_movie_and_is_always_consumed() {
        let mut world = World::new();
        world.insert_resource(PendingVideos::default());
        world.insert_resource(VideoStatus::default());
        world.insert_resource(VideoControl { skip: true });
        let mut player = VideoPlayer::default();
        player.step(&world, Some((64, 48)));
        assert!(!world.resource::<VideoControl>().skip, "a stray skip does not wait for the next movie");

        let request = |interruptable| VideoRequest { file: "x.bik".into(), interruptable, mute_audio: true, pause_music: true, letterbox: true };
        let frames = || Movie::from_frames(byroredux_video::MovieInfo { width: 2, height: 2, fps: 10.0, duration_secs: 1.0, audio_tracks: 0 }, (2, 2), vec![vec![0; 16]]);
        for (interruptable, ends) in [(false, false), (true, true)] {
            player.active = Some(Active { movie: frames(), request: request(interruptable), started: None, finished: false });
            world.resource_mut::<VideoControl>().skip = true;
            player.step(&world, Some((64, 48)));
            assert_eq!(player.active(), !ends, "interruptable={interruptable}");
        }
        assert!(matches!(world.resource::<VideoStatus>().last, Some((_, VideoOutcome::Played { ended_early: true }))));
    }
}
