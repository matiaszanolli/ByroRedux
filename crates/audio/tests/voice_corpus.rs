use std::{collections::BTreeMap, io, time::Instant};

use byroredux_audio::{SoundFailures, VoiceSoundCache, fuz};
use byroredux_bsa::{Ba2Archive, BsaArchive};
use byroredux_plugin::esm::test_paths::{fo4_data_dir, skyrim_se_data_dir};
use byroredux_xwma::{Decoder, Metadata};

const LIP_SAMPLES: usize = 100;
const FAILURE_LIMIT: usize = 20;
const STATIC_BUDGET_REASON: &str = "exceeds the static PCM limit";

const MAGNOLIA_SONGS: &[&str] = &[
    r"sound\voice\fallout4.esm\npcfmagnolia\0010c39e_1.fuz",
    r"sound\voice\fallout4.esm\npcfmagnolia\0019ddf3_1.fuz",
    r"sound\voice\fallout4.esm\npcfmagnolia\0019ddfe_1.fuz",
    r"sound\voice\fallout4.esm\npcfmagnolia\001d1660_1.fuz",
    r"sound\voice\fallout4.esm\npcfmagnolia\001d1661_1.fuz",
    r"sound\voice\fallout4.esm\npcfmagnolia\001d1664_1.fuz",
    r"sound\voice\fallout4.esm\npcfmagnolia\001d1666_1.fuz",
    r"sound\voice\fallout4.esm\npcfmagnolia\001d1667_1.fuz",
    r"sound\voice\fallout4.esm\npcfmagnolia\001d1669_1.fuz",
    r"sound\voice\fallout4.esm\npcfmagnolia_muffledreverb\001d1660_1.fuz",
    r"sound\voice\fallout4.esm\npcfmagnolia_muffledreverb\001d1667_1.fuz",
    r"sound\voice\fallout4.esm\npcfmagnolia_muffledreverb\001d1669_1.fuz",
    r"sound\voice\fallout4.esm\npcfmagnolia_normalreverb\001d1661_1.fuz",
    r"sound\voice\fallout4.esm\npcfmagnolia_normalreverb\001d1664_1.fuz",
    r"sound\voice\fallout4.esm\npcfmagnolia_normalreverb\001d1666_1.fuz",
];

struct Expected {
    entries: usize,
    short: usize,
    formats: &'static [(u16, u32, usize)],
    over_budget: &'static [&'static str],
    empty_lip: usize,
}

#[test]
#[ignore = "requires the installed Skyrim SE and Fallout 4 voice archives"]
fn vanilla_fuz_voice_corpus() {
    let skyrim_path = skyrim_se_data_dir().join("Skyrim - Voices_en0.bsa");
    let fallout_path = fo4_data_dir().join("Fallout4 - Voices.ba2");
    let skyrim = BsaArchive::open(&skyrim_path)
        .unwrap_or_else(|error| panic!("{}: {error}", skyrim_path.display()));
    let fallout = Ba2Archive::open(&fallout_path)
        .unwrap_or_else(|error| panic!("{}: {error}", fallout_path.display()));

    let skyrim_passed = check_archive(
        "Skyrim - Voices_en0.bsa",
        skyrim.list_files(),
        |path| skyrim.extract(path),
        Expected {
            entries: 75_408,
            short: 389,
            formats: &[(1, 44_100, 74_869), (2, 44_100, 539)],
            over_budget: &[],
            empty_lip: 0,
        },
    );
    let fallout_passed = check_archive(
        "Fallout4 - Voices.ba2",
        fallout.list_files(),
        |path| fallout.extract(path),
        Expected {
            entries: 116_101,
            short: 156,
            formats: &[(1, 44_100, 115_709), (2, 48_000, 392)],
            over_budget: MAGNOLIA_SONGS,
            empty_lip: 47,
        },
    );
    assert!(
        skyrim_passed && fallout_passed,
        "voice corpus differed from the expected results"
    );
}

