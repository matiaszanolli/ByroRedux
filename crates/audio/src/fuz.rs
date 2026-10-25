//! FUZE (`.fuz`) container parse + xWMA decode (#5912).
//!
//! Skyrim/FO4/FO76 voice lines ship as `.fuz`: a `FUZE` header, the raw
//! `.lip` sync bytes, then a complete `RIFF`/`XWMA` stream. The
//! container shape was pinned byte-level by `skyrim_fuz_container_shape`
//! (#5464); the payload census this module was written against (40/40
//! files in `Skyrim - Voices_en0.bsa`, `fuz_census` example) found every
//! embedded stream to be:
//!
//! ```text
//! RIFF(.., 'XWMA') { fmt  (18 B, tag 0x0161), dpds, data }
//! ```
//!
//! Tag `0x0161` is `WAVE_FORMAT_WMAUDIO2` — **WMA v2**, not XMA: the
//! "x" in xWMA is XAudio's (it is XAudio2's in-memory format), and the
//! audio inside is ordinary WMA whose packets ride a `dpds` decoded-
//! packet seek table. The census profile is uniform: mono 44.1 kHz,
//! `block_align` 1487 (the classic WMA2 packet size), ~32 kbps.
//!
//! **Parse, don't link.** No Rust audio stack in this workspace
//! (kira/symphonia) has a WMA decoder, and linking ffmpeg is out. The
//! engine therefore parses the container itself — parameters, `dpds`
//! duration, packet data — and hands the payload to a host `ffmpeg`
//! binary when one exists (probed at use, optional: without it a Skyrim
//! line stays silent on the subtitle estimate exactly as pre-#5912, one
//! warn, no error). With it, the decoded WAV bytes flow into
//! [`load_sound_from_bytes`] like any other vanilla format.

use std::io::{Read, Write};
use std::process::{Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

/// Ceiling on the PCM read back from `ffmpeg` (#6307): 64 MiB is ~6 minutes of
/// 44.1 kHz mono, an order of magnitude past the longest vanilla voice line, and
/// the same figure as `VoiceSoundCache`'s budget. A crafted `data` chunk could
/// otherwise buffer without limit under the cache's write guard.
pub const MAX_PCM_BYTES: usize = 64 * 1024 * 1024;

/// Wall-clock ceiling on one `ffmpeg` decode (#6307). A line decodes in well
/// under a second; the decode runs inside the voice cache's loader, so a hung
/// child would stall the frame that asked for the line.
pub const FFMPEG_TIMEOUT: Duration = Duration::from_secs(10);

/// How much of `ffmpeg`'s stderr is kept for the failure message.
const STDERR_KEPT: usize = 4096;

/// `WAVE_FORMAT_WMAUDIO2` — the payload tag on 40/40 censused SE voice
/// files. (The XMA family, 0x0165/0x0166, never appeared; a stream
/// carrying one would decode-fail with [`FuzDecodeError::UnsupportedFormat`]
/// rather than mis-decode.)
pub const WAVE_FORMAT_WMAUDIO2: u16 = 0x0161;

/// A parsed FUZE container: the `.lip` payload and the whole embedded
/// `RIFF`/`XWMA` image (it is a complete RIFF file of its own).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FuzContainer<'a> {
    /// The `FUZE` header's version field (1 on every observed file).
    pub version: u32,
    /// The raw `.lip` sync bytes — exposed verbatim; no consumer yet
    /// (mouth animation is unstarted), but the range is validated.
    pub lip: &'a [u8],
    /// The complete `RIFF`/`XWMA` audio stream.
    pub xwma: &'a [u8],
}

/// The `fmt ` + `dpds` + `data` summary of an embedded xWMA stream.
#[derive(Debug, Clone, PartialEq)]
pub struct XwmaStream<'a> {
    /// `fmt ` format tag — [`WAVE_FORMAT_WMAUDIO2`] on the vanilla
    /// census; anything else is surfaced so a decode attempt fails
    /// named rather than silently.
    pub format_tag: u16,
    pub channels: u16,
    pub sample_rate: u32,
    pub average_bytes_per_second: u32,
    pub block_align: u16,
    pub bits_per_sample: u16,
    /// The `dpds` chunk: one `u32` per data packet, the cumulative
    /// size of DECODED OUTPUT in BYTES up to and including that packet
    /// (XAudio2's decoded-packet seek table — ffmpeg's `xwma` demuxer
    /// reads the same semantics). The last entry over the decoded
    /// byte rate (`sample_rate × channels × bits/8`) is the stream's
    /// duration. Measured: a vanilla line's last dpds entry is exactly
    /// 2× its decoded sample count (16-bit mono).
    pub decoded_packet_seek: &'a [u8],
    /// The `data` chunk payload (the WMA packet stream).
    pub data: &'a [u8],
}

