//! Movie decoding for the games' own videos (PRD `bink-video-playback`, #6855).
//!
//! The decoder is the system FFmpeg, reached at run time through its command-line tools
//! (BIK-DEC-005): `ffprobe` measures a file, one `ffmpeg` process decodes its video to raw RGBA
//! frames on a pipe, and a second extracts its audio to a temporary WAV the audio crate streams.
//! Nothing is linked, so the MIT workspace carries no FFmpeg code; a machine without FFmpeg
//! reports [`VideoError::NoFfmpeg`] and the caller marks the movie *skipped*.
//!
//! [`Movie::advance`] is the whole playback contract: the caller owns the clock and asks which
//! frame belongs at a given elapsed time; a decoder that falls behind holds the last frame, one
//! that runs ahead is held back by a small bounded ring.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc::{sync_channel, Receiver, TryRecvError};
use std::thread::JoinHandle;

/// Decoded frames the reader thread may run ahead of playback.
const FRAME_RING: usize = 6;

/// What a movie request cannot do.
#[derive(Debug, thiserror::Error)]
pub enum VideoError {
    /// `ffmpeg` / `ffprobe` is not installed or not on `PATH`.
    #[error("FFmpeg is not installed (ffmpeg / ffprobe not found on PATH)")]
    NoFfmpeg,
    /// The file is missing or is no movie FFmpeg can read.
    #[error("cannot read the movie: {0}")]
    Unreadable(String),
    /// FFmpeg started and failed.
    #[error("FFmpeg failed: {0}")]
    Decode(String),
}

/// What `ffprobe` measures about a movie.
#[derive(Debug, Clone, PartialEq)]
pub struct MovieInfo {
    pub width: u32,
    pub height: u32,
    pub fps: f64,
    pub duration_secs: f64,
    pub audio_tracks: u32,
}

/// Whether the FFmpeg tools are installed.
pub fn ffmpeg_available() -> bool {
    tool_works("ffmpeg") && tool_works("ffprobe")
}

