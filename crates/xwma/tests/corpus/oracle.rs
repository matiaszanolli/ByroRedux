use std::{
    env,
    fs::{File, OpenOptions},
    io::{self, BufReader, BufWriter, Read, Write},
    os::unix::fs::OpenOptionsExt,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
};

use rayon::prelude::*;
use sha2::{Digest, Sha256};

use super::{ARCHIVES, Archive, DecodeSummary, TestResult, decode, payload, process};

const INPUT: &str = "/proc/self/fd/0";
const V3: [&str; 15] = [
    "-hide_banner",
    "-nostdin",
    "-v",
    "error",
    "-xerror",
    "-c:a",
    "wmav2",
    "-i",
    INPUT,
    "-map",
    "0:a:0",
    "-c:a",
    "pcm_s16le",
    "-f",
    "s16le",
];
const HEADER: &str = "archive\tmember\tinput_sha256\tchannels\trate\tdpds_frames\trust_frames\toracle_channels\toracle_rate\toracle_frames\trust_sha256\toracle_sha256\tdifferent_samples\tfirst_diff_sample\tfirst_missing_sample\tmax_abs_diff\tmax_native_frames\tterminal\tcomparison";

#[derive(Default)]
struct Difference {
    common: usize,
    unequal: usize,
    first: Option<usize>,
    max: u32,
}

impl Difference {
    fn add(&mut self, rust: &[i16], ffmpeg_le: &[u8]) {
        for (&actual, bytes) in rust.iter().zip(ffmpeg_le.as_chunks::<2>().0) {
            let expected = i16::from_le_bytes(*bytes);
            let delta = (i32::from(actual) - i32::from(expected)).unsigned_abs();
            self.max = self.max.max(delta);
            if delta != 0 {
                self.unequal += 1;
                self.first.get_or_insert(self.common);
            }
            self.common += 1;
        }
    }
}

struct Comparison {
    rust: DecodeSummary,
    input_sha: String,
    rust_sha: String,
    oracle_sha: String,
    oracle_channels: usize,
    oracle_rate: u32,
    oracle_frames: usize,
    diff: Difference,
    first_missing: Option<usize>,
    pass: bool,
}

impl Comparison {
    fn write(&self, out: &mut impl Write, archive: &str, name: &str) -> io::Result<()> {
        let field = |value: Option<usize>| value.map(|n| n.to_string()).unwrap_or_default();
        let channels = self.rust.metadata.map(|meta| usize::from(meta.channels));
        let rate = self.rust.metadata.map(|meta| meta.sample_rate as usize);
        let declared = self.rust.metadata.map(|meta| meta.frame_count as usize);
        let terminal = self
            .rust
            .error
            .as_ref()
            .map_or_else(|| "EOF".into(), ToString::to_string);
        writeln!(
            out,
            "{archive}\t{name}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{terminal}\t{}",
            self.input_sha,
            field(channels),
            field(rate),
            field(declared),
            self.rust.frames,
            self.oracle_channels,
            self.oracle_rate,
            self.oracle_frames,
            self.rust_sha,
            self.oracle_sha,
            self.diff.unequal,
            field(self.diff.first),
            field(self.first_missing),
            self.diff.max,
            self.rust.max_native,
            if self.pass { "pass" } else { "fail" },
        )
    }
}

