//! The movie player (PRD `bink-video-playback`, #6855): carries out the `PlayBink` requests scripts
//! raise (`byroredux_scripting::obscript_more::PendingVideos`) with the in-process Bink 1 decoder
//! (`byroredux_video`, BIK-DEC-005).
//!
//! A movie plays full-screen through the same overlay quad the loading cover and the HUD use, its
//! sound on the audio crate's movie slot, and holds the game still while it runs (`dt` is pinned to
//! 0.0 like the loading cover, BIK-PRES-002). Its mute and pause-music flags pause the world's sounds
//! and the music layers for its length (BIK-PRES-005). The picture follows the soundtrack's playback
//! position, so the two stay within a frame (BIK-PRES-003); a movie with no sound runs on the wall
//! clock. Escape ends an interruptable movie (BIK-PRES-004); so does `video.skip`, which the headless
//! gates drive (BIK-RPT-003). A movie the build cannot decode is reported Skipped and a missing or
//! damaged file Failed, never a stall (§3 outcomes). Design: `docs/engine/video-playback.md`.

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
    /// This build has no decoder for its format (Bink 2, another Bink revision).
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
    /// BIK-REQ-004 — the game's start sequence has been queued (or skipped) this run.
    start_sequence_done: bool,
}

/// BIK-REQ-004 — one game's start-sequence settings: the default INI the install ships, and the
/// compiled-in defaults of `sIntroSequence:General` and `sMainMenuMovieIntro:General`, read out of
/// each game's executable on 2026-11-13 (the setting object's default value precedes the pointer to
/// its name; `docs/engine/video-playback.md` §5). An INI key that is present, even empty, overrides
/// the compiled-in value.
struct StartSettings {
    default_ini: &'static str,
    intro_sequence: &'static str,
    main_menu_movie_intro: &'static str,
}

fn start_settings(game: byroredux_plugin::esm::reader::GameKind) -> Option<StartSettings> {
    use byroredux_plugin::esm::reader::GameKind;
    Some(match game {
        // Oblivion.exe; `Oblivion_default.ini` names the same four logos and the menu intro.
        GameKind::Oblivion => StartSettings {
            default_ini: "Oblivion_default.ini",
            intro_sequence: "bethesda softworks HD720p.bik,2k games.bik,game studios.bik,Oblivion Legal.bik",
            main_menu_movie_intro: "Oblivion iv logo.bik",
        },
        // Fallout3.exe: every movie setting is empty; `Fallout_default.ini` (FO3 and FNV) leaves
        // `SIntroSequence` and `SMainMenuMovieIntro` empty too.
        GameKind::Fallout3NV => StartSettings { default_ini: "Fallout_default.ini", intro_sequence: "", main_menu_movie_intro: "" },
        // SkyrimSE.exe: `BGS_Logo.bik`; `Skyrim_Default.ini` names no movie.
        GameKind::Skyrim => StartSettings { default_ini: "Skyrim_Default.ini", intro_sequence: "BGS_Logo.bik", main_menu_movie_intro: "" },
        // Fallout4.exe: `GameIntro_V3_B.bk2`; the exe has no `sMainMenuMovieIntro`.
        GameKind::Fallout4 => StartSettings { default_ini: "Fallout4_Default.ini", intro_sequence: "GameIntro_V3_B.bk2", main_menu_movie_intro: "" },
        // Starfield.exe: `BGS_LOGO_1080p_BinkVersion.bk2`; no `sMainMenuMovieIntro`.
        GameKind::Starfield => StartSettings {
            default_ini: "Starfield.ini",
            intro_sequence: "BGS_LOGO_1080p_BinkVersion.bk2",
            main_menu_movie_intro: "",
        },
        // Fallout 76 is out of scope (BIK-NG-005).
        _ => return None,
    })
}

/// `[General] key` of an INI file's text, case-insensitively: `Some("")` for a key set to nothing.
fn ini_general(text: &str, key: &str) -> Option<String> {
    let mut in_general = false;
    for line in text.lines() {
        let line = line.trim();
        if let Some(section) = line.strip_prefix('[').and_then(|rest| rest.strip_suffix(']')) {
            in_general = section.trim().eq_ignore_ascii_case("General");
            continue;
        }
        if !in_general || line.starts_with(';') {
            continue;
        }
        if let Some((name, value)) = line.split_once('=') {
            if name.trim().eq_ignore_ascii_case(key) {
                return Some(value.trim().to_string());
            }
        }
    }
    None
}