/// Why a `.fuz` payload could not become audible.
#[derive(Debug)]
pub enum FuzDecodeError {
    /// The bytes are not a FUZE container (bad magic / truncated header).
    BadContainer,
    /// The embedded RIFF is missing, not `XWMA`, or its chunks do not
    /// walk (a `fmt ` below 16 bytes, a chunk past its parent).
    BadXwma,
    /// The `fmt ` tag is not [`WAVE_FORMAT_WMAUDIO2`] (the XMA family,
    /// or something newer) — refusing to guess the payload.
    UnsupportedFormat(u16),
    /// No `ffmpeg` binary on the host — the optional route is absent.
    FfmpegMissing,
    /// `ffmpeg` ran and failed; carries its stderr tail.
    FfmpegFailed(String),
    /// `ffmpeg` produced no decodable output.
    FfmpegEmptyOutput,
    /// `ffmpeg` did not finish within [`FFMPEG_TIMEOUT`] and was killed.
    FfmpegTimedOut,
    /// `ffmpeg` produced more than [`MAX_PCM_BYTES`] of PCM and was cut off.
    FfmpegOutputTooLarge,
    /// The xWMA temp file could not be created or written (a full or
    /// read-only temp directory, permissions). `ffmpeg` may well be present:
    /// this is not [`Self::FfmpegMissing`] (#6307).
    TempWrite(std::io::Error),
}

impl std::fmt::Display for FuzDecodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BadContainer => write!(f, "not a FUZE container"),
            Self::BadXwma => write!(f, "embedded xWMA stream missing or malformed"),
            Self::UnsupportedFormat(tag) => {
                write!(f, "xWMA fmt tag {tag:#06x} is not WMAUDIO2 — refusing to guess")
            }
            Self::FfmpegMissing => write!(f, "no ffmpeg on the host — .fuz audio stays silent"),
            Self::FfmpegFailed(stderr) => {
                write!(f, "ffmpeg decode failed: {}", stderr.trim())
            }
            Self::FfmpegEmptyOutput => write!(f, "ffmpeg produced no audio output"),
            Self::FfmpegTimedOut => write!(f, "ffmpeg did not finish within {FFMPEG_TIMEOUT:?}"),
            Self::FfmpegOutputTooLarge => {
                write!(f, "ffmpeg produced more than {MAX_PCM_BYTES} bytes of audio")
            }
            Self::TempWrite(error) => {
                write!(f, "could not write the xWMA temp file for ffmpeg: {error}")
            }
        }
    }
}

impl std::error::Error for FuzDecodeError {}

/// Parse the FUZE container: magic + version + `.lip` size, the `.lip`
/// payload, then the whole remaining tail as the xWMA image (validated
/// to at least open as `RIFF....XWMA`).
pub fn parse_fuz(data: &[u8]) -> Option<FuzContainer<'_>> {
    if data.len() < 16 || &data[0..4] != b"FUZE" {
        return None;
    }
    let version = u32::from_le_bytes(data[4..8].try_into().ok()?);
    let lip_size = u32::from_le_bytes(data[8..12].try_into().ok()?) as usize;
    // The header is exactly 12 bytes; the `.lip` payload follows
    // immediately (its own `01 00 00 00` version word is INSIDE the lip
    // data, not container padding — the empirical probe that pinned the
    // layout put the RIFF at 12 + lip_size and found it there).
    let lip_end = 12usize.checked_add(lip_size)?;
    let lip = data.get(12..lip_end)?;
    let xwma = data.get(lip_end..)?;
    if xwma.len() < 12 || &xwma[0..4] != b"RIFF" || &xwma[8..12] != b"XWMA" {
        return None;
    }
    Some(FuzContainer {
        version,
        lip,
        xwma,
    })
}