fn probe(input: &File) -> TestResult<(usize, u32)> {
    let output = process::with_input("ffprobe", input)?
        .args([
            "-v",
            "error",
            "-select_streams",
            "a:0",
            "-show_entries",
            "stream=channels,sample_rate",
            "-of",
            "default=noprint_wrappers=1",
            INPUT,
        ])
        .output()?;
    if !output.status.success() {
        return Err(format!(
            "ffprobe {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    let text = String::from_utf8(output.stdout)?;
    let field = |key| {
        text.lines()
            .find_map(|line| line.strip_prefix(key))
            .ok_or_else(|| format!("ffprobe omitted {key}"))
    };
    let channels = field("channels=")?.parse()?;
    let rate = field("sample_rate=")?.parse()?;
    if !matches!(channels, 1 | 2) || rate == 0 {
        return Err(format!("ffprobe returned {channels} channels at {rate} Hz").into());
    }
    Ok((channels, rate))
}

fn compare(bytes: &[u8]) -> TestResult<Comparison> {
    let input = process::memory_file(bytes)?;
    // The V3 decode command with a seekable, RAM-backed file as input.xwm.
    let mut child = process::with_input("ffmpeg", &input)?
        .args(V3)
        .arg("-")
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;
    let result = compare_pcm(bytes, &mut child);
    // A pipe or decoder error can leave a running child. Always reap it.
    if result.is_err() {
        let _ = child.kill();
    }
    let status = child.wait()?;
    let (rust, rust_sha, oracle_sha, oracle_len, diff) = result?;
    let (oracle_channels, oracle_rate) = probe(&input)?;
    let oracle_frames = oracle_len / (oracle_channels * 2);
    let rust_samples = rust.frames * rust.metadata.map_or(0, |meta| usize::from(meta.channels));
    let oracle_samples = oracle_len / 2;
    let first_missing =
        (rust_samples != oracle_samples).then_some(rust_samples.min(oracle_samples));

    // A short decode can agree with FFmpeg sample for sample. The coverage
    // gate independently fails the unsatisfied dpds declaration.
    let pass = status.success()
        && (rust.error.is_none() || rust.is_short())
        && oracle_len.is_multiple_of(oracle_channels * 2)
        && rust.metadata.is_some_and(|meta| {
            usize::from(meta.channels) == oracle_channels && meta.sample_rate == oracle_rate
        })
        && rust.frames == oracle_frames
        && first_missing.is_none()
        && diff.max <= 1;
    Ok(Comparison {
        rust,
        input_sha: format!("{:x}", Sha256::digest(bytes)),
        rust_sha,
        oracle_sha,
        oracle_channels,
        oracle_rate,
        oracle_frames,
        diff,
        first_missing,
        pass,
    })
}

type PcmComparison = (DecodeSummary, String, String, usize, Difference);

fn compare_pcm(bytes: &[u8], child: &mut Child) -> TestResult<PcmComparison> {
    let mut pcm = BufReader::new(child.stdout.take().expect("piped oracle stdout"));
    let mut oracle_buffer = [0; 16_384];
    let mut rust_buffer = [0; 16_384];
    let mut diff = Difference::default();
    let mut rust_hash = Sha256::new();
    let mut oracle_hash = Sha256::new();
    let mut oracle_len = 0;
    let rust = decode(bytes, |chunk| {
        for (&sample, dest) in chunk.iter().zip(rust_buffer.as_chunks_mut::<2>().0) {
            *dest = sample.to_le_bytes();
        }
        let wanted = chunk.len() * 2;
        rust_hash.update(&rust_buffer[..wanted]);
        let mut n = 0;
        while n < wanted {
            let count = pcm.read(&mut oracle_buffer[n..wanted])?;
            if count == 0 {
                break;
            }
            n += count;
        }
        oracle_hash.update(&oracle_buffer[..n]);
        oracle_len += n;
        diff.add(chunk, &oracle_buffer[..n]);
        Ok(())
    })?;
    loop {
        let n = pcm.read(&mut oracle_buffer)?;
        if n == 0 {
            break;
        }
        oracle_hash.update(&oracle_buffer[..n]);
        oracle_len += n;
    }
    Ok((
        rust,
        format!("{:x}", rust_hash.finalize()),
        format!("{:x}", oracle_hash.finalize()),
        oracle_len,
        diff,
    ))
}

fn report_path() -> TestResult<PathBuf> {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()?;
    let target = workspace.join("target").canonicalize()?;
    let path = env::var_os("XWMA_CORPUS_REPORT").map_or_else(
        || target.join("xwma-phase1.tsv"),
        |path| workspace.join(path),
    );
    let parent = path
        .parent()
        .ok_or("XWMA_CORPUS_REPORT must name a file under target/")?
        .canonicalize()?;
    let name = path
        .file_name()
        .ok_or("XWMA_CORPUS_REPORT needs a filename")?;
    if !parent.starts_with(&target) {
        return Err("XWMA_CORPUS_REPORT must resolve under the workspace target/".into());
    }
    Ok(parent.join(name))
}

fn version(program: &str) -> TestResult<String> {
    let output = Command::new(program).arg("-version").output()?;
    if !output.status.success() {
        return Err(format!("{program} -version failed with {}", output.status).into());
    }
    let text = String::from_utf8(output.stdout)?;
    Ok(text
        .lines()
        .next()
        .ok_or("missing version line")?
        .to_owned())
}

#[test]
#[ignore = "XWMA-DEC-002 needs FFmpeg, ffprobe and the four vanilla game archives"]
fn compare_every_entry_with_v3_cli() -> TestResult<()> {
    let mut failures = 0;
    let path = report_path()?;
    let file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(&path)?;
    let mut out = BufWriter::new(file);
    writeln!(out, "# {}", version("ffmpeg")?)?;
    writeln!(out, "# {}", version("ffprobe")?)?;
    writeln!(out, "# ffmpeg {} -", V3.join(" "))?;
    writeln!(out, "{HEADER}")?;

    for spec in &ARCHIVES {
        let archive = Archive::open(spec)?;
        let names = archive.members(spec);
        let mut summary = vec![0; spec.counts.len()];
        let (mut archive_failures, mut short, mut max_diff) = (0, 0, 0);
        let mut done = 0;
        let mut next_progress = 2048;
        for batch in names.chunks(256) {
            let trials: Vec<_> = batch
                .par_iter()
                .map(|name| {
                    let bytes = archive.extract(name)?;
                    compare(payload(spec, &bytes)?)
                })
                .collect();
            for (&name, trial) in batch.iter().zip(trials) {
                summary[spec.group(name)] += 1;
                match trial {
                    Ok(trial) => {
                        trial.write(&mut out, spec.file, name)?;
                        short += usize::from(trial.rust.is_short());
                        max_diff = max_diff.max(trial.diff.max);
                        if !trial.pass {
                            archive_failures += 1;
                            eprintln!(
                                "CLI comparison failed for {} | {name}; see the report",
                                spec.file
                            );
                        }
                    }
                    Err(error) => {
                        archive_failures += 1;
                        let message = error.to_string().replace(['\t', '\r', '\n'], " ");
                        writeln!(
                            out,
                            "{}\t{name}{}\t{message}\tfail",
                            spec.file,
                            "\t".repeat(16)
                        )?;
                        eprintln!("{} | {name} | {message}", spec.file);
                    }
                }
            }
            out.flush()?;
            done += batch.len();
            if done >= next_progress {
                println!("{} | compared {done}/{}", spec.file, names.len());
                next_progress += 2048;
            }
        }
        println!(
            "{} | compared {summary:?} (expected {:?}) | CLI mismatches {archive_failures} | short against dpds {short} | peak difference {max_diff}",
            spec.file, spec.counts
        );
        failures += archive_failures + usize::from(summary != spec.counts);
    }
    println!("per-file counts and digests at {}", path.display());
    assert_eq!(
        failures, 0,
        "XWMA-DEC-002 CLI mismatches or archive count mismatches"
    );
    Ok(())
}
