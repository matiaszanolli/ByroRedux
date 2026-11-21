//! Movie decoding for the games' own videos (PRD `bink-video-playback`, #6855).
//!
//! Bink 1 (`BIKi`) video and audio are decoded in process by the pure-Rust, MIT-licensed
//! `mercs2_bink` crate (BIK-DEC-005): no external tool is needed and nothing foreign is linked.
//! Bink 2 (`KB2g`, `KB2j`) video and audio are decoded by the workspace's clean-room
//! `byroredux-bink2` and `byroredux-binkaudio` (BIK-DEC-003). Other Bink 1 and Bink 2 revisions are
//! reported [`VideoError::Unsupported`] and the caller marks the movie *skipped*.
//!
//! A reader thread decodes frames, converts them to RGBA with each decoder's converter and fits
//! them to the window; the movie's first audio track is decoded up front into a temporary WAV the
//! audio crate streams. [`Movie::advance`] is the whole playback contract: the caller owns the clock and asks
//! which frame belongs at a given elapsed time; a decoder that falls behind holds the last
//! frame, one that runs ahead is held back by a small bounded ring.

use std::io::{Read, Seek, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc::{sync_channel, Receiver, SyncSender, TryRecvError};
use std::thread::JoinHandle;

use byroredux_bink2 as bink2;
use mercs2_bink::{AudioDecoder, VideoDecoder};

/// Decoded frames the reader thread may run ahead of playback.
const FRAME_RING: usize = 6;

/// The Bink 2 revisions `byroredux-bink2` decodes (the last byte of the magic): its own list, which
/// `Bk2::open` also enforces (#7134). Checked here first so another revision is reported
/// `Unsupported`, not as a damaged file.
const BINK2_REVISIONS: &[u8] = bink2::REVISIONS;

/// Threads a Bink 2 movie's slices decode on (its frames have at most 4 slices).
const BINK2_THREADS: usize = 4;

/// What a movie request cannot do.
#[derive(Debug, thiserror::Error)]
pub enum VideoError {
    /// The movie is in a format this build has no decoder for: a Bink 1 revision other than `i` or a
    /// Bink 1 alpha or grayscale plane, a Bink 2 revision other than `g` and `j`, or a Bink 2 video
    /// flag `byroredux-bink2` does not model (#7135).
    #[error("{0}")]
    Unsupported(String),
    /// The file is missing or is no movie.
    #[error("cannot read the movie: {0}")]
    Unreadable(String),
    /// Decoding started and failed.
    #[error("decoding failed: {0}")]
    Decode(String),
}

/// What the container header says about a movie.
#[derive(Debug, Clone, PartialEq)]
pub struct MovieInfo {
    pub width: u32,
    pub height: u32,
    pub fps: f64,
    pub duration_secs: f64,
    pub audio_tracks: u32,
}

/// A parse error, sorted into what the caller does with it: Bink revisions without a decoder are
/// *unsupported*, anything else is a damaged file.
fn container_error(magic: &[u8], detail: String) -> VideoError {
    match magic {
        [b'K', b'B', b'2', revision, ..] if !BINK2_REVISIONS.contains(revision) => {
            VideoError::Unsupported(format!("Bink 2 revision '{}' has no decoder in this build", char::from(*revision)))
        }
        [b'B', b'I', b'K', revision, ..] if *revision != b'i' => {
            VideoError::Unsupported(format!("Bink 1 revision '{}' has no decoder in this build", char::from(*revision)))
        }
        _ => VideoError::Unreadable(detail),
    }
}

/// #7123 — a Bink 1 header must not size the decoder's planes past what a Bink 2 header may: the
/// dependency allows 0x7fff a side, so a 100-byte file could ask for ~1 GiB planes, and a failed
/// allocation aborts the process where no `catch_unwind` reaches. Checked after `Movie::parse`,
/// before `VideoDecoder::new`, with the same ceilings `byroredux-bink2` applies at `Bk2::open`.
fn check_bink1_picture(movie: &mercs2_bink::Movie) -> Result<(), VideoError> {
    let (width, height) = (movie.width as usize, movie.height as usize);
    if width > bink2::MAX_DIMENSION || height > bink2::MAX_DIMENSION || width * height > bink2::MAX_PIXELS {
        return Err(VideoError::Unreadable(format!(
            "a {width}x{height} Bink 1 picture is past the {}-a-side, {}-pixel limit",
            bink2::MAX_DIMENSION,
            bink2::MAX_PIXELS
        )));
    }
    Ok(())
}

fn describe(movie: &mercs2_bink::Movie) -> MovieInfo {
    let fps = if movie.fps.1 > 0 { f64::from(movie.fps.0) / f64::from(movie.fps.1) } else { 0.0 };
    MovieInfo {
        width: movie.width,
        height: movie.height,
        fps,
        duration_secs: if fps > 0.0 { f64::from(movie.frame_count) / fps } else { 0.0 },
        audio_tracks: movie.tracks.len() as u32,
    }
}

/// A Bink 2 movie's container, for a revision `byroredux-bink2` decodes.
fn bink2_container(path: &Path, magic: &[u8]) -> Result<bink2::Bk2, VideoError> {
    if !BINK2_REVISIONS.contains(&magic[3]) {
        return Err(container_error(magic, String::new()));
    }
    let file = bink2::Bk2::open(path).map_err(|error| match error.kind() {
        std::io::ErrorKind::Unsupported => VideoError::Unsupported(error.to_string()),
        _ => VideoError::Unreadable(error.to_string()),
    })?;
    if file.width == 0 || file.height == 0 || file.frames == 0 || file.fps_num == 0 || file.fps_den == 0 {
        return Err(VideoError::Unreadable(format!(
            "not a playable Bink 2 movie: {}x{}, {} frames at {}/{} fps",
            file.width, file.height, file.frames, file.fps_num, file.fps_den
        )));
    }
    Ok(file)
}

fn describe_bink2(file: &bink2::Bk2) -> MovieInfo {
    let fps = f64::from(file.fps_num) / f64::from(file.fps_den);
    MovieInfo {
        width: file.width as u32,
        height: file.height as u32,
        fps,
        duration_secs: file.frames as f64 / fps,
        audio_tracks: file.tracks.len() as u32,
    }
}

/// The file's first four bytes.
fn magic(path: &Path) -> Result<[u8; 4], VideoError> {
    let mut magic = [0u8; 4];
    std::fs::File::open(path)
        .and_then(|mut file| file.read_exact(&mut magic))
        .map_err(|error| VideoError::Unreadable(error.to_string()))?;
    Ok(magic)
}

/// Bink 1 header layout `probe` reads: the frame count, the audio-track count, the header length,
/// then one 12-byte record per track and a `frames + 1` table of 4-byte frame offsets.
const BIK1_FRAME_COUNT_AT: usize = 0x10;
const BIK1_TRACK_COUNT_AT: usize = 0x28;
const BIK1_HEADER_LEN: usize = 0x2c;
const BIK1_TRACK_RECORD_LEN: usize = 12;
const BIK1_INDEX_ENTRY_LEN: usize = 4;

/// Measure `path` from its container header alone: nothing of the picture is read.
pub fn probe(path: &Path) -> Result<MovieInfo, VideoError> {
    guard_open(path, || probe_unguarded(path))
}

fn probe_unguarded(path: &Path) -> Result<MovieInfo, VideoError> {
    let magic = magic(path)?;
    if &magic[..3] == b"KB2" {
        return bink2_container(path, &magic).map(|file| describe_bink2(&file));
    }
    let mut file = std::fs::File::open(path).map_err(|error| VideoError::Unreadable(error.to_string()))?;
    // The frame-offset table ends the part the parser reads.
    let mut head = [0u8; BIK1_HEADER_LEN];
    file.read_exact(&mut head).map_err(|error| VideoError::Unreadable(error.to_string()))?;
    if &head[..4] != b"BIKi" {
        return Err(container_error(&head, format!("not a Bink 1 movie: magic {:02x?}", &head[..4])));
    }
    let word = |at: usize| u32::from_le_bytes([head[at], head[at + 1], head[at + 2], head[at + 3]]) as usize;
    // The counts come from an untrusted header: clamp them to the Bink 2 container's ceilings.
    let tracks = word(BIK1_TRACK_COUNT_AT).min(bink2::MAX_AUDIO_TRACKS);
    let frames = word(BIK1_FRAME_COUNT_AT).min(bink2::MAX_FRAMES);
    let table_end = BIK1_HEADER_LEN + BIK1_TRACK_RECORD_LEN * tracks + BIK1_INDEX_ENTRY_LEN * (frames + 1);
    let mut bytes = head.to_vec();
    bytes.resize(table_end, 0);
    file.read_exact(&mut bytes[BIK1_HEADER_LEN..]).map_err(|error| VideoError::Unreadable(error.to_string()))?;
    match mercs2_bink::Movie::parse(&bytes) {
        Ok(movie) => check_bink1_picture(&movie).map(|()| describe(&movie)),
        Err(detail) => Err(container_error(&bytes, detail)),
    }
}

/// Where a movie's decoded audio waits: one file per movie in the temp directory.
fn temp_wav_path() -> PathBuf {
    static NEXT: AtomicU32 = AtomicU32::new(0);
    std::env::temp_dir().join(format!("byroredux-movie-{}-{}.wav", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)))
}