/// Walk the embedded `RIFF`/`XWMA` chunk list and lift the `fmt `,
/// `dpds` and `data` chunks. A chunk whose declared size overflows the
/// stream ends the walk; a stream without a usable `fmt ` declines.
pub fn parse_xwma(riff: &[u8]) -> Option<XwmaStream<'_>> {
    if riff.len() < 12 || &riff[0..4] != b"RIFF" || &riff[8..12] != b"XWMA" {
        return None;
    }
    let mut pos = 12usize;
    let mut fmt: Option<(u16, u16, u32, u32, u16, u16)> = None;
    let mut dpds: &[u8] = &[];
    let mut data: &[u8] = &[];
    while pos + 8 <= riff.len() {
        let cid = &riff[pos..pos + 4];
        let size = u32::from_le_bytes(riff[pos + 4..pos + 8].try_into().ok()?) as usize;
        let Some(payload) = riff.get(pos + 8..pos + 8 + size) else {
            break; // overflowing chunk — walk ends with what decoded
        };
        match cid {
            b"fmt " if payload.len() >= 16 => {
                fmt = Some((
                    u16::from_le_bytes(payload[0..2].try_into().ok()?),
                    u16::from_le_bytes(payload[2..4].try_into().ok()?),
                    u32::from_le_bytes(payload[4..8].try_into().ok()?),
                    u32::from_le_bytes(payload[8..12].try_into().ok()?),
                    u16::from_le_bytes(payload[12..14].try_into().ok()?),
                    u16::from_le_bytes(payload[14..16].try_into().ok()?),
                ));
            }
            b"dpds" => dpds = payload,
            b"data" => data = payload,
            _ => {}
        }
        // RIFF chunks pad to even sizes; honour the pad byte.
        pos += 8 + size + (size & 1);
    }
    let (tag, channels, rate, byterate, align, bits) = fmt?;
    Some(XwmaStream {
        format_tag: tag,
        channels,
        sample_rate: rate,
        average_bytes_per_second: byterate,
        block_align: align,
        bits_per_sample: bits,
        decoded_packet_seek: dpds,
        data,
    })
}

/// The stream's decoded duration, from the `dpds` seek table's last
/// cumulative decoded-BYTE count over the decoded byte rate (16-bit
/// PCM out — see [`XwmaStream::decoded_packet_seek`]). `None` without
/// a table, a zero rate, or a degenerate channel/bits combination.
pub fn decoded_duration_secs(stream: &XwmaStream) -> Option<f64> {
    let bytes_per_sample = u32::from(stream.bits_per_sample.max(8) / 8);
    let decoded_byte_rate =
        u64::from(stream.sample_rate) * u64::from(stream.channels) * u64::from(bytes_per_sample);
    if decoded_byte_rate == 0 || stream.decoded_packet_seek.len() < 4 {
        return None;
    }
    let last = u32::from_le_bytes(
        stream.decoded_packet_seek[stream.decoded_packet_seek.len() - 4..]
            .try_into()
            .ok()?,
    );
    Some(f64::from(last) / decoded_byte_rate as f64)
}

