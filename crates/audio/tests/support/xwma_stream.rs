use std::{
    io::Cursor,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering::Relaxed},
    },
    thread,
    time::{Duration, Instant},
};

use byroredux_bsa::{Ba2Archive, BsaArchive};
use kira::{
    AudioManager, AudioManagerSettings,
    backend::mock::{MockBackend, MockBackendSettings},
    sound::{PlaybackPosition, PlaybackState},
};

use super::*;
use crate::tests::xwma_fixture::{format, riff, stream};

const FRAME_LIMIT: usize = 36_864;
const CHUNK_LIMIT: usize = 4_096;
// kira 0.12.5 DecodeScheduler::BUFFER_SIZE, including its seed. The mixer
// copies four frames for interpolation and passes individual frames by value.
const KIRA_RING: usize = 16_384;
const KIRA_FRAME_COPIES: usize = 4_096; // Reserve covers the mixer/producer temporaries.

struct MeasuredStream {
    stream: XwmaStream,
    prior_chunk: usize,
    peak: Arc<AtomicUsize>,
    restarts: Arc<AtomicUsize>,
}

impl MeasuredStream {
    fn new(bytes: Vec<u8>) -> Self {
        Self {
            stream: XwmaStream::new(Box::new(Cursor::new(bytes))).unwrap(),
            prior_chunk: 0,
            peak: Arc::default(),
            restarts: Arc::default(),
        }
    }

    fn measure(&self, incoming_chunk: usize) {
        assert!(incoming_chunk <= CHUNK_LIMIT);
        let slots = KIRA_RING
            + KIRA_FRAME_COPIES
            + self.prior_chunk
            + incoming_chunk
            + self.stream.codec.pcm_frame_slots();
        self.peak.fetch_max(slots, Relaxed);
        assert!(slots <= FRAME_LIMIT, "retained {slots} decoded frame slots");
    }
}

impl Decoder for MeasuredStream {
    type Error = FromFileError;

    fn sample_rate(&self) -> u32 {
        self.stream.sample_rate()
    }

    fn num_frames(&self) -> usize {
        self.stream.num_frames()
    }

    fn decode(&mut self) -> Result<Vec<Frame>, Self::Error> {
        let frames = self.stream.decode()?;
        // kira retains the previous Vec while decode constructs the new one.
        self.measure(frames.capacity());
        self.prior_chunk = frames.capacity();
        Ok(frames)
    }

    fn seek(&mut self, index: usize) -> Result<usize, Self::Error> {
        let landed = self.stream.seek(index)?;
        self.measure(0);
        self.restarts.fetch_add(1, Relaxed);
        Ok(landed)
    }
}

fn mock() -> AudioManager<MockBackend> {
    AudioManager::new(AudioManagerSettings {
        backend_settings: MockBackendSettings {
            sample_rate: 48_000,
        },
        internal_buffer_size: 64,
        ..Default::default()
    })
    .unwrap()
}