/// A canonical 44-byte WAV header for 16-bit PCM.
fn wav_header(samples: usize, channels: u16, rate: u32) -> [u8; 44] {
    let data = (samples * 2) as u32;
    let mut h = [0u8; 44];
    h[0..4].copy_from_slice(b"RIFF");
    h[4..8].copy_from_slice(&(36 + data).to_le_bytes());
    h[8..16].copy_from_slice(b"WAVEfmt ");
    h[16..20].copy_from_slice(&16u32.to_le_bytes());
    h[20..22].copy_from_slice(&1u16.to_le_bytes());
    h[22..24].copy_from_slice(&channels.to_le_bytes());
    h[24..28].copy_from_slice(&rate.to_le_bytes());
    h[28..32].copy_from_slice(&(rate * u32::from(channels) * 2).to_le_bytes());
    h[32..34].copy_from_slice(&(channels * 2).to_le_bytes());
    h[34..36].copy_from_slice(&16u16.to_le_bytes());
    h[36..40].copy_from_slice(b"data");
    h[40..44].copy_from_slice(&data.to_le_bytes());
    h
}

/// The longest soundtrack decoded up front `[ours]`: an hour, twelve times the longest installed
/// movie (`FNVIntro.bik`, 290 s, measured 2026-11-15 over every game's `Data\Video`).
const MAX_SOUNDTRACK_SECS: f64 = 3600.0;
/// The most samples any soundtrack decodes to, whatever its header says: an hour of 48 kHz stereo
/// (the highest rate in the corpus), 691 MB of PCM, inside the WAV header's `u32` byte count.
const MAX_SOUNDTRACK_SAMPLES: usize = MAX_SOUNDTRACK_SECS as usize * 48_000 * 2;

/// The most interleaved samples a movie's soundtrack may decode to: its running time plus two
/// seconds, with half again as slack, at the track's rate and channel count, and never past
/// [`MAX_SOUNDTRACK_SAMPLES`] `[ours]`. Every term of the running time is a header field, so the
/// absolute ceiling is what bounds a hostile file (BIK-AUD-1: an fps of 1/u32::MAX overflowed
/// the old product, and a 1M-frame header allowed ~9.6 GB); the per-movie term only keeps an honest
/// short movie's buffer tight. Saturating throughout.
fn pcm_cap(duration_secs: f64, rate: u32, channels: usize) -> usize {
    let secs = if duration_secs.is_finite() { duration_secs.clamp(0.0, MAX_SOUNDTRACK_SECS) } else { 0.0 };
    let per_movie = (((secs + 2.0) * 1.5 * f64::from(rate)) as usize).saturating_mul(channels);
    per_movie.min(MAX_SOUNDTRACK_SAMPLES)
}

/// How far past its own duration one frame's audio may run: Bink front-loads preroll into frame 0,
/// 0.75 s over the frame in every installed movie with sound (58 files across the seven titles,
/// measured 2026-11-20), so two seconds is 2.7x that `[ours]`.
const FRAME_AUDIO_SLACK_SECS: f64 = 2.0;
/// The longest a frame counts as lasting when sizing its audio `[ours]`: no installed movie runs
/// below 23.976 fps, and the frame rate is a header field, so a near-zero one must not hand a single
/// frame the whole soundtrack.
const MAX_FRAME_SECS: f64 = 1.0;

/// The most interleaved samples one frame's audio may decode to (#7125): its duration, at most
/// [`MAX_FRAME_SECS`], plus [`FRAME_AUDIO_SLACK_SECS`], at the track's rate and channel count, never
/// past [`MAX_SOUNDTRACK_SAMPLES`]. [`pcm_cap`] bounds the movie; this keeps one packet (a zero-filled
/// one decodes ~100x its size) from spending that whole budget in one buffer.
fn frame_pcm_cap(fps: f64, rate: u32, channels: usize) -> usize {
    let frame_secs = if fps.is_finite() && fps > 0.0 { (1.0 / fps).min(MAX_FRAME_SECS) } else { 0.0 };
    let samples = (((frame_secs + FRAME_AUDIO_SLACK_SECS) * f64::from(rate)) as usize).saturating_mul(channels);
    samples.min(MAX_SOUNDTRACK_SAMPLES)
}

/// What a soundtrack decodes under: the format of its WAV, and the movie's ([`pcm_cap`]) and one
/// frame's ([`frame_pcm_cap`]) sample limits.
#[derive(Clone, Copy)]
struct Soundtrack {
    channels: u16,
    rate: u32,
    total_cap: usize,
    frame_cap: usize,
}

impl Soundtrack {
    fn new(info: &MovieInfo, channels: u16, rate: u32) -> Self {
        Self {
            channels,
            rate,
            total_cap: pcm_cap(info.duration_secs, rate, usize::from(channels)),
            frame_cap: frame_pcm_cap(info.fps, rate, usize::from(channels)),
        }
    }
}