/// Decode a whole `.fuz` image to WAV bytes via the host `ffmpeg`
/// (probed at use — see the module doc for the no-link rationale).
/// The embedded xWMA image is written to a temp file (ffmpeg's xwma
/// demuxer seeks), decoded to WAV on stdout, and handed to
/// [`load_sound_from_bytes`] by the caller.
pub fn fuz_to_wav_bytes(data: &[u8]) -> Result<Vec<u8>, FuzDecodeError> {
    let container = parse_fuz(data).ok_or(FuzDecodeError::BadContainer)?;
    let stream = parse_xwma(container.xwma).ok_or(FuzDecodeError::BadXwma)?;
    if stream.format_tag != WAVE_FORMAT_WMAUDIO2 {
        return Err(FuzDecodeError::UnsupportedFormat(stream.format_tag));
    }
    // ffmpeg demuxes xWMA natively (the `xwma` demuxer reads the same
    // RIFF/`XWMA` + `dpds` shape). Raw `s16le` on stdout, not `-f wav`:
    // a piped WAV cannot be seeked back to patch its RIFF/data sizes, so
    // ffmpeg writes 0xFFFFFFFF there and symphonia's wav reader dies on
    // the declared length. The canonical 44-byte header below carries
    // the real sizes; channel count and rate come from the parsed `fmt`.
    let temp = write_temp_xwma(container.xwma).map_err(FuzDecodeError::TempWrite)?;
    let mut command = Command::new("ffmpeg");
    command
        .args(["-nostdin", "-hide_banner", "-loglevel", "error", "-i"])
        .arg(&temp)
        .args(["-vn", "-acodec", "pcm_s16le", "-f", "s16le", "pipe:1"]);
    let result = run_bounded(command, MAX_PCM_BYTES, FFMPEG_TIMEOUT);
    let _ = std::fs::remove_file(&temp);
    let output = match result {
        Ok(output) => output,
        // No `ffmpeg` on the host (or it is not executable): the optional
        // route is absent, not misconfigured. Any other spawn failure (fd or
        // process limits) says what it was.
        Err(RunError::Spawn(error)) => {
            return Err(match error.kind() {
                std::io::ErrorKind::NotFound | std::io::ErrorKind::PermissionDenied => {
                    FuzDecodeError::FfmpegMissing
                }
                _ => FuzDecodeError::FfmpegFailed(format!("could not start ffmpeg: {error}")),
            });
        }
        Err(RunError::TimedOut) => return Err(FuzDecodeError::FfmpegTimedOut),
        Err(RunError::OutputTooLarge) => return Err(FuzDecodeError::FfmpegOutputTooLarge),
    };
    if !output.status.success() {
        return Err(FuzDecodeError::FfmpegFailed(
            String::from_utf8_lossy(&output.stderr).into_owned(),
        ));
    }
    if output.stdout.is_empty() {
        return Err(FuzDecodeError::FfmpegEmptyOutput);
    }
    Ok(wav_header(output.stdout, stream.channels, stream.sample_rate))
}

/// Write the xWMA image to a fresh temp file. `create_new` (`O_EXCL`) refuses an
/// existing path, a planted symlink included, instead of following and
/// truncating it; the name carries the pid and a per-process counter beside the
/// clock, so concurrent decodes cannot collide (#6307).
fn write_temp_xwma(xwma: &[u8]) -> std::io::Result<std::path::PathBuf> {
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let mut last = None;
    for _ in 0..8 {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let counter = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "byro-fuz-{}-{unique}-{counter}.xwma",
            std::process::id()
        ));
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
        match options.open(&path) {
            Ok(mut file) => {
                if let Err(error) = file.write_all(xwma) {
                    drop(file);
                    let _ = std::fs::remove_file(&path);
                    return Err(error);
                }
                return Ok(path);
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => last = Some(error),
            Err(error) => return Err(error),
        }
    }
    Err(last.unwrap_or_else(|| std::io::Error::from(std::io::ErrorKind::AlreadyExists)))
}

/// A finished child process: its exit status and what it wrote.
struct ChildOutput {
    status: ExitStatus,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

enum RunError {
    /// The process could not be started.
    Spawn(std::io::Error),
    /// It did not exit within the time limit and was killed.
    TimedOut,
    /// It wrote more than the stdout limit; the read end was closed on it.
    OutputTooLarge,
}

/// Run `command` to completion with stdout capped at `max_stdout` bytes and a
/// wall-clock `timeout`. Both pipes are drained on their own threads, so a
/// chatty child cannot block on a full stderr pipe, and stdout is read through
/// a bounded reader: past the cap the read end is closed (the child gets
/// `SIGPIPE`) rather than buffered (#6307).
fn run_bounded(
    mut command: Command,
    max_stdout: usize,
    timeout: Duration,
) -> Result<ChildOutput, RunError> {
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(RunError::Spawn)?;
    let stdout = child.stdout.take().expect("stdout was piped");
    let mut stderr = child.stderr.take().expect("stderr was piped");
    let stdout_reader = std::thread::spawn(move || {
        let mut kept = Vec::new();
        let _ = stdout.take(max_stdout as u64 + 1).read_to_end(&mut kept);
        kept
    });
    let stderr_reader = std::thread::spawn(move || {
        let mut kept = Vec::new();
        let _ = (&mut stderr).take(STDERR_KEPT as u64).read_to_end(&mut kept);
        let _ = std::io::copy(&mut stderr, &mut std::io::sink());
        kept
    });

    let deadline = Instant::now() + timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(5));
            }
            // Past the deadline, or the wait itself failed: reap the child.
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
        }
    };
    let stdout = stdout_reader.join().unwrap_or_default();
    let stderr = stderr_reader.join().unwrap_or_default();
    if stdout.len() > max_stdout {
        return Err(RunError::OutputTooLarge);
    }
    match status {
        Some(status) => Ok(ChildOutput { status, stdout, stderr }),
        None => Err(RunError::TimedOut),
    }
}