fn tool_works(tool: &str) -> bool {
    Command::new(tool)
        .arg("-version")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

fn run_tool(tool: &str, args: &[&str], path: &Path) -> Result<String, VideoError> {
    let output = Command::new(tool)
        .args(args)
        .arg(path)
        .stdin(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .map_err(|error| match error.kind() {
            std::io::ErrorKind::NotFound => VideoError::NoFfmpeg,
            _ => VideoError::Decode(format!("{tool}: {error}")),
        })?;
    if !output.status.success() {
        return Err(VideoError::Unreadable(String::from_utf8_lossy(&output.stderr).trim().to_owned()));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// `10000000/333333` (or `30`) as frames per second; `0.0` when it is neither.
fn parse_rate(rate: &str) -> f64 {
    match rate.split_once('/') {
        Some((numerator, denominator)) => {
            let (n, d) = (numerator.trim().parse::<f64>().unwrap_or(0.0), denominator.trim().parse::<f64>().unwrap_or(0.0));
            if d > 0.0 {
                n / d
            } else {
                0.0
            }
        }
        None => rate.trim().parse().unwrap_or(0.0),
    }
}

/// Measure `path`: its first video stream's size and frame rate, its length and its audio tracks.
pub fn probe(path: &Path) -> Result<MovieInfo, VideoError> {
    let video = run_tool(
        "ffprobe",
        &["-v", "error", "-select_streams", "v:0", "-show_entries", "stream=width,height,r_frame_rate:format=duration", "-of", "default=noprint_wrappers=1"],
        path,
    )?;
    let field = |key: &str| video.lines().find_map(|line| line.strip_prefix(key)?.strip_prefix('=')).map(str::trim);
    let width = field("width").and_then(|v| v.parse().ok());
    let height = field("height").and_then(|v| v.parse().ok());
    let (Some(width), Some(height)) = (width, height) else {
        return Err(VideoError::Unreadable("no video stream".into()));
    };
    let audio = run_tool("ffprobe", &["-v", "error", "-select_streams", "a", "-show_entries", "stream=index", "-of", "csv=p=0"], path)?;
    Ok(MovieInfo {
        width,
        height,
        fps: field("r_frame_rate").map_or(0.0, parse_rate),
        duration_secs: field("duration").and_then(|v| v.parse().ok()).unwrap_or(0.0),
        audio_tracks: audio.lines().filter(|line| !line.trim().is_empty()).count() as u32,
    })
}

/// Where a movie's decoded audio waits: one file per movie in the temp directory.
fn temp_wav_path() -> PathBuf {
    static NEXT: AtomicU32 = AtomicU32::new(0);
    std::env::temp_dir().join(format!("byroredux-movie-{}-{}.wav", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)))
}

/// Decode `path`'s first audio track to 48 kHz stereo PCM in a temporary WAV.
fn extract_audio(path: &Path) -> Result<PathBuf, VideoError> {
    let wav = temp_wav_path();
    let status = Command::new("ffmpeg")
        .args(["-v", "error", "-nostdin", "-y", "-i"])
        .arg(path)
        .args(["-map", "0:a:0", "-vn", "-ac", "2", "-ar", "48000", "-c:a", "pcm_s16le"])
        .arg(&wav)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|error| match error.kind() {
            std::io::ErrorKind::NotFound => VideoError::NoFfmpeg,
            _ => VideoError::Decode(format!("ffmpeg: {error}")),
        })?;
    if status.success() {
        Ok(wav)
    } else {
        let _ = std::fs::remove_file(&wav);
        Err(VideoError::Decode("audio extraction failed".into()))
    }
}

/// What a movie shows at a given moment.
#[derive(Debug, PartialEq)]
pub enum Playhead<'a> {
    /// The decoder has produced nothing yet.
    Waiting,
    /// Frame `index` is the one to show; `new` is whether it changed since the last call.
    Frame { index: u64, pixels: &'a [u8], new: bool },
    /// Every frame has been shown.
    Finished,
}

/// One movie being decoded.
pub struct Movie {
    info: MovieInfo,
    size: (u32, u32),
    rx: Option<Receiver<Vec<u8>>>,
    reader: Option<JoinHandle<()>>,
    child: Option<Child>,
    audio: Option<PathBuf>,
    frame: Vec<u8>,
    index: i64,
    exhausted: bool,
}

impl Movie {
    /// Start decoding `path` into `size` RGBA frames: fitted inside it with black bars when
    /// `letterbox`, stretched to fill it otherwise. Blocks while the audio track is extracted.
    pub fn open(path: &Path, size: (u32, u32), letterbox: bool) -> Result<Self, VideoError> {
        let info = probe(path)?;
        let (width, height) = (size.0.max(2), size.1.max(2));
        let filter = if letterbox {
            format!("scale={width}:{height}:force_original_aspect_ratio=decrease,pad={width}:{height}:(ow-iw)/2:(oh-ih)/2:black")
        } else {
            format!("scale={width}:{height}")
        };
        let audio = if info.audio_tracks > 0 { extract_audio(path).ok() } else { None };
        let mut child = Command::new("ffmpeg")
            .args(["-v", "error", "-nostdin", "-i"])
            .arg(path)
            .args(["-an", "-vf", &filter, "-pix_fmt", "rgba", "-f", "rawvideo", "-"])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|error| match error.kind() {
                std::io::ErrorKind::NotFound => VideoError::NoFfmpeg,
                _ => VideoError::Decode(format!("ffmpeg: {error}")),
            })?;
        let mut stdout = child.stdout.take().ok_or_else(|| VideoError::Decode("no decoder pipe".into()))?;
        let frame_bytes = width as usize * height as usize * 4;
        let (tx, rx) = sync_channel::<Vec<u8>>(FRAME_RING);
        let reader = std::thread::Builder::new()
            .name("movie-decode".into())
            .spawn(move || loop {
                let mut frame = vec![0u8; frame_bytes];
                if stdout.read_exact(&mut frame).is_err() || tx.send(frame).is_err() {
                    break;
                }
            })
            .map_err(|error| VideoError::Decode(format!("decoder thread: {error}")))?;
        Ok(Self { info, size: (width, height), rx: Some(rx), reader: Some(reader), child: Some(child), audio, frame: Vec::new(), index: -1, exhausted: false })
    }

    /// A movie over frames that already exist (tests).
    pub fn from_frames(info: MovieInfo, size: (u32, u32), frames: Vec<Vec<u8>>) -> Self {
        let (tx, rx) = sync_channel(frames.len().max(1));
        for frame in frames {
            tx.send(frame).expect("the ring holds every frame");
        }
        Self { info, size, rx: Some(rx), reader: None, child: None, audio: None, frame: Vec::new(), index: -1, exhausted: false }
    }

    pub fn info(&self) -> &MovieInfo {
        &self.info
    }

    /// The decoded frames' size.
    pub fn size(&self) -> (u32, u32) {
        self.size
    }

    /// The temporary WAV holding the first audio track, if the movie has one.
    pub fn audio_path(&self) -> Option<&Path> {
        self.audio.as_deref()
    }

    /// Frames shown so far (the highest index reached plus one).
    pub fn frames_shown(&self) -> u64 {
        (self.index + 1).max(0) as u64
    }

    /// Decode every remaining frame as fast as the decoder produces them, returning how many the
    /// movie held in all. For measuring a corpus (BIK-G-002), not for playback.
    pub fn decode_to_end(&mut self) -> u64 {
        if let Some(rx) = &self.rx {
            while let Ok(frame) = rx.recv() {
                self.frame = frame;
                self.index += 1;
            }
        }
        self.exhausted = true;
        (self.index + 1).max(0) as u64
    }

    /// The frame that belongs `elapsed_secs` into the movie. Frames the clock has passed are
    /// dropped; a decoder that has not produced the wanted frame yet leaves the last one up.
    pub fn advance(&mut self, elapsed_secs: f64) -> Playhead<'_> {
        let fps = if self.info.fps > 0.0 { self.info.fps } else { 25.0 };
        let target = (elapsed_secs.max(0.0) * fps).floor() as i64;
        let before = self.index;
        while self.index < target && !self.exhausted {
            let Some(rx) = &self.rx else {
                self.exhausted = true;
                break;
            };
            match rx.try_recv() {
                Ok(frame) => {
                    self.frame = frame;
                    self.index += 1;
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => self.exhausted = true,
            }
        }
        if self.exhausted && self.index < target {
            return Playhead::Finished;
        }
        if self.index < 0 {
            return Playhead::Waiting;
        }
        Playhead::Frame { index: self.index as u64, pixels: &self.frame, new: self.index != before }
    }
}

impl Drop for Movie {
    fn drop(&mut self) {
        // Close the ring first: a reader blocked in `send` wakes with an error. Then kill the
        // decoder, which closes the pipe a reader blocked in `read` waits on.
        self.rx = None;
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
        if let Some(wav) = self.audio.take() {
            let _ = std::fs::remove_file(wav);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(fps: f64) -> MovieInfo {
        MovieInfo { width: 2, height: 2, fps, duration_secs: 0.0, audio_tracks: 0 }
    }

    #[test]
    fn frame_rates_parse_as_fractions_and_plain_numbers() {
        assert!((parse_rate("10000000/333333") - 30.0).abs() < 0.001, "FNVIntro.bik's rate");
        assert!((parse_rate("24000/1001") - 23.976).abs() < 0.001);
        assert_eq!(parse_rate("25"), 25.0);
        assert_eq!(parse_rate("0/0"), 0.0);
        assert_eq!(parse_rate("garbage"), 0.0);
    }

    /// The clock picks the frame; frames it has passed are dropped; one it has not reached is
    /// held back; running out of frames ends the movie only once the clock wants a frame
    /// that does not exist.
    #[test]
    fn the_clock_picks_the_frame_and_the_last_frame_is_held_until_the_clock_passes_it() {
        let mut movie = Movie::from_frames(info(10.0), (2, 2), (0..5u8).map(|n| vec![n; 16]).collect());
        assert_eq!(movie.advance(0.0), Playhead::Frame { index: 0, pixels: &[0u8; 16], new: true });
        assert_eq!(movie.advance(0.05), Playhead::Frame { index: 0, pixels: &[0u8; 16], new: false }, "still frame 0");
        assert_eq!(movie.advance(0.25), Playhead::Frame { index: 2, pixels: &[2u8; 16], new: true }, "frame 1 was dropped");
        assert_eq!(movie.frames_shown(), 3);
        assert_eq!(movie.advance(0.45), Playhead::Frame { index: 4, pixels: &[4u8; 16], new: true });
        assert_eq!(movie.advance(0.49), Playhead::Frame { index: 4, pixels: &[4u8; 16], new: false }, "the last frame holds");
        assert_eq!(movie.advance(0.5), Playhead::Finished, "the clock wants a sixth frame");
    }

    #[test]
    fn a_decoder_that_has_produced_nothing_is_waited_on() {
        let mut movie = Movie::from_frames(info(10.0), (2, 2), Vec::new());
        // A live channel with no frame yet is `Empty`; the stand-in's sender is already gone, so
        // build the waiting case by hand.
        let (tx, rx) = sync_channel::<Vec<u8>>(1);
        movie.rx = Some(rx);
        assert_eq!(movie.advance(0.0), Playhead::Waiting);
        tx.send(vec![7; 16]).unwrap();
        assert_eq!(movie.advance(0.0), Playhead::Frame { index: 0, pixels: &[7u8; 16], new: true });
        drop(tx);
        assert_eq!(movie.advance(0.2), Playhead::Finished, "the decoder closed before frame 2");
    }

    /// BIK-G-002 / acceptance scenario 1: every Bink 1 movie (`BIK` magic) of every installed
    /// game decodes to its last frame, and the frame count agrees with the probed length.
    /// `BYROREDUX_STEAM_COMMON=<steamapps/common> cargo test -p byroredux-video --release --
    /// --ignored --nocapture` (needs FFmpeg and the games).
    #[test]
    #[ignore = "needs FFmpeg and the games' Data\\Video folders"]
    fn every_installed_bink_1_movie_decodes_to_its_last_frame() {
        assert!(ffmpeg_available(), "this test needs the FFmpeg tools");
        let Ok(common) = std::env::var("BYROREDUX_STEAM_COMMON") else {
            panic!("set BYROREDUX_STEAM_COMMON to the Steam library's steamapps/common folder");
        };
        let (mut decoded, mut failed) = (0u32, Vec::new());
        for (game, folder) in [
            ("Oblivion", "Data/Video"),
            ("Fallout 3 goty", "Data/Video"),
            ("Fallout New Vegas", "Data/Video"),
            ("Skyrim Special Edition", "Data/Video"),
            ("Fallout 4", "Data/Video"),
        ] {
            let Ok(entries) = std::fs::read_dir(Path::new(&common).join(game).join(folder)) else { continue };
            let mut files: Vec<_> = entries.flatten().map(|entry| entry.path()).collect();
            files.sort();
            for file in files {
                let mut magic = [0u8; 3];
                if std::fs::File::open(&file).and_then(|mut f| f.read_exact(&mut magic)).is_err() || &magic != b"BIK" {
                    continue;
                }
                let outcome = Movie::open(&file, (320, 180), true).map(|mut movie| {
                    let expected = (movie.info().duration_secs * movie.info().fps).round() as i64;
                    (movie.decode_to_end(), expected)
                });
                match outcome {
                    Ok((frames, expected)) if frames > 0 && (frames as i64 - expected).abs() <= 3 => {
                        decoded += 1;
                        println!("ok   {game}/{}: {frames} frames (probed {expected})", file.file_name().unwrap().to_string_lossy());
                    }
                    other => failed.push(format!("{game}/{}: {other:?}", file.file_name().unwrap().to_string_lossy())),
                }
            }
        }
        println!("{decoded} Bink 1 movies decoded");
        assert!(failed.is_empty(), "movies that did not decode to their last frame: {failed:#?}");
        assert!(decoded > 0, "no Bink 1 movie was found");
    }

    /// Needs the FFmpeg tools; passes trivially without them (the movie would be skipped).
    #[test]
    fn a_generated_movie_probes_decodes_letterboxed_and_yields_its_audio() {
        if !ffmpeg_available() {
            eprintln!("skipped: FFmpeg is not installed");
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("clip.mkv");
        let made = Command::new("ffmpeg")
            .args(["-v", "error", "-y", "-f", "lavfi", "-i", "testsrc=size=64x48:rate=10:duration=1", "-f", "lavfi", "-i", "sine=frequency=440:duration=1"])
            .args(["-c:v", "mpeg4", "-c:a", "pcm_s16le"])
            .arg(&file)
            .status()
            .expect("ffmpeg runs");
        assert!(made.success(), "the test clip is generated");

        let found = probe(&file).unwrap();
        assert_eq!((found.width, found.height, found.audio_tracks), (64, 48, 1));
        assert!((found.fps - 10.0).abs() < 0.01 && (found.duration_secs - 1.0).abs() < 0.2, "{found:?}");

        // 4:3 into a square window: black bars above and below.
        let mut movie = Movie::open(&file, (96, 96), true).unwrap();
        assert_eq!(movie.size(), (96, 96));
        let wav = movie.audio_path().map(Path::to_path_buf).expect("the audio track is extracted");
        assert!(std::fs::metadata(&wav).unwrap().len() > 44, "a WAV with samples");
        let started = std::time::Instant::now();
        let mut shown = 0u64;
        let mut first_row_black = false;
        loop {
            assert!(started.elapsed().as_secs() < 20, "the movie never finished");
            match movie.advance(started.elapsed().as_secs_f64()) {
                Playhead::Frame { pixels, new, .. } => {
                    if new && shown == 0 {
                        first_row_black = pixels[..96 * 4].chunks(4).all(|px| px[..3] == [0, 0, 0]);
                    }
                    shown += u64::from(new);
                }
                Playhead::Waiting => {}
                Playhead::Finished => break,
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert!(first_row_black, "letterboxing pads the top");
        assert!((5..=11).contains(&shown), "about ten frames shown at ten fps, got {shown}");
        drop(movie);
        assert!(!wav.exists(), "dropping the movie removes its audio file");
    }
}