/// Decode the movie's first audio track to a temporary WAV. `None` when it has no audio or the
/// track does not decode (the picture then plays silent, with a warning).
fn extract_audio(movie: &mercs2_bink::Movie, name: &str) -> Option<PathBuf> {
    let track = movie.tracks.first()?;
    let mut decoder = match AudioDecoder::new(track) {
        Ok(decoder) => decoder,
        Err(error) => {
            log::warn!("video: {name}: its first audio track cannot be decoded: {error}");
            return None;
        }
    };
    let soundtrack = Soundtrack::new(&describe(movie), track.channels() as u16, track.rate());
    decode_soundtrack(name, movie.frame_count as usize, soundtrack, |frame, pcm| {
        let record = movie.split(frame as _).ok().and_then(|(records, _)| records.first().copied());
        let Some(record) = record else { return Ok(false) };
        pcm.extend(decoder.decode_record(record).map_err(|error| error.to_string())?);
        Ok(true)
    })
}

/// Decode a soundtrack frame by frame straight into a temporary WAV the audio crate streams, for
/// every codec: `decode_frame` appends frame `n`'s samples to an emptied buffer and answers whether
/// to go on (`Ok(false)` ends the track quietly, an `Err` ends it with a warning). A frame past the
/// frame cap ends the track before it is written, and the WAV never passes the movie's cap, so
/// memory holds one frame's audio, not the soundtrack (#7125). `None` when nothing decoded or the
/// file cannot be written (the picture then plays silent, with a warning).
fn decode_soundtrack(
    name: &str,
    frames: usize,
    soundtrack: Soundtrack,
    mut decode_frame: impl FnMut(usize, &mut Vec<i16>) -> Result<bool, String>,
) -> Option<PathBuf> {
    let path = temp_wav_path();
    let mut written = 0usize;
    let result = std::fs::File::create(&path).and_then(|file| {
        let mut out = std::io::BufWriter::new(file);
        out.write_all(&wav_header(0, soundtrack.channels, soundtrack.rate))?;
        let mut pcm: Vec<i16> = Vec::new();
        for frame in 0..frames {
            pcm.clear();
            let go_on = match decode_frame(frame, &mut pcm) {
                Ok(go_on) => go_on,
                Err(error) => {
                    log::warn!("video: {name}: its audio stops at frame {frame}: {error}");
                    false
                }
            };
            if pcm.len() > soundtrack.frame_cap {
                log::warn!(
                    "video: {name}: frame {frame}'s audio decodes to {} samples, past the {} one frame may hold; \
                     its audio stops there",
                    pcm.len(),
                    soundtrack.frame_cap
                );
                break;
            }
            let room = soundtrack.total_cap - written;
            let over = pcm.len() > room;
            if over {
                log::warn!(
                    "video: {name}: its audio decodes past the movie's running time; stopped at {} samples",
                    soundtrack.total_cap
                );
                pcm.truncate(room);
            }
            for sample in &pcm {
                out.write_all(&sample.to_le_bytes())?;
            }
            written += pcm.len();
            if over || !go_on {
                break;
            }
        }
        let mut file = out.into_inner().map_err(|error| error.into_error())?;
        file.seek(std::io::SeekFrom::Start(0))?;
        file.write_all(&wav_header(written, soundtrack.channels, soundtrack.rate))
    });
    match result {
        Ok(()) if written > 0 => Some(path),
        Ok(()) => {
            let _ = std::fs::remove_file(&path);
            None
        }
        Err(error) => {
            log::warn!("video: {name}: its soundtrack could not be written: {error}");
            let _ = std::fs::remove_file(&path);
            None
        }
    }
}

/// The Bink track ID the original plays for an English install (BIK-DEC-002, #6896). A Bink 2 file's
/// tracks are language versions of one mix (binkaudio A9) and carry the language's ID. Fallout 4
/// (`Fallout4.exe`, 2026-11-17) maps `sLanguage` to a track ID before `BinkSetSoundTrack`: `en` 1, `fr`
/// 2, `it` 3, `de` 4, `es` 5, `ja` 8, and every other language 1. The engine has no language setting, so
/// it plays the English track.
const ENGLISH_TRACK_ID: u32 = 1;

/// The index of the track the original plays: the one carrying [`ENGLISH_TRACK_ID`], else the first
/// (a file with no such ID, such as a mod's, plays its first track).
fn original_track_index(ids: impl IntoIterator<Item = u32>) -> usize {
    ids.into_iter().position(|id| id == ENGLISH_TRACK_ID).unwrap_or(0)
}

/// BIK-DEC-003: a Bink 2 movie's sound, one track decoded up front by `byroredux-binkaudio` into a
/// temporary WAV: the English one ([`original_track_index`], BIK-DEC-002), else the first. `None` without
/// tracks, or when the track does not decode (the picture then plays silent, with a warning).
fn bink2_audio(file: &mut bink2::Bk2, name: &str) -> Option<PathBuf> {
    let index = original_track_index(file.tracks.iter().map(|track| track.id));
    let track = *file.tracks.get(index)?;
    let mut decoder = match byroredux_binkaudio::Decoder::new(track.rate, track.stereo(), track.dct()) {
        Ok(decoder) => decoder,
        Err(error) => {
            log::warn!("video: {name}: its first audio track cannot be decoded: {error}");
            return None;
        }
    };
    let soundtrack = Soundtrack::new(&describe_bink2(file), track.channels() as u16, track.rate);
    let wav = decode_soundtrack(name, file.frames, soundtrack, |frame, pcm| {
        let packet = file.audio(frame).map_err(|error| error.to_string())?.swap_remove(index);
        let Some(packet) = packet else { return Ok(true) };
        // #7125 — refuse a packet stating more than one frame may hold before `decode` grows the
        // buffer for it: a zero-filled 13 MB packet could otherwise decode the whole soundtrack.
        if packet.decoded_bytes as usize / 2 > soundtrack.frame_cap {
            return Err(format!(
                "its packet states {} samples, past the {} one frame may hold",
                packet.decoded_bytes / 2,
                soundtrack.frame_cap
            ));
        }
        decoder.decode(packet.decoded_bytes, &packet.data, pcm).map_err(|error| error.to_string())?;
        Ok(true)
    });
    if file.tracks.len() > 1 {
        log::info!("video: {name}: sound from audio track ID {} (track {index} of {})", track.id, file.tracks.len());
    }
    wav
}