fn consume_until(manager: &mut AudioManager<MockBackend>, mut done: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !done() {
        assert!(Instant::now() < deadline, "stream playback stalled");
        let mixer = manager.backend_mut();
        mixer.on_start_processing();
        mixer.process();
        thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn xwma_playback_reaches_the_exact_declared_end() {
    let bytes = stream(&format(2), 6_144);
    let mut decoder = MeasuredStream::new(bytes.clone());
    let mut frames = 0;
    loop {
        let chunk = decoder.decode().unwrap();
        if chunk.is_empty() {
            break;
        }
        frames += chunk.len();
    }
    assert_eq!(frames, 6_144);

    let data = crate::load_streaming_sound_from_bytes(bytes).unwrap();
    assert_eq!(data.num_frames(), frames);
    let mut manager = mock();
    let mut handle = manager.play(data).unwrap();
    consume_until(&mut manager, || handle.state() == PlaybackState::Stopped);
    assert!(handle.pop_error().is_none());
    assert!(handle.position() >= (frames - 64) as f64 / 48_000.0);
}

#[test]
fn xwma_claiming_more_frames_than_it_holds_stops_with_an_error() {
    let mut manager = mock();
    let bytes = stream(&format(2), 6_656);
    let data = crate::load_streaming_sound_from_bytes(bytes).unwrap();
    let mut handle = manager.play(data.loop_region(0.0..)).unwrap();
    consume_until(&mut manager, || handle.state() == PlaybackState::Stopped);
    let Some(FromFileError::IoError(error)) = handle.pop_error() else {
        panic!("expected a terminal decode error");
    };
    assert_eq!(error.kind(), io::ErrorKind::UnexpectedEof);
    assert_eq!(
        error.to_string(),
        "xWMA declared 6656 frames and wmav2 produced 6144"
    );
}

#[test]
fn malformed_xwma_returns_an_error() {
    let mut bytes = stream(&format(2), 6_144);
    bytes.pop();
    assert!(crate::load_streaming_sound_from_bytes(bytes).is_err());
}

#[test]
fn xwma_paced_playback_seek_and_loop_restart_stay_bounded() {
    // Build a stream long enough to fill the ring. After the first packet,
    // each of these synthetic packets completes the previous tail and emits
    // four more silent frames. The codec primes two and drains one.
    let packets = 8u32;
    let mut data = vec![0; packets as usize * 32];
    for packet in data.as_chunks_mut::<32>().0 {
        packet[0] = 5;
    }
    let table: Vec<u8> = (1..=packets)
        .flat_map(|packets| ((5 * packets - 2) * 2048 * 4).to_le_bytes())
        .collect();
    let bytes = riff(&[(b"fmt ", &format(2)), (b"dpds", &table), (b"data", &data)]);
    let decoder = MeasuredStream::new(bytes);
    let peak = Arc::clone(&decoder.peak);
    let restarts = Arc::clone(&decoder.restarts);
    let mut manager = mock();
    let begin = 4_096;
    let end = 28_672;
    let mut handle = manager
        .play(
            StreamingSoundData::from_decoder(decoder)
                .loop_region(PlaybackPosition::Samples(begin)..PlaybackPosition::Samples(end)),
        )
        .unwrap();

    // Pace the real kira worker, allowing backpressure before issuing a seek.
    consume_until(&mut manager, || handle.position() > 0.05);
    handle.seek_to(8_192.0 / 48_000.0);
    consume_until(&mut manager, || restarts.load(Relaxed) >= 2);
    consume_until(&mut manager, || restarts.load(Relaxed) >= 3);
    handle.set_loop_region(None);
    consume_until(&mut manager, || handle.state() == PlaybackState::Stopped);
    assert!(handle.pop_error().is_none());
    assert!(peak.load(Relaxed) > KIRA_RING + KIRA_FRAME_COPIES + 2 * 2048);
}

#[test]
#[ignore = "requires BYROREDUX_SKYRIMSE_DATA and BYROREDUX_FO4_DATA"]
fn vanilla_xwma_music_corpus() {
    let total_start = Instant::now();
    let root = |name| {
        PathBuf::from(
            std::env::var_os(name).unwrap_or_else(|| panic!("set {name} to the Data directory")),
        )
    };
    let skyrim =
        BsaArchive::open(root("BYROREDUX_SKYRIMSE_DATA").join("Skyrim - Sounds.bsa")).unwrap();
    let fallout =
        Ba2Archive::open(root("BYROREDUX_FO4_DATA").join("Fallout4 - Sounds.ba2")).unwrap();
    assert_archive(
        "Skyrim - Sounds.bsa",
        skyrim.list_files(),
        |name| skyrim.extract(name).unwrap(),
        269,
        ("mus_explore_day_09.xwm", 23_762_944, 44_100),
    );
    assert_archive(
        "Fallout4 - Sounds.ba2",
        fallout.list_files(),
        |name| fallout.extract(name).unwrap(),
        459,
        ("mus_public_02.xwm", 18_132_992, 48_000),
    );
    println!("Streamed both archives in {:.2?}", total_start.elapsed());
}

fn assert_archive(
    archive: &str,
    mut names: Vec<&str>,
    extract: impl Fn(&str) -> Vec<u8>,
    expected_count: usize,
    expected_longest: (&str, usize, u32),
) {
    let start = Instant::now();
    names.retain(|name| {
        let key = name.replace('/', "\\").to_ascii_lowercase();
        key.starts_with("music\\") && key.ends_with(".xwm")
    });
    assert_eq!(names.len(), expected_count, "{archive}");
    names.sort_unstable();
    let mut longest = ("", 0, 1);
    let mut peak = 0;
    for name in names {
        let mut decoder = MeasuredStream::new(extract(name));
        let track = name.rsplit(['\\', '/']).next().unwrap();
        let seek_frame = decoder.num_frames() * 3 / 4;
        let mut seek_sample = None;
        let mut total = 0;
        loop {
            let chunk = decoder
                .decode()
                .unwrap_or_else(|error| panic!("{archive}/{name}: {error}"));
            if chunk.is_empty() {
                break;
            }
            if track == expected_longest.0 && (total..total + chunk.len()).contains(&seek_frame) {
                seek_sample = Some(chunk[seek_frame - total]);
            }
            total += chunk.len();
        }
        assert_eq!(total, decoder.num_frames(), "{archive}/{name}");
        let rate = decoder.sample_rate();
        if total as u64 * u64::from(longest.2) > longest.1 as u64 * u64::from(rate) {
            longest = (track, total, rate);
        }
        if let Some(sample) = seek_sample {
            // Replay from a drained codec and compare with the uninterrupted pass.
            let mut position = decoder.seek(seek_frame).unwrap();
            loop {
                let chunk = decoder.decode().unwrap();
                assert!(
                    !chunk.is_empty(),
                    "seek replay ended before frame {seek_frame}"
                );
                if let Some(replayed) = chunk.get(seek_frame - position) {
                    assert_eq!(*replayed, sample, "{archive}/{name}: PCM after replay");
                    break;
                }
                position += chunk.len();
            }
        }
        peak = peak.max(decoder.peak.load(Relaxed));
    }
    assert_eq!(longest, expected_longest);
    println!(
        "{archive}: {expected_count} clean streams, peak {peak} frame slots, longest {longest:?}, {:.2?}",
        start.elapsed()
    );
}