/// BIK-REQ-004 — the movies `game` plays at launch, in order: `sIntroSequence` (a comma- or
/// semicolon-separated list), then `sMainMenuMovieIntro`, each from the install's default INI when
/// it sets the key and the executable's compiled-in default otherwise. `ini_text` is that INI.
pub(crate) fn start_sequence(game: byroredux_plugin::esm::reader::GameKind, ini_text: Option<&str>) -> Vec<String> {
    let Some(settings) = start_settings(game) else {
        return Vec::new();
    };
    let setting = |key: &str, compiled: &str| ini_text.and_then(|text| ini_general(text, key)).unwrap_or_else(|| compiled.to_string());
    let intro = setting("sIntroSequence", settings.intro_sequence);
    let menu = setting("sMainMenuMovieIntro", settings.main_menu_movie_intro);
    intro
        .split([',', ';'])
        .chain(std::iter::once(menu.as_str()))
        .map(str::trim)
        .filter(|file| !file.is_empty())
        .map(str::to_string)
        .collect()
}

/// BIK-REQ-005 — whether this run plays the start sequence: not with `--skip-movies`, and not in a
/// benchmark (`--bench-frames`, which measures the world, as every headless gate does) unless
/// `--start-movies` asks for it.
pub(crate) fn start_movies_wanted(args: &[String]) -> bool {
    let has = |flag: &str| args.iter().any(|a| a == flag);
    !has("--skip-movies") && (!has("--bench-frames") || has("--start-movies"))
}

impl VideoPlayer {
    /// Whether a movie is on screen: the game is held still, input goes to the movie.
    pub(crate) fn active(&self) -> bool {
        self.active.is_some()
    }

    /// BIK-REQ-004 / BIK-REQ-005 — once a run, when the world is up (`ready`): queue the game's start
    /// sequence ahead of anything else, or log why it is skipped. Engine-started movies may be ended
    /// with Escape (an engine choice; the original's is not recorded) and hold the world like a
    /// muted, letterboxed `PlayBink`.
    pub(crate) fn queue_start_sequence(&mut self, world: &World, ready: bool) {
        if self.start_sequence_done || !ready {
            return;
        }
        let Some(game) = world.try_resource::<crate::cell_loader::LoadedCellIndex>().map(|index| index.0.game) else {
            return;
        };
        self.start_sequence_done = true;
        let args = crate::cli_args::effective_args();
        let ini = crate::cli_args::esm_discovery_root(&args)
            .ok()
            .and_then(|(_, data)| {
                let install = data.parent()?.to_path_buf();
                let name = start_settings(game)?.default_ini;
                crate::systems::script_requests::find_case_insensitive(&install, name)
            })
            .and_then(|path| std::fs::read(path).ok())
            .map(|bytes| String::from_utf8_lossy(&bytes).into_owned());
        let movies = start_sequence(game, ini.as_deref());
        if movies.is_empty() {
            return;
        }
        if !start_movies_wanted(&args) {
            log::info!("video: start sequence skipped ({} movie(s); --skip-movies, or a benchmark without --start-movies)", movies.len());
            return;
        }
        log::info!("video: start sequence {movies:?}");
        if let Some(mut pending) = world.try_resource_mut::<PendingVideos>() {
            let requests = movies.into_iter().map(|file| VideoRequest {
                file,
                interruptable: true,
                mute_audio: true,
                pause_music: true,
                letterbox: true,
                new_game: false,
            });
            let queued: Vec<VideoRequest> = requests.chain(pending.0.drain(..)).collect();
            pending.0 = queued;
        }
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
        let Some(path) = movie_path(&request.file) else {
            return note(world, VideoOutcome::Failed("the file is not under the data folder's Video".into()), &request);
        };
        let movie = match Movie::open(&path, window, request.letterbox) {
            Ok(movie) => movie,
            Err(byroredux_video::VideoError::Unsupported(reason)) => return note(world, VideoOutcome::Skipped(reason), &request),
            Err(error) => return note(world, VideoOutcome::Failed(error.to_string()), &request),
        };
        let info = movie.info();
        let frames = (info.duration_secs * info.fps).round().max(0.0) as u64;
        log::info!(
            "video: playing '{}' ({}x{} at {:.2} fps, {:.1} s, {} audio track(s)) interruptable={} mute={} pause_music={} letterbox={} new_game={}",
            request.file,
            info.width,
            info.height,
            info.fps,
            info.duration_secs,
            info.audio_tracks,
            request.interruptable,
            request.mute_audio,
            request.pause_music,
            request.letterbox,
            request.new_game
        );
        if let Some(mut status) = world.try_resource_mut::<VideoStatus>() {
            status.playing = Some(PlayingVideo { request: request.clone(), frame: 0, frames });
        }
        // BIK-PRES-005 — the flags hold the world's sound and the music for the movie's length.
        if let Some(mut audio) = world.try_resource_mut::<byroredux_audio::AudioWorld>() {
            audio.hold_for_movie(request.mute_audio, request.pause_music);
        }
        self.active = Some(Active { movie, request, started: None, finished: false });
    }