/// `rgba` (`src` pixels wide and high) fitted into a `dst` frame: centred inside it with black bars
/// when `letterbox`, stretched to fill it otherwise, bilinearly resampled.
fn fit(rgba: &[u8], src: (usize, usize), dst: (usize, usize), letterbox: bool) -> Vec<u8> {
    if src == dst {
        return rgba.to_vec();
    }
    let mut out = vec![0u8; dst.0 * dst.1 * 4];
    for pixel in out.as_chunks_mut::<4>().0 {
        pixel[3] = 255;
    }
    // The destination rectangle the picture fills.
    let (rw, rh) = if letterbox {
        let scale = (dst.0 as f64 / src.0 as f64).min(dst.1 as f64 / src.1 as f64);
        (((src.0 as f64 * scale).round() as usize).clamp(1, dst.0), ((src.1 as f64 * scale).round() as usize).clamp(1, dst.1))
    } else {
        dst
    };
    let (x0, y0) = ((dst.0 - rw) / 2, (dst.1 - rh) / 2);
    // Source column and 8-bit weight for each destination column, computed once.
    let columns: Vec<(usize, usize, u32)> = (0..rw)
        .map(|x| {
            let at = ((x as f64 + 0.5) * src.0 as f64 / rw as f64 - 0.5).clamp(0.0, (src.0 - 1) as f64);
            let left = at.floor() as usize;
            (left, (left + 1).min(src.0 - 1), ((at - left as f64) * 256.0) as u32)
        })
        .collect();
    for y in 0..rh {
        let at = ((y as f64 + 0.5) * src.1 as f64 / rh as f64 - 0.5).clamp(0.0, (src.1 - 1) as f64);
        let top = at.floor() as usize;
        let (bottom, wy) = ((top + 1).min(src.1 - 1), ((at - top as f64) * 256.0) as u32);
        let (row_a, row_b) = (&rgba[top * src.0 * 4..(top + 1) * src.0 * 4], &rgba[bottom * src.0 * 4..(bottom + 1) * src.0 * 4]);
        let line = &mut out[((y0 + y) * dst.0 + x0) * 4..((y0 + y) * dst.0 + x0 + rw) * 4];
        for (pixel, &(left, right, wx)) in line.as_chunks_mut::<4>().0.iter_mut().zip(&columns) {
            for channel in 0..3 {
                let a = row_a[left * 4 + channel] as u32 * (256 - wx) + row_a[right * 4 + channel] as u32 * wx;
                let b = row_b[left * 4 + channel] as u32 * (256 - wx) + row_b[right * 4 + channel] as u32 * wx;
                pixel[channel] = ((a * (256 - wy) + b * wy + 32768) >> 16) as u8;
            }
        }
    }
    out
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
    /// BIK-DEC-004 — a frame did not decode: the frames before it have been shown, and the movie
    /// ends here. Carries the decoder's reason, naming the file and the frame.
    Failed(&'a str),
}

/// What the reader thread hands the player: a fitted RGBA frame, or why the next one did not decode.
type DecodedFrame = Result<Vec<u8>, String>;

/// BIK-DEC-004 — run a reader thread's decode loop so that a decoder panic ends the movie as
/// `Failed` with a reason, like a frame that does not decode. Otherwise the panic only closes the
/// ring, and the player reads a decoder that died mid-movie as one that reached the last frame.
fn guard_decoder(name: &str, tx: &SyncSender<DecodedFrame>, decode: impl FnOnce()) {
    let Err(payload) = std::panic::catch_unwind(std::panic::AssertUnwindSafe(decode)) else {
        return;
    };
    let reason = format!("{name}: the decoder panicked: {}", panic_message(&*payload));
    log::warn!("video: {reason}");
    let _ = tx.send(Err(reason));
}

/// #7128 — the open path runs on the caller's thread (the engine's frame loop): the container
/// parse, the decoder's setup and the whole soundtrack decode. A panic in any of them ends the
/// request as [`VideoError::Decode`] with a reason, as [`guard_decoder`] ends a reader thread's, instead
/// of unwinding into the engine. [`Movie::open`] and [`probe`] run their bodies through it.
fn guard_open<T>(path: &Path, open: impl FnOnce() -> Result<T, VideoError>) -> Result<T, VideoError> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(open)).unwrap_or_else(|payload| {
        let reason = format!("{}: the decoder panicked while opening: {}", movie_name(path), panic_message(&*payload));
        log::warn!("video: {reason}");
        Err(VideoError::Decode(reason))
    })
}

/// A panic's message, when it carries one.
fn panic_message(payload: &(dyn std::any::Any + Send)) -> String {
    payload
        .downcast_ref::<&str>()
        .map(|message| (*message).to_owned())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "no message".to_owned())
}

/// Remove a soundtrack decoded for a movie that then failed to start, so its temp WAV is not left.
fn discard_wav(audio: &Option<PathBuf>) {
    if let Some(wav) = audio {
        let _ = std::fs::remove_file(wav);
    }
}

/// Start a movie's reader thread, the one place one is spawned. `decode` runs under
/// [`guard_decoder`] (BIK-DEC-004) with the movie's name and an `emit` that hands one frame, or the
/// reason the next did not decode, to the player's ring: a reason is logged, and `emit` answers
/// `false` once the player has dropped the ring, so the reader stops.
fn spawn_reader(
    name: String,
    decode: impl FnOnce(&str, &mut dyn FnMut(DecodedFrame) -> bool) + Send + 'static,
) -> Result<(Receiver<DecodedFrame>, JoinHandle<()>), VideoError> {
    let (tx, rx) = sync_channel::<DecodedFrame>(FRAME_RING);
    let reader = std::thread::Builder::new()
        .name("movie-decode".into())
        .spawn(move || {
            let panic_tx = tx.clone();
            guard_decoder(&name.clone(), &panic_tx, move || {
                let mut emit = |frame: DecodedFrame| {
                    if let Err(reason) = &frame {
                        log::warn!("video: {reason}");
                    }
                    tx.send(frame).is_ok()
                };
                decode(&name, &mut emit);
            });
        })
        .map_err(|error| VideoError::Decode(format!("decoder thread: {error}")))?;
    Ok((rx, reader))
}

/// The file name a movie's log lines and failure reasons carry.
fn movie_name(path: &Path) -> String {
    path.file_name().map_or_else(|| path.display().to_string(), |n| n.to_string_lossy().into_owned())
}

/// One movie being decoded.
pub struct Movie {
    info: MovieInfo,
    size: (u32, u32),
    rx: Option<Receiver<DecodedFrame>>,
    reader: Option<JoinHandle<()>>,
    audio: Option<PathBuf>,
    frame: Vec<u8>,
    index: i64,
    exhausted: bool,
    /// BIK-DEC-004 — why decoding stopped short, once the reader said so.
    failure: Option<String>,
}

impl Movie {
    /// Start decoding `path` into `size` RGBA frames: fitted inside it with black bars when
    /// `letterbox`, stretched to fill it otherwise. Blocks while the audio track is decoded.
    pub fn open(path: &Path, size: (u32, u32), letterbox: bool) -> Result<Self, VideoError> {
        guard_open(path, || Self::open_unguarded(path, size, letterbox))
    }

