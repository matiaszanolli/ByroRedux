use std::{
    collections::HashSet,
    env,
    fs::File,
    io::{Seek, Write},
    ops::Range,
    os::fd::FromRawFd,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use sha2::{Digest, Sha256};

use super::{ARCHIVES, Archive, TestResult, payload};
use byroredux_xwma::Decoder;

const INPUT_PATH: &str = "/proc/self/fd/0";

pub(super) fn memory_file(bytes: &[u8]) -> std::io::Result<File> {
    // SAFETY: the name is NUL terminated. A nonnegative result is a newly owned fd.
    let fd = unsafe { libc::memfd_create(c"xwma-input".as_ptr(), libc::MFD_CLOEXEC) };
    if fd < 0 {
        return Err(std::io::Error::last_os_error());
    }
    // SAFETY: fd was returned by memfd_create and has no other owner.
    let mut file = unsafe { File::from_raw_fd(fd) };
    file.write_all(bytes)?;
    file.rewind()?;
    Ok(file)
}

pub(super) fn with_input(program: &str, input: &File) -> std::io::Result<Command> {
    let mut command = Command::new(program);
    command.stdin(Stdio::from(input.try_clone()?));
    Ok(command)
}

fn worker(input: &File, mode: &str) -> TestResult<Command> {
    let mut command = Command::new(env::current_exe()?);
    command
        .args([
            "--ignored",
            "--exact",
            "process::decode_worker",
            "--nocapture",
        ])
        .env("XWMA_TEST_WORKER", mode)
        .stdin(Stdio::from(input.try_clone()?));
    Ok(command)
}

// Libtest parent processes never change their FFmpeg CPU flags. Restriction
// is applied only inside a fresh process before its first codec call.
unsafe extern "C" {
    fn av_force_cpu_flags(flags: std::ffi::c_int);
}

#[test]
#[ignore = "child process for CPU flags and bounded mutation tests"]
fn decode_worker() {
    let Ok(mode) = env::var("XWMA_TEST_WORKER") else {
        return;
    };
    if mode == "scalar" {
        // SAFETY: the worker runs one test and has made no FFmpeg calls yet.
        unsafe { av_force_cpu_flags(0) };
    }
    if mode == "mutation" {
        let limit = libc::rlimit {
            rlim_cur: 256 * 1024 * 1024,
            rlim_max: 256 * 1024 * 1024,
        };
        // SAFETY: limit is initialized and only the worker process is restricted.
        assert_eq!(unsafe { libc::setrlimit(libc::RLIMIT_AS, &limit) }, 0);
    }
    let source = File::open(INPUT_PATH).unwrap();
    let mut decoder = match Decoder::from_reader(source) {
        Ok(decoder) => decoder,
        Err(_) if mode == "mutation" => {
            println!("XWMA_MUTATION_ERROR");
            return;
        }
        Err(error) => panic!("{error}"),
    };
    let meta = decoder.metadata();
    let mut digest = Sha256::new();
    let mut frames = 0usize;
    let mut pcm_bytes = [0u8; 2048 * 2 * 4];
    loop {
        match decoder.next_f32() {
            Ok(Some(pcm)) => {
                frames += pcm.len() / usize::from(meta.channels);
                if mode != "mutation" {
                    for (sample, dst) in pcm.iter().zip(pcm_bytes.as_chunks_mut::<4>().0) {
                        *dst = sample.to_le_bytes();
                    }
                    digest.update(&pcm_bytes[..pcm.len() * 4]);
                }
            }
            Ok(None) => break,
            Err(_) if mode == "mutation" => {
                println!("XWMA_MUTATION_ERROR");
                return;
            }
            Err(error) => panic!("{error}"),
        }
    }
    assert_eq!(frames, meta.frame_count as usize);
    if mode == "mutation" {
        println!("XWMA_MUTATION_COMPLETE");
    } else {
        println!(
            "XWMA_PCM {} {} {frames} {:x}",
            meta.channels,
            meta.sample_rate,
            digest.finalize()
        );
    }
}

fn fixed_inputs() -> TestResult<Vec<(String, Vec<u8>)>> {
    let mut samples = Vec::new();
    for spec in &ARCHIVES {
        let archive = Archive::open(spec)?;
        let names = archive.members(spec);
        if names.len() < 8 {
            return Err(format!("too few entries in {}", spec.file).into());
        }
        for numerator in [0, 1, 3, 5] {
            let name = names[names.len() * numerator / 8];
            let bytes = archive.extract(name)?;
            samples.push((
                format!("{} | {name}", spec.file),
                payload(spec, &bytes)?.to_vec(),
            ));
        }
    }
    Ok(samples)
}

#[test]
#[ignore = "XWMA-DEC-006 needs the four vanilla game archives"]
fn cpu_flags_are_bit_identical() -> TestResult<()> {
    let inputs = fixed_inputs()?;
    for (path, bytes) in &inputs {
        let input = memory_file(bytes)?;
        let run = |mode| -> TestResult<String> {
            let result = worker(&input, mode)?.output()?;
            if !result.status.success() {
                return Err(format!(
                    "{path} ({mode}): {}",
                    String::from_utf8_lossy(&result.stderr)
                )
                .into());
            }
            let stdout = String::from_utf8(result.stdout)?;
            stdout
                .lines()
                .find(|line| line.starts_with("XWMA_PCM "))
                .map(str::to_owned)
                .ok_or_else(|| format!("{path} ({mode}): missing PCM digest").into())
        };
        assert_eq!(run("default")?, run("scalar")?, "{path}");
    }
    println!(
        "{} files have identical interleaved f32 PCM with default and forced-zero CPU flags",
        inputs.len()
    );
    Ok(())
}

fn chunks(bytes: &[u8]) -> [Range<usize>; 3] {
    let mut ranges = [0..0, 0..0, 0..0];
    let mut pos = 12;
    while pos + 8 <= bytes.len() {
        let tag = &bytes[pos..pos + 4];
        let size = u32::from_le_bytes(bytes[pos + 4..pos + 8].try_into().unwrap()) as usize;
        let start = pos + 8;
        let range = start..start + size;
        if let Some(index) = [b"fmt ", b"dpds", b"data"]
            .iter()
            .position(|name| *name == tag)
        {
            ranges[index] = range;
        }
        pos = start + size + (size & 1);
    }
    assert!(
        ranges
            .iter()
            .all(|range| !range.is_empty() && range.end <= bytes.len())
    );
    ranges
}

fn draw(state: &mut u64) -> usize {
    *state ^= *state << 13;
    *state ^= *state >> 7;
    *state ^= *state << 17;
    *state as usize
}

#[test]
#[ignore = "scenario 6 needs the four vanilla game archives and Linux process limits"]
fn bit_flips_terminate_within_limits() -> TestResult<()> {
    let inputs = fixed_inputs()?;
    let ranges: Vec<_> = inputs.iter().map(|(_, bytes)| chunks(bytes)).collect();
    let mut random = 0x7413_2026_1124_u64;
    let mut mutations = HashSet::new();
    let (mut complete, mut errors) = (0, 0);
    for index in 0..1000 {
        let (fixture, byte, bit) = loop {
            let fixture = draw(&mut random) % inputs.len();
            let range = &ranges[fixture][index % 3];
            let byte = range.start + draw(&mut random) % range.len();
            let bit = 1 << (draw(&mut random) % 8);
            if mutations.insert((fixture, byte, bit)) {
                break (fixture, byte, bit);
            }
        };
        let (path, bytes) = &inputs[fixture];
        let mut mutated = bytes.clone();
        mutated[byte] ^= bit;
        let input = memory_file(&mutated)?;
        let mut child = worker(&input, "mutation")?
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if child.try_wait()?.is_some() {
                break;
            }
            if Instant::now() >= deadline {
                let _ = child.kill();
                child.wait()?;
                panic!("mutation {index} {path} byte {byte} mask {bit:#04x} exceeded 5 seconds");
            }
            thread::sleep(Duration::from_millis(1));
        }
        let output = child.wait_with_output()?;
        assert!(
            output.status.success(),
            "mutation {index} {path} byte {byte} mask {bit:#04x}: {}",
            output.status
        );
        let stdout = String::from_utf8(output.stdout)?;
        match stdout
            .lines()
            .find(|line| line.starts_with("XWMA_MUTATION_"))
        {
            Some("XWMA_MUTATION_COMPLETE") => complete += 1,
            Some("XWMA_MUTATION_ERROR") => errors += 1,
            _ => panic!("mutation {index} {path} did not report a result"),
        }
    }
    println!(
        "1,000 fixed-seed fmt/dpds/data bit flips within 5 seconds and 256 MiB per case: {complete} completed, {errors} returned an error"
    );
    Ok(())
}