    fn finish(&mut self, world: &World, outcome: VideoOutcome) {
        let Some(active) = self.active.take() else {
            return;
        };
        if let Some(mut audio) = world.try_resource_mut::<byroredux_audio::AudioWorld>() {
            audio.stop_movie_sound();
            audio.release_movie_hold();
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
        let Self { active, texture, .. } = self;
        let active = active.as_mut()?;
        let audio_path: Option<PathBuf> = if active.started.is_none() { active.movie.audio_path().map(Path::to_path_buf) } else { None };
        let (width, height) = active.movie.size();
        let elapsed = movie_clock(world, active.started);
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

/// BIK-PRES-003 — the movie's clock: its soundtrack's playback position while one plays, so the
/// frame shown is the one the sound is at; the wall clock since the first frame otherwise (no sound
/// in the file, no audio device, or the soundtrack shorter than the picture).
fn movie_clock(world: &World, started: Option<Instant>) -> f64 {
    let Some(started) = started else {
        return 0.0;
    };
    world
        .try_resource::<byroredux_audio::AudioWorld>()
        .and_then(|audio| audio.movie_sound_position())
        .unwrap_or_else(|| started.elapsed().as_secs_f64())
}

/// The movie's soundtrack on the audio crate's movie slot, from the start.
fn play_audio(world: &World, wav: &Path) {
    match byroredux_audio::load_streaming_sound_from_file(wav) {
        Ok(sound) => {
            if let Some(mut audio) = world.try_resource_mut::<byroredux_audio::AudioWorld>() {
                audio.play_movie_sound(sound);
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
            new_game: false,
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

        let request = |interruptable| VideoRequest { file: "x.bik".into(), interruptable, mute_audio: true, pause_music: true, letterbox: true, new_game: false };
        let frames = || Movie::from_frames(byroredux_video::MovieInfo { width: 2, height: 2, fps: 10.0, duration_secs: 1.0, audio_tracks: 0 }, (2, 2), vec![vec![0; 16]]);
        for (interruptable, ends) in [(false, false), (true, true)] {
            player.active = Some(Active { movie: frames(), request: request(interruptable), started: None, finished: false });
            world.resource_mut::<VideoControl>().skip = true;
            player.step(&world, Some((64, 48)));
            assert_eq!(player.active(), !ends, "interruptable={interruptable}");
        }
        assert!(matches!(world.resource::<VideoStatus>().last, Some((_, VideoOutcome::Played { ended_early: true }))));
    }

    /// BIK-REQ-004 — the start sequence comes from the shipped default INI where it sets the key
    /// (even to nothing) and from the executable's compiled-in default otherwise: Oblivion's four
    /// logos then its menu intro, Skyrim's logo, nothing on Fallout 3 / New Vegas.
    #[test]
    fn the_start_sequence_reads_the_default_ini_over_the_compiled_in_settings() {
        use byroredux_plugin::esm::reader::GameKind;
        let oblivion_ini = "[General]\nSMainMenuMovieIntro=Oblivion iv logo.bik\nSIntroSequence=bethesda softworks HD720p.bik,2k games.bik,game studios.bik,Oblivion Legal.bik\n";
        assert_eq!(
            start_sequence(GameKind::Oblivion, Some(oblivion_ini)),
            ["bethesda softworks HD720p.bik", "2k games.bik", "game studios.bik", "Oblivion Legal.bik", "Oblivion iv logo.bik"]
        );
        assert_eq!(start_sequence(GameKind::Oblivion, None).len(), 5, "the exe defaults agree");
        assert_eq!(start_sequence(GameKind::Skyrim, Some("[Display]\nfGamma=1\n")), ["BGS_Logo.bik"], "no key: the exe default");
        assert!(start_sequence(GameKind::Skyrim, Some("[General]\nsIntroSequence=\n")).is_empty(), "set to nothing: none");
        assert!(start_sequence(GameKind::Fallout3NV, Some("[General]\nSIntroSequence=\nsIntroMovie=Fallout INTRO Vsk.bik\n")).is_empty());
        assert_eq!(start_sequence(GameKind::Fallout4, None), ["GameIntro_V3_B.bk2"]);
        assert_eq!(start_sequence(GameKind::Starfield, None), ["BGS_LOGO_1080p_BinkVersion.bk2"]);
        assert_eq!(ini_general("[General]\n; sIntroSequence=x\n[Other]\nsIntroSequence=y\n", "sIntroSequence"), None, "comments and other sections");
    }

    /// BIK-REQ-005 — `--skip-movies` skips the start sequence; a benchmark skips it unless
    /// `--start-movies` asks for it.
    #[test]
    fn the_start_sequence_honours_the_skip_flags() {
        let args = |list: &[&str]| list.iter().map(|a| a.to_string()).collect::<Vec<_>>();
        assert!(start_movies_wanted(&args(&["byroredux", "--esm", "x.esm"])));
        assert!(!start_movies_wanted(&args(&["byroredux", "--skip-movies"])));
        assert!(!start_movies_wanted(&args(&["byroredux", "--bench-frames", "30"])));
        assert!(start_movies_wanted(&args(&["byroredux", "--bench-frames", "30", "--start-movies"])));
        assert!(!start_movies_wanted(&args(&["byroredux", "--bench-frames", "30", "--start-movies", "--skip-movies"])));
    }
}