    fn open_unguarded(path: &Path, size: (u32, u32), letterbox: bool) -> Result<Self, VideoError> {
        let magic = magic(path)?;
        if &magic[..3] == b"KB2" {
            return Self::open_bink2(path, &magic, size, letterbox);
        }
        let name = movie_name(path);
        let bytes = std::fs::read(path).map_err(|error| VideoError::Unreadable(error.to_string()))?;
        let (info, audio, mut video) = {
            let movie = mercs2_bink::Movie::parse(&bytes).map_err(|detail| container_error(&bytes, detail))?;
            check_bink1_picture(&movie)?;
            let video = VideoDecoder::new(&movie).map_err(VideoError::Unsupported)?;
            (describe(&movie), extract_audio(&movie, &name), video)
        };
        let (width, height) = (size.0.max(2), size.1.max(2));
        let (rx, reader) = spawn_reader(name, move |name, emit| {
            let movie = match mercs2_bink::Movie::parse(&bytes) {
                Ok(movie) => movie,
                Err(error) => {
                    emit(Err(format!("{name}: the container does not re-parse: {error}")));
                    return;
                }
            };
            let target = (width as usize, height as usize);
            for index in 0..movie.frame_count {
                let frame = match video.decode(&movie, index) {
                    Ok(frame) => frame,
                    Err(error) => {
                        emit(Err(format!("{name}: frame {index} does not decode: {error}")));
                        return;
                    }
                };
                let rgba = mercs2_bink::rgb::to_rgba(frame);
                if !emit(Ok(fit(&rgba, (frame.width as usize, frame.height as usize), target, letterbox))) {
                    return;
                }
            }
        })
        .inspect_err(|_| discard_wav(&audio))?;
        Ok(Self::streaming(info, (width, height), rx, Some(reader), audio))
    }

    /// BIK-DEC-003: a Bink 2 movie, its first audio track decoded up front, its video packets read
    /// from the file as the reader thread needs them and its slices decoded on up to `BINK2_THREADS`
    /// threads.
    fn open_bink2(path: &Path, magic: &[u8], size: (u32, u32), letterbox: bool) -> Result<Self, VideoError> {
        let mut file = bink2_container(path, magic)?;
        let info = describe_bink2(&file);
        let name = movie_name(path);
        let audio = bink2_audio(&mut file, &name);
        let (width, height) = (size.0.max(2), size.1.max(2));
        let (rx, reader) = spawn_reader(name, move |name, emit| {
            let mut decoder = bink2::Decoder::for_file(&file);
            decoder.options.threads = std::thread::available_parallelism().map_or(1, |n| n.get()).min(BINK2_THREADS);
            let (source, target) = ((file.width, file.height), (width as usize, height as usize));
            let mut rgba = vec![0u8; source.0 * source.1 * 4];
            for index in 0..file.frames {
                let decoded = match file.video(index) {
                    Ok(packet) => decoder.decode(&packet, file.is_key(index)).map_err(|error| error.to_string()),
                    Err(error) => Err(error.to_string()),
                };
                match decoded {
                    Ok(frame) => frame.rgba_into(&mut rgba),
                    Err(error) => {
                        emit(Err(format!("{name}: frame {index} does not decode: {error}")));
                        return;
                    }
                }
                if !emit(Ok(fit(&rgba, source, target, letterbox))) {
                    return;
                }
            }
        })
        .inspect_err(|_| discard_wav(&audio))?;
        Ok(Self::streaming(info, (width, height), rx, Some(reader), audio))
    }

    /// A movie read from `rx`, which `reader` (when there is one) fills.
    fn streaming(
        info: MovieInfo,
        size: (u32, u32),
        rx: Receiver<DecodedFrame>,
        reader: Option<JoinHandle<()>>,
        audio: Option<PathBuf>,
    ) -> Self {
        Self { info, size, rx: Some(rx), reader, audio, frame: Vec::new(), index: -1, exhausted: false, failure: None }
    }

    /// A movie over frames that already exist (tests).
    pub fn from_frames(info: MovieInfo, size: (u32, u32), frames: Vec<Vec<u8>>) -> Self {
        Self::from_decoded(info, size, frames.into_iter().map(Ok).collect())
    }

    /// A movie whose decoder fails after `frames`, with `reason` (tests of BIK-DEC-004).
    pub fn failing_after(info: MovieInfo, size: (u32, u32), frames: Vec<Vec<u8>>, reason: &str) -> Self {
        let decoded = frames.into_iter().map(Ok).chain(std::iter::once(Err(reason.to_owned()))).collect();
        Self::from_decoded(info, size, decoded)
    }

    fn from_decoded(info: MovieInfo, size: (u32, u32), decoded: Vec<DecodedFrame>) -> Self {
        let (tx, rx) = sync_channel(decoded.len().max(1));
        for item in decoded {
            tx.send(item).expect("the ring holds every frame");
        }
        Self::streaming(info, size, rx, None, None)
    }

    /// BIK-DEC-004 — why decoding stopped before the last frame, once the reader has said so.
    pub fn failure(&self) -> Option<&str> {
        self.failure.as_deref()
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
            while let Ok(item) = rx.recv() {
                match item {
                    Ok(frame) => {
                        self.frame = frame;
                        self.index += 1;
                    }
                    Err(reason) => {
                        self.failure = Some(reason);
                        break;
                    }
                }
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
                Ok(Ok(frame)) => {
                    self.frame = frame;
                    self.index += 1;
                }
                Ok(Err(reason)) => {
                    self.failure = Some(reason);
                    self.exhausted = true;
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => self.exhausted = true,
            }
        }
        if self.exhausted && self.index < target {
            return match &self.failure {
                Some(reason) => Playhead::Failed(reason),
                None => Playhead::Finished,
            };
        }
        if self.index < 0 {
            return Playhead::Waiting;
        }
        Playhead::Frame { index: self.index as u64, pixels: &self.frame, new: self.index != before }
    }
}

impl Drop for Movie {
    fn drop(&mut self) {
        // Close the ring first: a reader blocked in `send` wakes with an error and ends.
        self.rx = None;
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
        let (tx, rx) = sync_channel::<DecodedFrame>(1);
        movie.rx = Some(rx);
        assert_eq!(movie.advance(0.0), Playhead::Waiting);
        tx.send(Ok(vec![7; 16])).unwrap();
        assert_eq!(movie.advance(0.0), Playhead::Frame { index: 0, pixels: &[7u8; 16], new: true });
        drop(tx);
        assert_eq!(movie.advance(0.2), Playhead::Finished, "the decoder closed before frame 2");
    }

    /// BIK-DEC-004 — a frame that does not decode ends the movie as Failed with the decoder's reason,
    /// after the frames decoded before it have been shown; never as a clean finish.
    #[test]
    fn a_frame_that_does_not_decode_ends_the_movie_as_failed() {
        let reason = "x.bik: frame 2 does not decode: bad block";
        let mut movie = Movie::failing_after(info(10.0), (2, 2), vec![vec![0; 16], vec![1; 16]], reason);
        assert_eq!(movie.advance(0.15), Playhead::Frame { index: 1, pixels: &[1u8; 16], new: true }, "frames before it play");
        assert_eq!(movie.advance(0.25), Playhead::Failed(reason));
        assert_eq!(movie.failure(), Some(reason));

        let mut movie = Movie::failing_after(info(10.0), (2, 2), vec![vec![0; 16]], reason);
        assert_eq!(movie.decode_to_end(), 1, "the corpus count stops at the failure");
        assert_eq!(movie.failure(), Some(reason));
    }

