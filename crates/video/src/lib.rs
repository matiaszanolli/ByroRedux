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

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc::{sync_channel, Receiver, TryRecvError};
use std::thread::JoinHandle;

use byroredux_bink2 as bink2;
use mercs2_bink::{AudioDecoder, VideoDecoder};

/// Decoded frames the reader thread may run ahead of playback.
const FRAME_RING: usize = 6;

/// The Bink 2 revisions `byroredux-bink2` decodes (the last byte of the magic).
const BINK2_REVISIONS: &[u8] = b"gj";

/// Threads a Bink 2 movie's slices decode on (its frames have at most 4 slices).
const BINK2_THREADS: usize = 4;

/// What a movie request cannot do.
#[derive(Debug, thiserror::Error)]
pub enum VideoError {
    /// The movie is in a format this build has no decoder for (a Bink 1 revision other than `i`, a
    /// Bink 2 revision other than `g` and `j`, an alpha or grayscale plane).
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
    let file = bink2::Bk2::open(path).map_err(|error| VideoError::Unreadable(error.to_string()))?;
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

/// Measure `path` from its container header alone: nothing of the picture is read.
pub fn probe(path: &Path) -> Result<MovieInfo, VideoError> {
    let magic = magic(path)?;
    if &magic[..3] == b"KB2" {
        return bink2_container(path, &magic).map(|file| describe_bink2(&file));
    }
    let mut file = std::fs::File::open(path).map_err(|error| VideoError::Unreadable(error.to_string()))?;
    // The header names the tracks (0x28) and the frame index (0x10); the offset table ends the
    // part the parser reads.
    let mut head = [0u8; 0x2c];
    file.read_exact(&mut head).map_err(|error| VideoError::Unreadable(error.to_string()))?;
    if &head[..4] != b"BIKi" {
        return Err(container_error(&head, format!("not a Bink 1 movie: magic {:02x?}", &head[..4])));
    }
    let word = |at: usize| u32::from_le_bytes([head[at], head[at + 1], head[at + 2], head[at + 3]]) as usize;
    let table_end = 0x2c + 12 * word(0x28).min(0x100) + 4 * (word(0x10).min(1_000_000) + 1);
    let mut bytes = head.to_vec();
    bytes.resize(table_end, 0);
    file.read_exact(&mut bytes[0x2c..]).map_err(|error| VideoError::Unreadable(error.to_string()))?;
    match mercs2_bink::Movie::parse(&bytes) {
        Ok(movie) => Ok(describe(&movie)),
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

/// Decode the movie's first audio track to a temporary WAV. `None` when it has no audio or the
/// track does not decode (the picture then plays silent, with a warning).
fn extract_audio(movie: &mercs2_bink::Movie) -> Option<PathBuf> {
    let track = movie.tracks.first()?;
    let mut decoder = match AudioDecoder::new(track) {
        Ok(decoder) => decoder,
        Err(error) => {
            log::warn!("video: its first audio track cannot be decoded: {error}");
            return None;
        }
    };
    let mut pcm: Vec<i16> = Vec::new();
    for frame in 0..movie.frame_count {
        let record = movie.split(frame).ok().and_then(|(records, _)| records.first().copied());
        match record.map(|record| decoder.decode_record(record)) {
            Some(Ok(samples)) => pcm.extend(samples),
            Some(Err(error)) => {
                log::warn!("video: its audio stops at frame {frame}: {error}");
                break;
            }
            None => break,
        }
    }
    write_wav(&pcm, track.channels() as u16, track.rate())
}

/// Interleaved 16-bit PCM into a temporary WAV the audio crate streams; `None` when there is none or
/// it cannot be written (the picture then plays silent, with a warning).
fn write_wav(pcm: &[i16], channels: u16, rate: u32) -> Option<PathBuf> {
    if pcm.is_empty() {
        return None;
    }
    let wav = temp_wav_path();
    let written = std::fs::File::create(&wav).and_then(|file| {
        let mut out = std::io::BufWriter::new(file);
        out.write_all(&wav_header(pcm.len(), channels, rate))?;
        for sample in pcm {
            out.write_all(&sample.to_le_bytes())?;
        }
        out.flush()
    });
    match written {
        Ok(()) => Some(wav),
        Err(error) => {
            log::warn!("video: its soundtrack could not be written: {error}");
            let _ = std::fs::remove_file(&wav);
            None
        }
    }
}

/// BIK-DEC-003: a Bink 2 movie's sound, its first track decoded up front by `byroredux-binkaudio` into
/// a temporary WAV. The tracks are language versions of one mix (binkaudio A9); which one the original
/// picks is BIK-DEC-002's open question, so the first plays. `None` without tracks, or when the track
/// does not decode (the picture then plays silent, with a warning).
fn bink2_audio(file: &mut bink2::Bk2, name: &str) -> Option<PathBuf> {
    let track = *file.tracks.first()?;
    let mut decoder = match byroredux_binkaudio::Decoder::new(track.rate, track.stereo(), track.dct()) {
        Ok(decoder) => decoder,
        Err(error) => {
            log::warn!("video: {name}: its first audio track cannot be decoded: {error}");
            return None;
        }
    };
    let mut pcm: Vec<i16> = Vec::new();
    for frame in 0..file.frames {
        let packet = match file.audio(frame) {
            Ok(mut packets) => packets.swap_remove(0),
            Err(error) => {
                log::warn!("video: {name}: its audio stops at frame {frame}: {error}");
                break;
            }
        };
        if let Some(packet) = packet {
            if let Err(error) = decoder.decode(packet.decoded_bytes, &packet.data, &mut pcm) {
                log::warn!("video: {name}: its audio stops at frame {frame}: {error}");
                break;
            }
        }
    }
    if file.tracks.len() > 1 {
        log::info!("video: {name}: sound from audio track ID {} (the first of {})", track.id, file.tracks.len());
    }
    write_wav(&pcm, track.channels() as u16, track.rate)
}

/// `rgba` (`src` pixels wide and high) fitted into a `dst` frame: centred inside it with black bars
/// when `letterbox`, stretched to fill it otherwise, bilinearly resampled.
fn fit(rgba: &[u8], src: (usize, usize), dst: (usize, usize), letterbox: bool) -> Vec<u8> {
    if src == dst {
        return rgba.to_vec();
    }
    let mut out = vec![0u8; dst.0 * dst.1 * 4];
    for pixel in out.chunks_exact_mut(4) {
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
        for (pixel, &(left, right, wx)) in line.chunks_exact_mut(4).zip(&columns) {
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
}

/// One movie being decoded.
pub struct Movie {
    info: MovieInfo,
    size: (u32, u32),
    rx: Option<Receiver<Vec<u8>>>,
    reader: Option<JoinHandle<()>>,
    audio: Option<PathBuf>,
    frame: Vec<u8>,
    index: i64,
    exhausted: bool,
}

impl Movie {
    /// Start decoding `path` into `size` RGBA frames: fitted inside it with black bars when
    /// `letterbox`, stretched to fill it otherwise. Blocks while the audio track is decoded.
    pub fn open(path: &Path, size: (u32, u32), letterbox: bool) -> Result<Self, VideoError> {
        let magic = magic(path)?;
        if &magic[..3] == b"KB2" {
            return Self::open_bink2(path, &magic, size, letterbox);
        }
        let bytes = std::fs::read(path).map_err(|error| VideoError::Unreadable(error.to_string()))?;
        let (info, audio, mut video) = {
            let movie = mercs2_bink::Movie::parse(&bytes).map_err(|detail| container_error(&bytes, detail))?;
            let video = VideoDecoder::new(&movie).map_err(VideoError::Unsupported)?;
            (describe(&movie), extract_audio(&movie), video)
        };
        let (width, height) = (size.0.max(2), size.1.max(2));
        let (tx, rx) = sync_channel::<Vec<u8>>(FRAME_RING);
        let reader = std::thread::Builder::new()
            .name("movie-decode".into())
            .spawn(move || {
                let Ok(movie) = mercs2_bink::Movie::parse(&bytes) else { return };
                let target = (width as usize, height as usize);
                for index in 0..movie.frame_count {
                    let frame = match video.decode(&movie, index) {
                        Ok(frame) => frame,
                        Err(error) => {
                            log::warn!("video: frame {index} does not decode: {error}");
                            return;
                        }
                    };
                    let rgba = mercs2_bink::rgb::to_rgba(frame);
                    if tx.send(fit(&rgba, (frame.width as usize, frame.height as usize), target, letterbox)).is_err() {
                        return;
                    }
                }
            })
            .map_err(|error| VideoError::Decode(format!("decoder thread: {error}")))?;
        Ok(Self { info, size: (width, height), rx: Some(rx), reader: Some(reader), audio, frame: Vec::new(), index: -1, exhausted: false })
    }

    /// BIK-DEC-003: a Bink 2 movie, its first audio track decoded up front, its video packets read
    /// from the file as the reader thread needs them and its slices decoded on up to `BINK2_THREADS`
    /// threads.
    fn open_bink2(path: &Path, magic: &[u8], size: (u32, u32), letterbox: bool) -> Result<Self, VideoError> {
        let mut file = bink2_container(path, magic)?;
        let info = describe_bink2(&file);
        let name = path.file_name().map_or_else(|| path.display().to_string(), |n| n.to_string_lossy().into_owned());
        let audio = bink2_audio(&mut file, &name);
        let (width, height) = (size.0.max(2), size.1.max(2));
        let (tx, rx) = sync_channel::<Vec<u8>>(FRAME_RING);
        let reader = std::thread::Builder::new()
            .name("movie-decode".into())
            .spawn(move || {
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
                            log::warn!("video: {name}: frame {index} does not decode: {error}");
                            return;
                        }
                    }
                    if tx.send(fit(&rgba, source, target, letterbox)).is_err() {
                        return;
                    }
                }
            })
            .map_err(|error| VideoError::Decode(format!("decoder thread: {error}")))?;
        Ok(Self { info, size: (width, height), rx: Some(rx), reader: Some(reader), audio, frame: Vec::new(), index: -1, exhausted: false })
    }

    /// A movie over frames that already exist (tests).
    pub fn from_frames(info: MovieInfo, size: (u32, u32), frames: Vec<Vec<u8>>) -> Self {
        let (tx, rx) = sync_channel(frames.len().max(1));
        for frame in frames {
            tx.send(frame).expect("the ring holds every frame");
        }
        Self { info, size, rx: Some(rx), reader: None, audio: None, frame: Vec::new(), index: -1, exhausted: false }
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
        let (tx, rx) = sync_channel::<Vec<u8>>(1);
        movie.rx = Some(rx);
        assert_eq!(movie.advance(0.0), Playhead::Waiting);
        tx.send(vec![7; 16]).unwrap();
        assert_eq!(movie.advance(0.0), Playhead::Frame { index: 0, pixels: &[7u8; 16], new: true });
        drop(tx);
        assert_eq!(movie.advance(0.2), Playhead::Finished, "the decoder closed before frame 2");
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
        let mut other = b"BIKb".to_vec();
        other.resize(0x80, 0);
        assert!(matches!(probe(&write("c.bik", &other)), Err(VideoError::Unsupported(reason)) if reason.contains("'b'")));
        assert!(matches!(probe(&write("d.bik", b"not a movie at all, not at all......................")), Err(VideoError::Unreadable(_))));
        assert!(matches!(probe(&dir.path().join("missing.bik")), Err(VideoError::Unreadable(_))));
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
