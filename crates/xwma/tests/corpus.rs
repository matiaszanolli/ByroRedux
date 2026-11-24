use std::{env, error::Error as StdError, io, path::PathBuf};

use byroredux_bsa::{Ba2Archive, BsaArchive};
use byroredux_xwma::{Decoder, Error, Metadata};
use rayon::prelude::*;

#[cfg(target_os = "linux")]
#[path = "corpus/oracle.rs"]
mod oracle;
#[cfg(target_os = "linux")]
#[path = "corpus/process.rs"]
mod process;

type TestResult<T> = Result<T, Box<dyn StdError + Send + Sync>>;

struct ArchiveSpec {
    root: &'static str,
    file: &'static str,
    extension: &'static str,
    counts: &'static [usize],
}

const ARCHIVES: [ArchiveSpec; 4] = [
    ArchiveSpec {
        root: "BYROREDUX_SKYRIMSE_DATA",
        file: "Skyrim - Sounds.bsa",
        extension: "xwm",
        counts: &[269],
    },
    ArchiveSpec {
        root: "BYROREDUX_FO4_DATA",
        file: "Fallout4 - Sounds.ba2",
        extension: "xwm",
        counts: &[6463, 459],
    },
    ArchiveSpec {
        root: "BYROREDUX_SKYRIMSE_DATA",
        file: "Skyrim - Voices_en0.bsa",
        extension: "fuz",
        counts: &[75_408],
    },
    ArchiveSpec {
        root: "BYROREDUX_FO4_DATA",
        file: "Fallout4 - Voices.ba2",
        extension: "fuz",
        counts: &[116_101],
    },
];

enum Archive {
    Bsa(BsaArchive),
    Ba2(Ba2Archive),
}

impl Archive {
    fn open(spec: &ArchiveSpec) -> TestResult<Self> {
        let root = env::var_os(spec.root)
            .ok_or_else(|| format!("set {} to the game Data directory", spec.root))?;
        let path = PathBuf::from(root).join(spec.file);
        if spec.file.ends_with(".bsa") {
            BsaArchive::open(&path).map(Self::Bsa)
        } else {
            Ba2Archive::open(&path).map(Self::Ba2)
        }
        .map_err(|error| format!("{}: {error}", path.display()).into())
    }

    fn members<'a>(&'a self, spec: &ArchiveSpec) -> Vec<&'a str> {
        let mut names = match self {
            Self::Bsa(bsa) => bsa.list_files(),
            Self::Ba2(ba2) => ba2.list_files(),
        };
        names.retain(|path| {
            path.rsplit_once('.')
                .is_some_and(|(_, ext)| ext.eq_ignore_ascii_case(spec.extension))
        });
        names.sort_unstable();
        names
    }

    fn extract(&self, name: &str) -> io::Result<Vec<u8>> {
        match self {
            Self::Bsa(bsa) => bsa.extract(name),
            Self::Ba2(ba2) => ba2.extract(name),
        }
    }
}

impl ArchiveSpec {
    fn group(&self, name: &str) -> usize {
        let first = name.split(['/', '\\']).next().unwrap_or_default();
        usize::from(self.counts.len() == 2 && first.eq_ignore_ascii_case("music"))
    }
}

fn payload<'a>(spec: &ArchiveSpec, bytes: &'a [u8]) -> TestResult<&'a [u8]> {
    if spec.extension == "xwm" {
        return Ok(bytes);
    }
    unwrap_fuz(bytes).ok_or_else(|| "invalid FUZE wrapper".into())
}

// The payload-only subset of audio::fuz::parse_fuz. Keeping this in the test
// avoids a dependency on the audio crate and its playback backends.
fn unwrap_fuz(bytes: &[u8]) -> Option<&[u8]> {
    if bytes.get(..4)? != b"FUZE" {
        return None;
    }
    let lip_size = u32::from_le_bytes(bytes.get(8..12)?.try_into().ok()?);
    let start = 12usize.checked_add(usize::try_from(lip_size).ok()?)?;
    let xwma = bytes.get(start..)?;
    (xwma.get(..4)? == b"RIFF" && xwma.get(8..12)? == b"XWMA").then_some(xwma)
}

#[derive(Default)]
struct DecodeSummary {
    metadata: Option<Metadata>,
    frames: usize,
    max_native: u32,
    max_chunk: usize,
    error: Option<Error>,
}