    /// BIK-DEC-004 / #6991 — every reader thread is spawned by `spawn_reader`, which runs the decode
    /// loop under `guard_decoder` (`a_decoder_panic_ends_the_movie_as_failed` runs it). A second spawn
    /// site would bypass that, so the one named-thread spawn in this file must sit inside
    /// `spawn_reader`; the needle is split so this test's own text cannot match it (#5469).
    #[test]
    fn spawn_reader_is_the_only_reader_thread() {
        let src = include_str!("lib.rs");
        let needle = concat!("thread::Builder::", "new()");
        let spawns: Vec<usize> = src.match_indices(needle).map(|(at, _)| at).collect();
        assert_eq!(spawns.len(), 1, "one reader spawn site, in spawn_reader");
        let start = src.find("fn spawn_reader(").expect("spawn_reader exists");
        let end = start + src[start..].find("\n}\n").expect("spawn_reader closes at column zero");
        assert!((start..end).contains(&spawns[0]), "the reader spawn must be spawn_reader's");
    }

    /// BIK-DEC-004 — a decoder that panics mid-movie ends it as Failed with a reason, like a frame
    /// that does not decode: the panic would otherwise only close the ring, which reads as Finished.
    #[test]
    fn a_decoder_panic_ends_the_movie_as_failed() {
        let (rx, reader) = spawn_reader("x.bk2".to_owned(), |_, emit| {
            assert!(emit(Ok(vec![7; 16])));
            panic!("slice 3 out of range");
        })
        .unwrap();
        let mut movie = Movie::streaming(info(10.0), (2, 2), rx, Some(reader), None);
        assert_eq!(movie.decode_to_end(), 1, "the frame before the panic was decoded");
        assert_eq!(movie.failure(), Some("x.bk2: the decoder panicked: slice 3 out of range"));
    }

    /// #7128 — a panic on the open path (the caller's thread) is a `Decode` error naming the movie,
    /// not an unwind into the engine; what the body returns passes through untouched.
    #[test]
    fn a_panic_while_opening_is_a_decode_error() {
        let path = Path::new("/movies/x.bik");
        let opened: Result<u32, VideoError> = guard_open(path, || panic!("track table out of range"));
        assert!(
            matches!(&opened, Err(VideoError::Decode(reason)) if reason == "x.bik: the decoder panicked while opening: track table out of range"),
            "{opened:?}"
        );
        assert!(matches!(guard_open(path, || Ok(7)), Ok(7)));
        assert!(matches!(guard_open::<()>(path, || Err(VideoError::Unreadable("no".into()))), Err(VideoError::Unreadable(_))));
    }

    /// #7128 — both public entry points run their whole body through `guard_open`, so nothing they
    /// call (container parse, decoder setup, soundtrack decode) runs outside it: the statement after
    /// each signature is the guard. The signatures are split so this test's own text cannot match
    /// them (#5469); each is found once, in production code.
    #[test]
    fn the_open_path_runs_under_the_open_guard() {
        let src = include_str!("lib.rs");
        for entry in [concat!("pub fn ", "probe(path: &Path)"), concat!("pub fn ", "open(path: &Path,")] {
            assert_eq!(src.matches(entry).count(), 1, "{entry} is defined once");
            let start = src.find(entry).unwrap();
            let first_statement = src[start..].lines().nth(1).unwrap_or("").trim();
            assert!(first_statement.starts_with("guard_open(path, || "), "{entry} must run its body through guard_open: {first_statement}");
        }
    }

    /// A solid `w`×`h` RGBA picture.
    fn solid(w: usize, h: usize, rgb: [u8; 3]) -> Vec<u8> {
        (0..w * h).flat_map(|_| [rgb[0], rgb[1], rgb[2], 255]).collect()
    }

    #[test]
    fn a_picture_the_size_of_the_window_is_not_resampled() {
        let picture: Vec<u8> = (0..4 * 3 * 4).map(|n| n as u8).collect();
        assert_eq!(fit(&picture, (4, 3), (4, 3), true), picture);
    }

    #[test]
    fn letterboxing_centres_the_picture_between_black_bars() {
        // 4:3 into a square window: the picture is 8 high and 6 rows of bars remain, 3 above and below.
        let out = fit(&solid(4, 3, [200, 100, 50]), (4, 3), (8, 8), true);
        let row = |y: usize| &out[y * 8 * 4..(y + 1) * 8 * 4];
        let all = |y: usize, px: [u8; 4]| row(y).chunks(4).all(|p| p == px);
        assert!(all(0, [0, 0, 0, 255]) && all(7, [0, 0, 0, 255]), "bars are opaque black");
        assert!(all(3, [200, 100, 50, 255]), "the picture fills its rows");
        let bars = (0..8).filter(|&y| all(y, [0, 0, 0, 255])).count();
        assert_eq!(bars, 2, "a 4:3 picture in a square leaves two rows of bars at this size");
    }

    #[test]
    fn without_letterbox_the_picture_fills_the_window() {
        let out = fit(&solid(4, 3, [9, 8, 7]), (4, 3), (8, 8), false);
        assert!(out.chunks(4).all(|p| p == [9, 8, 7, 255]));
    }

    /// BIK-DEC-002 (#6896): the English track is the one with ID 1, wherever it sits in the file; a file
    /// without that ID plays its first track.
    #[test]
    fn the_english_track_is_the_one_with_id_one() {
        assert_eq!(original_track_index([1, 2, 3, 4, 5]), 0, "Fallout 4's films: IDs 1 to 5");
        assert_eq!(original_track_index([0, 1, 2, 3, 4, 5]), 1, "Fallout 4's GameIntro_V3_B: IDs 0 to 5");
        assert_eq!(original_track_index([8, 3, 1]), 2);
        assert_eq!(original_track_index([7, 9]), 0, "no ID 1: the first track");
        assert_eq!(original_track_index([]), 0);
    }

    /// INC3-1: the soundtrack buffer is bounded by the movie's running time, and stops there.
    #[test]
    fn the_soundtrack_is_capped_by_the_running_time() {
        assert_eq!(pcm_cap(10.0, 48_000, 2), (12.0 * 1.5 * 48_000.0) as usize * 2);
        assert_eq!(pcm_cap(f64::NAN, 48_000, 2), pcm_cap(0.0, 48_000, 2), "a degenerate duration is no running time");
        // BIK-AUD-1: header-controlled terms cannot overflow it or lift it past the absolute ceiling.
        let hostile_secs = 30_000.0 / (1.0 / f64::from(u32::MAX));
        assert_eq!(pcm_cap(hostile_secs, 48_000, 2), MAX_SOUNDTRACK_SAMPLES);
        assert_eq!(pcm_cap(1_000_000.0 / 30.0, 48_000, 2), MAX_SOUNDTRACK_SAMPLES);
        assert_eq!(pcm_cap(10.0, u32::MAX, 2), MAX_SOUNDTRACK_SAMPLES, "nor can the track's rate");
        assert!(MAX_SOUNDTRACK_SAMPLES * 2 <= u32::MAX as usize, "the WAV header's byte count holds it");
        // Frames of 4 samples against a 10-sample movie: the third is cut to 2 and the track stops.
        let caps = Soundtrack { channels: 2, rate: 48_000, total_cap: 10, frame_cap: 6 };
        let mut asked = 0;
        let wav = decode_soundtrack("t", 100, caps, |_, pcm| {
            asked += 1;
            pcm.extend([7i16; 4]);
            Ok(true)
        })
        .expect("a soundtrack was written");
        let bytes = std::fs::read(&wav).unwrap();
        let _ = std::fs::remove_file(&wav);
        assert_eq!(asked, 3, "decoding stops at the cap");
        assert_eq!(bytes[..44], wav_header(10, 2, 48_000), "the header counts the samples written");
        assert_eq!(bytes.len(), 44 + 20);
    }