fn check_archive(
    name: &str,
    mut paths: Vec<&str>,
    extract: impl Fn(&str) -> io::Result<Vec<u8>>,
    expected: Expected,
) -> bool {
    let start = Instant::now();
    paths.retain(|path| {
        path.rsplit_once('.')
            .is_some_and(|(_, ext)| ext.eq_ignore_ascii_case("fuz"))
    });
    paths.sort_unstable();
    let entries = paths.len();
    let failures = SoundFailures::default();
    let mut formats = BTreeMap::new();
    let mut playable = 0;
    let mut short = 0;
    let mut empty_lip = 0;
    let mut rejected = Vec::new();
    let mut other_failures = BTreeMap::new();

    for (index, path) in paths.into_iter().enumerate() {
        let check = check_voice(path, || extract(path), &failures, index < LIP_SAMPLES);
        if let Some(metadata) = check.metadata {
            *formats
                .entry((metadata.channels, metadata.sample_rate))
                .or_insert(0) += 1;
        }
        match check.empty_lip {
            Ok(empty) => empty_lip += usize::from(empty),
            Err(reason) => {
                other_failures.insert(path, reason.to_owned());
            }
        }
        match check.playback {
            Some(Ok(is_short)) => {
                playable += 1;
                short += usize::from(is_short);
            }
            Some(Err(reason)) => {
                other_failures.entry(path).or_insert(reason);
            }
            None => rejected.push(path),
        }
    }

    let failure_entries: BTreeMap<_, _> = failures.entries().into_iter().collect();
    let mut over_budget = Vec::new();
    for path in rejected {
        let reason = failure_entries
            .get(path)
            .map_or("voice cache returned None", String::as_str);
        if reason.contains(STATIC_BUDGET_REASON) {
            over_budget.push(path);
        } else {
            other_failures
                .entry(path)
                .or_insert_with(|| reason.to_owned());
        }
    }
    let formats: Vec<_> = formats
        .into_iter()
        .map(|((channels, sample_rate), count)| (channels, sample_rate, count))
        .collect();
    let expected_playable = expected.entries - expected.over_budget.len();
    println!("{name}");
    println!("  entries: {entries} (expected {})", expected.entries);
    println!("  playable: {playable} (expected {expected_playable})");
    println!(
        "  short against dpds: {short} (expected {})",
        expected.short
    );
    println!(
        "  source formats (channels, Hz, entries): {formats:?} (expected {:?})",
        expected.formats
    );
    println!(
        "  empty lip payloads in {LIP_SAMPLES} samples: {empty_lip} (expected {})",
        expected.empty_lip
    );
    println!(
        "  over the static budget: {} (expected {})",
        over_budget.len(),
        expected.over_budget.len()
    );
    for path in &over_budget {
        println!("    {path}: {}", failure_entries[*path]);
    }
    if over_budget != expected.over_budget {
        println!("  expected over-budget paths: {:?}", expected.over_budget);
    }
    println!("  other failures: {}", other_failures.len());
    for (path, reason) in other_failures.iter().take(FAILURE_LIMIT) {
        println!("    {path}: {reason}");
    }
    println!("  SoundFailures entries: {}", failure_entries.len());
    println!("  wall-clock: {:.3}s", start.elapsed().as_secs_f64());

    entries == expected.entries
        && playable == expected_playable
        && short == expected.short
        && formats == expected.formats
        && empty_lip == expected.empty_lip
        && over_budget == expected.over_budget
        && other_failures.is_empty()
        && failure_entries.len() == over_budget.len()
}

struct VoiceCheck {
    metadata: Option<Metadata>,
    empty_lip: Result<bool, &'static str>,
    playback: Option<Result<bool, String>>,
}

fn check_voice(
    path: &str,
    extract: impl FnOnce() -> io::Result<Vec<u8>>,
    failures: &SoundFailures,
    sample_lip: bool,
) -> VoiceCheck {
    // Keep both PCM owners local to this line (the cache and its returned Arc).
    let mut cache = VoiceSoundCache::with_budget_and_failures(
        VoiceSoundCache::DEFAULT_BUDGET_BYTES,
        failures.clone(),
    );
    let mut metadata = None;
    let mut empty_lip = Ok(false);
    let sound = cache.get_or_try_load(path, || {
        let bytes = extract()?;
        if sample_lip {
            empty_lip = check_lip(&bytes);
        }
        let payload = fuz::voice_payload(bytes);
        if let Ok(payload) = &payload {
            metadata = Some(Decoder::open(payload).map(|decoder| decoder.metadata()));
        }
        payload.map(Some)
    });
    let playback = sound.map(|sound| {
        let metadata = metadata
            .as_ref()
            .ok_or("voice loader did not inspect the inner stream")?
            .as_ref()
            .map_err(|error| format!("xWMA metadata: {error}"))?;
        let frames = sound.num_frames();
        let declared = metadata.frame_count as usize;
        if frames == 0 || frames > declared {
            return Err(format!("decoded {frames} frames, dpds declares {declared}"));
        }
        if sound.sample_rate != metadata.sample_rate {
            return Err(format!(
                "decoded sample rate {} Hz, fmt declares {} Hz",
                sound.sample_rate, metadata.sample_rate
            ));
        }
        Ok(frames < declared)
    });
    VoiceCheck {
        metadata: metadata.and_then(Result::ok),
        empty_lip,
        playback,
    }
}

fn check_lip(bytes: &[u8]) -> Result<bool, &'static str> {
    let container = fuz::parse_fuz(bytes).ok_or("invalid FUZE container")?;
    if !container.xwma.starts_with(b"RIFF") || container.xwma.get(8..12) != Some(b"XWMA") {
        return Err("expected an inner RIFF/XWMA stream");
    }
    let declared = u32::from_le_bytes(bytes[8..12].try_into().expect("parsed FUZE header"));
    if container.lip.len() != declared as usize {
        return Err("lip payload length differs from the FUZE header");
    }
    Ok(declared == 0)
}