/// Wrap raw little-endian s16 PCM in the canonical 44-byte PCM WAV
/// header (real sizes — safe for a non-seekable producer).
fn wav_header(pcm: Vec<u8>, channels: u16, sample_rate: u32) -> Vec<u8> {
    let block_align = channels * 2;
    let byte_rate = sample_rate * u32::from(block_align);
    let data_len = pcm.len() as u32;
    let mut wav = Vec::with_capacity(44 + pcm.len());
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + data_len).to_le_bytes());
    wav.extend_from_slice(b"WAVE");
    wav.extend_from_slice(b"fmt ");
    wav.extend_from_slice(&16u32.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes()); // PCM
    wav.extend_from_slice(&channels.to_le_bytes());
    wav.extend_from_slice(&sample_rate.to_le_bytes());
    wav.extend_from_slice(&byte_rate.to_le_bytes());
    wav.extend_from_slice(&block_align.to_le_bytes());
    wav.extend_from_slice(&16u16.to_le_bytes()); // bits
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&data_len.to_le_bytes());
    wav.extend_from_slice(&pcm);
    wav
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A minimal well-formed FUZE image: the 12-byte header, the lip
    /// payload, and an XWMA stream with fmt/dpds/data — the #5464 shape
    /// (lip starts at byte 12; there is no reserved gap) with the #5912
    /// payload census filled in.
    fn fixture(tag: u16, dpds: &[u32], data: &[u8]) -> Vec<u8> {
        let mut xwma = Vec::new();
        xwma.extend_from_slice(b"RIFF");
        // size patched below
        xwma.extend_from_slice(&[0, 0, 0, 0]);
        xwma.extend_from_slice(b"XWMA");
        let mut fmt = Vec::new();
        fmt.extend_from_slice(&tag.to_le_bytes());
        fmt.extend_from_slice(&1u16.to_le_bytes()); // mono
        fmt.extend_from_slice(&44_100u32.to_le_bytes());
        fmt.extend_from_slice(&4_000u32.to_le_bytes());
        fmt.extend_from_slice(&1_487u16.to_le_bytes());
        fmt.extend_from_slice(&16u16.to_le_bytes());
        fmt.extend_from_slice(&0u16.to_le_bytes()); // cbSize
        let chunk = |cid: &[u8; 4], payload: &[u8], out: &mut Vec<u8>| {
            out.extend_from_slice(cid);
            out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
            out.extend_from_slice(payload);
            if payload.len() & 1 == 1 {
                out.push(0);
            }
        };
        chunk(b"fmt ", &fmt, &mut xwma);
        let dpds_bytes: Vec<u8> = dpds.iter().flat_map(|v| v.to_le_bytes()).collect();
        chunk(b"dpds", &dpds_bytes, &mut xwma);
        chunk(b"data", data, &mut xwma);
        let riff_size = (xwma.len() - 8) as u32;
        xwma[4..8].copy_from_slice(&riff_size.to_le_bytes());

        let mut fuz = Vec::new();
        fuz.extend_from_slice(b"FUZE");
        fuz.extend_from_slice(&1u32.to_le_bytes());
        fuz.extend_from_slice(&9u32.to_le_bytes());
        fuz.extend_from_slice(b"LIPSLIPSY");
        fuz.extend_from_slice(&xwma);
        fuz
    }

    const SAMPLE_DATA: &[u8] = &[0xAB; 32];

    #[test]
    fn container_and_stream_parse_and_duration() {
        let fuz = fixture(WAVE_FORMAT_WMAUDIO2, &[8_820, 17_640], SAMPLE_DATA);
        let container = parse_fuz(&fuz).expect("container parses");
        assert_eq!(container.version, 1);
        assert_eq!(container.lip, b"LIPSLIPSY");
        let stream = parse_xwma(container.xwma).expect("xwma parses");
        assert_eq!(stream.format_tag, WAVE_FORMAT_WMAUDIO2);
        assert_eq!(stream.channels, 1);
        assert_eq!(stream.sample_rate, 44_100);
        assert_eq!(stream.block_align, 1_487);
        assert_eq!(stream.data, SAMPLE_DATA);
        assert_eq!(decoded_duration_secs(&stream), Some(0.2));
    }

    #[test]
    fn malformed_inputs_decline_named() {
        assert_eq!(parse_fuz(b"NOPE"), None, "bad magic");
        assert_eq!(parse_fuz(b"FUZE"), None, "truncated header");
        // A lip size that swallows the whole tail leaves no RIFF.
        let mut fuz = fixture(WAVE_FORMAT_WMAUDIO2, &[], SAMPLE_DATA);
        fuz[8..12].copy_from_slice(&u32::MAX.to_le_bytes());
        assert_eq!(parse_fuz(&fuz), None, "lip size overflow");
        // A stream without XWMA form / fmt chunk.
        let fuz = fixture(WAVE_FORMAT_WMAUDIO2, &[], SAMPLE_DATA);
        let container = parse_fuz(&fuz).expect("container parses");
        let mut riff = container.xwma.to_vec();
        riff[8..12].copy_from_slice(b"WAVE");
        assert_eq!(parse_xwma(&riff), None, "non-XWMA form");
        assert_eq!(
            parse_xwma(&container.xwma[0..20]),
            None,
            "chunks truncated before fmt"
        );
    }

    /// The XMA family (0x0165/0x0166) never appeared on the census; a
    /// stream carrying one must fail NAMED at decode, not mis-decode.
    #[test]
    fn non_wma2_tags_fail_the_decode_named() {
        let fuz = fixture(0x0166, &[4_410], SAMPLE_DATA);
        match fuz_to_wav_bytes(&fuz) {
            Err(FuzDecodeError::UnsupportedFormat(0x0166)) => {}
            other => panic!("expected UnsupportedFormat(0x0166), got {other:?}"),
        }
        // And garbage fails as a container, before any ffmpeg spawn.
        match fuz_to_wav_bytes(b"NOTAFUZECONTAINER........") {
            Err(FuzDecodeError::BadContainer) => {}
            other => panic!("expected BadContainer, got {other:?}"),
        }
    }

    /// #6307 - the subprocess runner is bounded in output and time, and a temp-write failure is
    /// not reported as a missing ffmpeg. `sh` stands in for `ffmpeg`.
    #[cfg(unix)]
    mod bounded {
        use super::super::*;

        fn sh(script: &str) -> Command {
            let mut command = Command::new("sh");
            command.args(["-c", script]);
            command
        }

        #[test]
        fn output_within_the_cap_is_returned_with_its_status() {
            let out = run_bounded(sh("printf abcd; printf oops >&2"), 16, Duration::from_secs(10))
                .unwrap_or_else(|_| panic!("a short run"));
            assert!(out.status.success());
            assert_eq!((out.stdout.as_slice(), out.stderr.as_slice()), (&b"abcd"[..], &b"oops"[..]));
        }

        #[test]
        fn output_past_the_cap_is_cut_off_not_buffered() {
            // 100 MB of zeros against a 1 KB cap: the read end closes, the child dies of SIGPIPE.
            let result = run_bounded(sh("head -c 100000000 /dev/zero"), 1024, Duration::from_secs(10));
            assert!(matches!(result, Err(RunError::OutputTooLarge)));
        }

        #[test]
        fn a_child_that_outlives_the_timeout_is_killed() {
            let started = Instant::now();
            // `exec` so the kill lands on the sleeper itself, not on a shell that forked it.
            let result = run_bounded(sh("exec sleep 30"), 1024, Duration::from_millis(150));
            assert!(matches!(result, Err(RunError::TimedOut)));
            assert!(started.elapsed() < Duration::from_secs(10), "the child was not waited out");
        }

        #[test]
        fn a_chatty_stderr_does_not_block_the_child() {
            // Far more than a pipe buffer (64 KiB) on stderr, with a clean exit.
            let out = run_bounded(sh("head -c 1000000 /dev/zero >&2; printf ok"), 16, Duration::from_secs(10))
                .unwrap_or_else(|_| panic!("a run that talks on stderr"));
            assert!(out.status.success());
            assert_eq!(out.stdout, b"ok");
            assert_eq!(out.stderr.len(), STDERR_KEPT, "only the head of stderr is kept");
        }

        #[test]
        fn a_missing_binary_is_a_spawn_error() {
            let result = run_bounded(Command::new("byro-no-such-binary"), 16, Duration::from_secs(1));
            assert!(matches!(result, Err(RunError::Spawn(_))));
        }

        #[test]
        fn the_temp_file_is_exclusive_and_unique() {
            let a = write_temp_xwma(b"one").unwrap();
            let b = write_temp_xwma(b"two").unwrap();
            assert_ne!(a, b);
            assert_eq!(std::fs::read(&a).unwrap(), b"one");
            assert_eq!(std::fs::read(&b).unwrap(), b"two");
            // An existing path (a planted symlink included) is refused, never truncated.
            let planted = std::env::temp_dir().join(format!("byro-fuz-test-{}", std::process::id()));
            std::fs::write(&planted, b"keep").unwrap();
            let mut options = std::fs::OpenOptions::new();
            options.write(true).create_new(true);
            assert_eq!(
                options.open(&planted).unwrap_err().kind(),
                std::io::ErrorKind::AlreadyExists
            );
            for path in [a, b, planted] {
                let _ = std::fs::remove_file(path);
            }
        }
    }

    #[test]
    fn a_temp_write_failure_is_not_reported_as_a_missing_ffmpeg() {
        let error = FuzDecodeError::TempWrite(std::io::Error::from(std::io::ErrorKind::StorageFull));
        assert!(!matches!(error, FuzDecodeError::FfmpegMissing));
        assert!(error.to_string().contains("temp file"), "{error}");
    }

    /// The real corpus end-to-end: a vanilla SE line parses, reports its
    /// dpds duration, and (when the host has ffmpeg) decodes to a WAV
    /// kira can load. Game-data + ffmpeg gated: skipped when either is
    /// absent, strict when the strict lane asks for game data.
    #[test]
    fn real_voice_line_parses_and_decodes() {
        let Some(data_dir) = std::env::var("BYROREDUX_SKYRIMSE_DATA")
            .ok()
            .filter(|v| std::path::Path::new(v).is_dir())
            .or_else(|| {
                let fallback = std::path::Path::new(
                    "/mnt/data/SteamLibrary/steamapps/common/Skyrim Special Edition/Data",
                );
                fallback.is_dir().then(|| fallback.to_string_lossy().into_owned())
            })
        else {
            if std::env::var("BYROREDUX_REQUIRE_GAME_DATA").is_ok() {
                panic!("BYROREDUX_REQUIRE_GAME_DATA set but no Skyrim SE data found");
            }
            eprintln!("[fuz] skipping: no Skyrim SE data");
            return;
        };
        let bsa_path = std::path::Path::new(&data_dir).join("Skyrim - Voices_en0.bsa");
        if !bsa_path.is_file() {
            eprintln!("[fuz] skipping: no Voices archive");
            return;
        }
        let archive = byroredux_bsa::BsaArchive::open(&bsa_path).expect("open voices bsa");
        let path = archive
            .list_files()
            .into_iter()
            .find(|f| f.to_ascii_lowercase().ends_with(".fuz"))
            .expect("the voice archive ships .fuz lines");
        let bytes = archive.extract(path).expect("extract");
        let container = parse_fuz(&bytes).expect("vanilla container parses");
        let stream = parse_xwma(container.xwma).expect("vanilla xwma parses");
        assert_eq!(
            stream.format_tag, WAVE_FORMAT_WMAUDIO2,
            "the census tag (40/40 files) holds on a fresh sample"
        );
        let duration = decoded_duration_secs(&stream).expect("dpds duration");
        assert!(
            duration > 0.2 && duration < 60.0,
            "a dialogue line lasts a sane span, got {duration}s"
        );
        match fuz_to_wav_bytes(&bytes) {
            Ok(wav) => {
                assert_eq!(&wav[0..4], b"RIFF", "the bridge yields a WAV image");
                assert!(wav.len() > 44, "WAV with actual payload");
                // kira accepts the bridge's output — the same call the
                // VoiceSoundCache path makes.
                let sound = crate::load_sound_from_bytes(wav).expect("wav decodes");
                assert!(
                    (sound.duration().as_secs_f64() - duration).abs() < 0.5,
                    "decoded duration {:.3} ~= dpds duration {duration:.3}",
                    sound.duration().as_secs_f64()
                );
            }
            Err(FuzDecodeError::FfmpegMissing) => {
                eprintln!("[fuz] skipping decode leg: no host ffmpeg");
            }
            Err(e) => panic!("vanilla line failed to decode: {e}"),
        }
    }
}