impl DecodeSummary {
    #[cfg(target_os = "linux")]
    fn is_short(&self) -> bool {
        matches!(
            (&self.error, self.metadata),
            (Some(Error::FrameCountMismatch { declared, decoded }), Some(meta))
                if *declared == meta.frame_count
                    && *decoded as usize == self.frames && decoded < declared
        )
    }
}

fn decode(
    bytes: &[u8],
    mut samples: impl FnMut(&[i16]) -> TestResult<()>,
) -> TestResult<DecodeSummary> {
    let mut decoder = match Decoder::open(bytes) {
        Ok(decoder) => decoder,
        Err(error) => {
            return Ok(DecodeSummary {
                error: Some(error),
                ..DecodeSummary::default()
            });
        }
    };
    let meta = decoder.metadata();
    let mut summary = DecodeSummary {
        metadata: Some(meta),
        ..DecodeSummary::default()
    };
    let channels = usize::from(meta.channels);
    loop {
        match decoder.next_s16() {
            Ok(Some(pcm)) => {
                assert!(!pcm.is_empty() && pcm.len().is_multiple_of(channels));
                let frames = pcm.len() / channels;
                summary.frames += frames;
                summary.max_chunk = summary.max_chunk.max(frames);
                samples(pcm)?;
            }
            Ok(None) => break,
            Err(error) => {
                summary.error = Some(error);
                break;
            }
        }
    }
    summary.max_native = decoder.largest_codec_frame();
    Ok(summary)
}

#[test]
#[ignore = "XWMA-DEC-001/003 need the four vanilla game archives"]
fn decode_all_to_declared_end() -> TestResult<()> {
    let mut all_failures = 0;
    for spec in &ARCHIVES {
        let archive = Archive::open(spec)?;
        let names = archive.members(spec);
        let mut counts = vec![0; spec.counts.len()];
        let mut failures = vec![0; spec.counts.len()];
        let mut max_native = (0, "", 0);
        let mut max_chunk = 0;
        let mut cap_candidates = Vec::new();

        for batch in names.chunks(256) {
            let summaries: Vec<_> = batch
                .par_iter()
                .map(|name| {
                    let bytes = archive.extract(name)?;
                    decode(payload(spec, &bytes)?, |_| Ok(()))
                })
                .collect();
            for (&name, summary) in batch.iter().zip(summaries) {
                let group = spec.group(name);
                counts[group] += 1;
                match summary {
                    Ok(summary) => {
                        let meta = summary.metadata;
                        let rate = meta.map_or(0, |meta| meta.sample_rate);
                        if summary.max_native > max_native.0 {
                            max_native = (summary.max_native, name, rate);
                        }
                        max_chunk = max_chunk.max(summary.max_chunk);
                        if summary.error.is_some()
                            || meta.is_none_or(|meta| summary.frames != meta.frame_count as usize)
                        {
                            failures[group] += 1;
                            eprintln!(
                                "{} | {name} | {rate} Hz | {} frames | {}",
                                spec.file,
                                summary.frames,
                                summary.error.as_ref().map_or_else(
                                    || "frame count mismatch".to_owned(),
                                    ToString::to_string
                                )
                            );
                            if matches!(summary.error, Some(Error::Codec { code, .. }) if code == -22)
                            {
                                cap_candidates.push((name, rate));
                            }
                        }
                    }
                    Err(error) => {
                        failures[group] += 1;
                        eprintln!("{} | {name} | {error}", spec.file);
                    }
                }
            }
        }
        // libavcodec reports its attempted allocation size in its native log.
        // Replay EINVAL failures serially so any cap diagnostic has one clear member and rate.
        for (name, rate) in &cap_candidates {
            eprintln!(
                "FFmpeg allocation-cap diagnostic replay | {} | {name} | {rate} Hz",
                spec.file
            );
            let bytes = archive.extract(name)?;
            let _ = decode(payload(spec, &bytes)?, |_| Ok(()))?;
        }
        println!(
            "{} | counts {counts:?} (expected {:?}) | failures {failures:?} | largest native {} at {} Hz ({}) | largest Rust chunk {max_chunk} | cap diagnostic replays {}",
            spec.file,
            spec.counts,
            max_native.0,
            max_native.2,
            max_native.1,
            cap_candidates.len()
        );
        all_failures += failures.into_iter().sum::<usize>() + usize::from(counts != spec.counts);
    }
    assert_eq!(
        all_failures, 0,
        "XWMA-DEC-001/003 failures or archive count mismatches"
    );
    Ok(())
}