    /// #7125: one frame's audio is bounded by the frame, not only the movie, so a single packet
    /// cannot decode the whole hour the running-time cap allows; a header frame rate cannot lift
    /// it either. The track ends with the frames before it, which stay playable.
    #[test]
    fn one_frame_cannot_spend_the_soundtrack() {
        let at_30 = frame_pcm_cap(30.0, 48_000, 2);
        assert_eq!(at_30, (((1.0 / 30.0) + FRAME_AUDIO_SLACK_SECS) * 48_000.0) as usize * 2);
        let slowest = frame_pcm_cap(1.0, 48_000, 2);
        assert_eq!(frame_pcm_cap(1.0 / f64::from(u32::MAX), 48_000, 2), slowest, "a near-zero frame rate");
        assert_eq!(frame_pcm_cap(0.0, 48_000, 2), (FRAME_AUDIO_SLACK_SECS * 48_000.0) as usize * 2);
        assert_eq!(frame_pcm_cap(f64::NAN, 48_000, 2), frame_pcm_cap(0.0, 48_000, 2));
        assert!(slowest * 1000 < MAX_SOUNDTRACK_SAMPLES, "a frame holds seconds, not the hour");

        let caps = Soundtrack { channels: 2, rate: 48_000, total_cap: MAX_SOUNDTRACK_SAMPLES, frame_cap: 8 };
        let wav = decode_soundtrack("t", 100, caps, |frame, pcm| {
            pcm.extend(std::iter::repeat_n(1i16, if frame == 2 { 9 } else { 8 }));
            Ok(true)
        })
        .expect("the frames before the oversized one were written");
        let bytes = std::fs::read(&wav).unwrap();
        let _ = std::fs::remove_file(&wav);
        assert_eq!(bytes.len(), 44 + 2 * 16, "frames 0 and 1 only");
        assert!(decode_soundtrack("t", 1, caps, |_, pcm| {
            pcm.extend([1i16; 9]);
            Ok(true)
        })
        .is_none(), "an oversized first frame leaves no soundtrack");
    }

    #[test]
    fn a_wav_header_describes_its_samples() {
        let h = wav_header(100, 2, 44100);
        assert_eq!(&h[0..4], b"RIFF");
        assert_eq!(u32::from_le_bytes(h[4..8].try_into().unwrap()), 36 + 200);
        assert_eq!(u16::from_le_bytes(h[22..24].try_into().unwrap()), 2);
        assert_eq!(u32::from_le_bytes(h[24..28].try_into().unwrap()), 44100);
        assert_eq!(u32::from_le_bytes(h[28..32].try_into().unwrap()), 44100 * 4);
        assert_eq!(u32::from_le_bytes(h[40..44].try_into().unwrap()), 200);
    }

    /// Bink 1 and Bink 2 revisions without a decoder are reported unsupported, not as damaged
    /// files; garbage, an empty Bink 2 header and a missing file are unreadable.
    #[test]
    fn formats_without_a_decoder_are_unsupported_and_damage_is_unreadable() {
        let dir = tempfile::tempdir().unwrap();
        let write = |name: &str, bytes: &[u8]| {
            let path = dir.path().join(name);
            std::fs::write(&path, bytes).unwrap();
            path
        };
        let mut bink2 = b"KB2a".to_vec();
        bink2.resize(0x80, 0);
        assert!(matches!(probe(&write("a.bk2", &bink2)), Err(VideoError::Unsupported(reason)) if reason.contains("Bink 2 revision 'a'")));
        assert!(matches!(Movie::open(&write("b.bk2", &bink2), (8, 8), true), Err(VideoError::Unsupported(_))));
        bink2[3] = b'g';
        assert!(matches!(probe(&write("e.bk2", &bink2)), Err(VideoError::Unreadable(_))), "no frames, no size");
        assert!(matches!(Movie::open(&write("f.bk2", &bink2), (8, 8), true), Err(VideoError::Unreadable(_))));
        // #7135: a video flag the DLL decodes differently (bit 20 on KB2g) is unsupported, not damage.
        bink2[0x24..0x28].copy_from_slice(&(1u32 << 20).to_le_bytes());
        assert!(matches!(probe(&write("g.bk2", &bink2)), Err(VideoError::Unsupported(reason)) if reason.contains("video flags")));
        assert!(matches!(Movie::open(&write("h.bk2", &bink2), (8, 8), true), Err(VideoError::Unsupported(_))));
        let mut other = b"BIKb".to_vec();
        other.resize(0x80, 0);
        assert!(matches!(probe(&write("c.bik", &other)), Err(VideoError::Unsupported(reason)) if reason.contains("'b'")));
        assert!(matches!(probe(&write("d.bik", b"not a movie at all, not at all......................")), Err(VideoError::Unreadable(_))));
        assert!(matches!(probe(&dir.path().join("missing.bik")), Err(VideoError::Unreadable(_))));
    }

    /// A parseable one-frame `BIKi` movie of `width` x `height`: the header, the frame index and a
    /// frame of three small plane records, laid out as the `BIK1_*` offsets read them.
    fn bink1_movie(width: u32, height: u32) -> Vec<u8> {
        let put = |bytes: &mut Vec<u8>, at: usize, value: u32| bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
        let mut frame = Vec::new();
        for _ in 0..3 {
            frame.extend_from_slice(&8u32.to_le_bytes());
            frame.extend_from_slice(&[0; 8]);
        }
        let mut bytes = vec![0u8; BIK1_HEADER_LEN];
        bytes[..4].copy_from_slice(b"BIKi");
        for (at, value) in [(0x08, 1), (BIK1_FRAME_COUNT_AT, 1), (0x14, width), (0x18, height), (0x1c, 30), (0x20, 1), (BIK1_TRACK_COUNT_AT, 0)] {
            put(&mut bytes, at, value);
        }
        let first = BIK1_HEADER_LEN + BIK1_INDEX_ENTRY_LEN * 2;
        bytes.extend_from_slice(&(first as u32 | 1).to_le_bytes());
        bytes.extend_from_slice(&((first + frame.len()) as u32 | 1).to_le_bytes());
        bytes.extend_from_slice(&frame);
        let len = (bytes.len() - 8) as u32;
        put(&mut bytes, 4, len);
        bytes
    }

    /// #7123: a Bink 1 header is held to the picture limits a Bink 2 header is, so a 100-byte file
    /// cannot make the decoder reserve gigabyte planes; a picture at the limits still probes.
    #[test]
    fn a_bink_1_picture_past_the_bink_2_limits_is_refused_before_the_decoder_sizes_it() {
        let dir = tempfile::tempdir().unwrap();
        let write = |name: &str, bytes: &[u8]| {
            let path = dir.path().join(name);
            std::fs::write(&path, bytes).unwrap();
            path
        };
        let small = write("small.bik", &bink1_movie(64, 32));
        assert_eq!(probe(&small).map(|info| (info.width, info.height)).unwrap(), (64, 32));
        let at_limit = write("limit.bik", &bink1_movie(bink2::MAX_DIMENSION as u32, (bink2::MAX_PIXELS / bink2::MAX_DIMENSION) as u32));
        assert!(probe(&at_limit).is_ok(), "a picture exactly at the side and area limits is a movie");
        for (name, width, height) in [("vast.bik", 32767, 32767), ("wide.bik", 8193, 16), ("area.bik", 4097, 4096)] {
            let path = write(name, &bink1_movie(width, height));
            assert!(
                matches!(probe(&path), Err(VideoError::Unreadable(reason)) if reason.contains("limit")),
                "{width}x{height} probes"
            );
            assert!(
                matches!(Movie::open(&path, (8, 8), true), Err(VideoError::Unreadable(reason)) if reason.contains("limit")),
                "{width}x{height} opens"
            );
        }
    }

    /// BIK-DEC-003: a Bink 2 movie without audio tracks (the 96x32 probe stream of
    /// `byroredux-bink2`'s tests, four frames at 30 fps) plays every frame, converted as the decoder
    /// converts it.
    #[test]
    fn a_bink_2_movie_plays_its_frames() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("probe.bk2");
        std::fs::write(&path, include_bytes!("../../bink2/tests/data/probe_pr.bk2")).unwrap();
        let info = probe(&path).unwrap();
        assert_eq!((info.width, info.height, info.fps, info.audio_tracks), (96, 32, 30.0, 0));
        assert!((info.duration_secs - 4.0 / 30.0).abs() < 1e-9);
        let mut movie = Movie::open(&path, (96, 32), true).unwrap();
        assert!(movie.audio_path().is_none());
        assert_eq!(movie.decode_to_end(), 4);
        let mut file = bink2::Bk2::open(&path).unwrap();
        let mut decoder = bink2::Decoder::for_file(&file);
        let mut last = Vec::new();
        for index in 0..file.frames {
            let packet = file.video(index).unwrap();
            last = decoder.decode(&packet, file.is_key(index)).unwrap().to_rgba();
        }
        assert_eq!(movie.frame, last, "the last frame, unscaled");
    }

    /// BIK-DEC-003: a Bink 2 movie's sound (`byroredux-binkaudio`'s handcrafted stream: 5 frames, one
    /// 48 kHz stereo track, a short last packet) becomes the WAV the audio crate streams.
    #[test]
    fn a_bink_2_movie_brings_its_sound() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sound.bk2");
        std::fs::write(&path, include_bytes!("../../binkaudio/tests/data/fixture.bk2")).unwrap();
        let movie = Movie::open(&path, (32, 32), true).unwrap();
        let wav = std::fs::read(movie.audio_path().expect("a soundtrack")).unwrap();
        assert_eq!(&wav[..4], b"RIFF");
        assert_eq!(u16::from_le_bytes([wav[22], wav[23]]), 2, "stereo");
        assert_eq!(u32::from_le_bytes(wav[24..28].try_into().unwrap()), 48000);
        assert_eq!(wav.len() - 44, 2 * (4 * 3840 + 1280), "every stated sample");
    }

    /// BIK-G-002, BIK-DEC-003 / acceptance scenario 1: every Bink 1 movie (`BIKi`) and every Bink 2
    /// movie (`KB2g`, `KB2j`) of every installed game decodes to its last frame, and the frame count
    /// is the header's. Also reports the decode speed. `BYROREDUX_STEAM_COMMON=<steamapps/common>
    /// cargo test -p byroredux-video --release -- --ignored --nocapture` (needs the games).
    #[test]
    #[ignore = "needs the games' Data\\Video folders"]
    fn every_installed_bink_movie_decodes_to_its_last_frame() {
        let Ok(common) = std::env::var("BYROREDUX_STEAM_COMMON") else {
            panic!("set BYROREDUX_STEAM_COMMON to the Steam library's steamapps/common folder");
        };
        let (mut decoded, mut failed) = ([0u32; 2], Vec::new());
        for (game, folder) in [
            ("Oblivion", "Data/Video"),
            ("Fallout 3 goty", "Data/Video"),
            ("Fallout New Vegas", "Data/Video"),
            ("Skyrim Special Edition", "Data/Video"),
            ("Fallout 4", "Data/Video"),
            ("Fallout76", "Data/Video"),
            ("Starfield", "Data/video"),
        ] {
            let Ok(entries) = std::fs::read_dir(Path::new(&common).join(game).join(folder)) else { continue };
            let mut files: Vec<_> = entries.flatten().map(|entry| entry.path()).collect();
            files.sort();
            for file in files {
                let mut magic = [0u8; 3];
                if std::fs::File::open(&file).and_then(|mut f| f.read_exact(&mut magic)).is_err() {
                    continue;
                }
                let bink2 = match &magic {
                    b"BIK" => false,
                    b"KB2" => true,
                    _ => continue,
                };
                let name = format!("{game}/{}", file.file_name().unwrap().to_string_lossy());
                let started = std::time::Instant::now();
                let outcome = probe(&file).and_then(|info| {
                    let expected = (info.duration_secs * info.fps).round() as u64;
                    Movie::open(&file, (320, 180), true).map(|mut movie| (movie.decode_to_end(), expected))
                });
                // BIK-DEC-003: a Bink 2 movie with tracks brings its sound
                let outcome = outcome.and_then(|(frames, expected)| match (bink2, probe(&file)) {
                    (true, Ok(info)) if info.audio_tracks > 0 => Movie::open(&file, (32, 32), true)
                        .map(|movie| (if movie.audio_path().is_some() { frames } else { 0 }, expected)),
                    _ => Ok((frames, expected)),
                });
                match outcome {
                    Ok((frames, expected)) if frames > 0 && frames == expected => {
                        decoded[bink2 as usize] += 1;
                        println!("ok   {name}: {frames} frames in {:.1} s", started.elapsed().as_secs_f64());
                    }
                    other => failed.push(format!("{name}: {other:?}")),
                }
            }
        }
        println!("{} Bink 1 and {} Bink 2 movies decoded", decoded[0], decoded[1]);
        assert!(failed.is_empty(), "movies that did not decode to their last frame: {failed:#?}");
        assert!(decoded.iter().sum::<u32>() > 0, "no Bink movie was found");
    }
}
