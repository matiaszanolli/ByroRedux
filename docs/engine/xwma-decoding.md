# xWMA Decoding

**Status**: PROPOSED (2026-11-24).

This design covers the in-process decoder of the
[xWMA Audio Decoding PRD](../prd/xwma-audio.md). The
[Phase 0 findings](xwma-phase0-findings.md) record the evidence
for the container, oracle and unresolved codec details.

**Decided** (2026-11-24, PRD Q1): the codec is FFmpeg's `wmav2` decoder, vendored at one
pinned release and linked statically with every other component disabled. The
public-description search found no source to write a decoder from
([V2](xwma-phase0-findings.md#v2--public-wma-v2-descriptions)), and the other existing
decoders were set aside ([V2 addendum](xwma-phase0-findings.md#v2-addendum--existing-decoders-for-alternative-b)).
The sections below describe the wrapper around that codec: the byte-source boundary, the
playback integration, the streaming bound and the build and oracle rules. Where a stage says
"codec", it means libavcodec's decoder instance.

## Decoder boundary

A Rust wrapper crate takes a bounded, seekable byte source
and exposes stream metadata and incremental PCM output. The wrapper owns RIFF
parsing and packet framing; libavcodec owns the codec state. The audio crate owns asset paths,
conversion to playback frames, and the kira adapters. The game layer owns
music requests and their translation to directives. The same codec entry
point consumes loose and archived assets from either game.

| Stage | Contract |
|---|---|
| Open | Validate the RIFF `XWMA` form and the `fmt `, `dpds` and `data` ranges. Check little-endian lengths, odd-chunk padding, overflow and enclosing source bounds. Skip well-formed unknown chunks and reject ambiguous duplicate required chunks. A supported stream reports channels, sample rate and a validated presentation frame count. |
| Configure | Support `WAVE_FORMAT_WMAUDIO2` (`0x0161`), mono and stereo. Verify the declared channels, rate and block alignment before opening the codec or allocating. Report unsupported tags with the tag value. Accept only the variants V7 has shown the vendored build decodes. |
| Read | Use the declared compressed packet boundaries and the packet table's cumulative decoded byte counts. Check the table shape, count, monotonicity and PCM-frame alignment. Keep table access over the source range or in a fixed-size cache. |
| Decode | Send one `block_align` packet at a time to the codec and receive its frames one at a time, copying each into a bounded buffer, so multiple frames in one compressed packet never cause a packet-sized PCM allocation. The wrapper bounds every input it hands the codec: packet size, packet count and the declared duration. |
| Drain | Account for codec priming, trailing padding and overlap in one presentation timeline. Ordinary EOF requires the validated presentation count to have been delivered. Early EOF, excess output under the verified count rules, and an incomplete packet become terminal errors. |

Microsoft's `dpds` table supplies a decoded-size declaration, and its byte
units must be converted using the validated PCM width and channel count.
The [V3 radio fixture](xwma-phase0-findings.md#v3--candidate-oracle-and-tolerance)
shows a 512-frame difference between the declaration and FFmpeg output from
a complete file. Establish its presentation-length rule with an independent
oracle before freezing the open contract. An unexplained discrepancy cannot
pass by adding silence, trimming to the shorter result or widening the
sample tolerance.

The bitstream parser is C code reading files that mods supply. The wrapper treats it as
untrusted-input handling: inputs are bounded before the call, a decode step either
consumes input, emits PCM or ends, and an error invalidates the decoder instance. A
discarded instance can be reopened from its original source. This gives
static and streamed callers the same error semantics and prevents continued
playback from partial state. A corpus of truncated and bit-flipped copies of vanilla
files is the standing test of that boundary (PRD scenario 6). [PRD decoder requirements](../prd/xwma-audio.md#51-the-decoder-xwma-dec).

## Playback integration

| Route | Integration |
|---|---|
| Static effects | Recognize xWMA at the existing byte-loading boundary, decode through the shared codec, and hand the completed frames to `StaticSoundData`. Enforce a checked static allocation limit before reserving. Cache on successful decode through the existing path-keyed sound cache. |
| Streamed music | Supply the common [streaming decoder](../../crates/audio/src/stream_decoder.rs) with a WMA v2 packet/frame backend. Retain kira's streaming voice and its normal control, playback, and error paths. Decoding runs off the mixer callback. |
| Voice | If the owner selects the in-process voice route, [the FUZE reader](../../crates/audio/src/fuz.rs) keeps ownership of the wrapper and lip data and supplies the inner xWMA slice. The voice caller keeps subtitle-estimate fallback on failure. |

The audio caller attaches the resolved asset path and reason to decoder
errors and deduplicates reports per session. A music decoder error ends
the stream and changes the directive to Unresolved with its reason.
The codec receives no music-stack state. [Music](music.md) owns music
selection, looping, mixing and reporting. Translation preserves authored
palette separators, cue points and loop data as described in the
[Phase 0 music findings](xwma-phase0-findings.md#v4--palette-layout-loop-data-and-fallout-4-behavior).

The streaming adapter consumes codec-internal zero-output progress before
returning to kira. Each call returns a nonempty chunk, an error, or EOF
after the advertised last frame. Returning an empty chunk early makes
kira's scheduler keep requesting the same frame. On truncation reuse the
existing claim-checking behavior and surface the terminal error to the
controller. [ClaimCheckedDecoder](../../crates/audio/src/stream_decoder.rs).

For seek and loop restart, flush the codec (`avcodec_flush_buffers`) and drop pending
output together. Use `dpds` for frame-to-packet lookup. A packet selected
that way still needs a verified restart point and the required decoder
preroll. If no checkpoint is known, replay incrementally from the start
and discard through the requested position. Perform nontrivial replay on
the stream worker with cancellation and bounded work per step. Use the
same presentation timeline for restart, seek and loop point conversion.
The decoded-audio cap applies throughout preroll and a loop restart.

## Streaming bound for XWMA-STRM-002

**Per live voice, retain at most 36,864 decoded frame slots** from the
codec through kira's per-voice handoff. A frame slot holds simultaneous
samples for at most two channels, including mono duplicated into kira's
stereo frame. Count allocations at capacity and count simultaneous
copies, including during chunk replacement and seeking.

| Owner | Maximum frame slots |
|---|---:|
| kira 0.12.5 per-voice ring, including its interpolation seed | 16,384 |
| Prior and incoming adapter chunks, at most 4,096 each | 8,192 |
| Codec PCM and time-domain overlap/scratch | 8,192 |
| Remaining per-voice copies and interpolation | 4,096 |
| Total | **36,864** |

The cap corresponds to **576 KiB** at a conservative 16 bytes per slot
including a timestamp, or about 0.84 seconds of frame slots at 44.1 kHz.
The shared device mix buffer, compressed source and packet index are
outside the decoded-frame count. Track the compressed input separately
under existing asset-loading limits. Cap per-voice codec scratch and
adapter capacity before decoding. An archive may require materializing
its compressed xWMA entry through the asset provider. The entire track
must never be staged as PCM.

The ring depth comes from kira 0.12.5
`src/sound/streaming/sound/decode_scheduler.rs:18,42–58,112–139`.
Lines 147–179 retain a previous decode chunk while the next one is being
produced. The mixer takes a four-frame interpolation window
(`src/sound/streaming/sound.rs:140–155`). Recheck these capacities on a
kira upgrade. A crossfade or palette counts each live voice independently
and sums its usage against the number of live voices.

The largest `dpds` advance observed in a single music packet was 31,616
decoded frames in Skyrim and 30,720 in Fallout 4. The adapter must be able
to pause within that packet and release output in bounded chunks.

| Archive searched | Tracks | Longest track by declared duration | Frames at rate | Duration |
|---|---:|---|---|---:|
| `Skyrim - Sounds.bsa` | 269 | `music\explore\mus_explore_day_09.xwm` | 23,762,944 at 44,100 Hz | 538.842 s |
| `Fallout4 - Sounds.ba2` | 459 | `music\mus_public_02.xwm` | 18,132,992 at 48,000 Hz | 377.771 s |

For the acceptance test, enumerate the two archives from the PRD's data
roots and select the longest by validated `frames / sample_rate` across
both. The current longest Skyrim stream has SHA-256
`a7a40ae59a42984d0ced960ae702ffea2c8128399c190e69479e227275fd3cc6`.
Play it to EOF through the regular streaming adapter with a paced test
consumer, including backpressure, seek and loop restart. Count PCM
capacity and simultaneous ownership at allocation and replacement points,
including the ring, staging buffers, decoder PCM storage and scratch.
Assert a high-water mark of at most 36,864 frame slots throughout and
exact emitted length in an uninterrupted pass. Use the Fallout 4 maximum
as the second archive regression. Exercise a clipped data chunk and assert
a terminal error and voice stop. Surface the measured peak with the file
digest and metadata.

## Build, provenance and oracle

The codec is not written here, so there is no clean-room record. What the repository owes
instead is a reproducible build and a licence trail.

- **One pinned release, vendored.** The release identifier, its source digest, the licence
  notice of every file compiled and the exact configure line live in the repository,
  vendored the way `third_party/ruffle` is. The configure line disables every component but
  the `wmav2` decoder and what it needs, and disables assembly. V3 measured a one-count
  difference between CPU-optimised and scalar runs of one build, so a scalar build makes the
  decoded samples the same on every machine (PRD XWMA-DEC-006).
- **Static link, no FFmpeg on the machine.** The engine neither starts the `ffmpeg` binary
  nor loads a system `libavcodec` (PRD XWMA-DEC-005). The build needs a C toolchain; V7
  records what else it needs and how the cross-platform builds PRD is affected.
- **Licence.** The library is LGPL-2.1-or-later as a whole; V7 confirms the licence of the
  files a `wmav2`-only build compiles. The engine ships the notice and the means to relink.
- **The oracle is the same code.** The `ffmpeg` command line of the same pinned release,
  run out of process on the same file, decodes it with the command in the
  [V3 findings](xwma-phase0-findings.md#v3--candidate-oracle-and-tolerance). Agreement
  proves the wrapper's packetising and sample conversion, not the codec. The
  codec is adjudicated against Microsoft's decoder wherever the two disagree, starting with
  the Fallout 4 radio file whose output is 512 frames shorter than its `dpds` declaration.
- **What is recorded per file** is the corpus input hash, stream format, oracle version and
  invocation, reported and actual frames, first differing sample and maximum absolute
  signed-16-bit difference. Exact frame count, channel order, sample rate and alignment are
  checked before the one-count tolerance. Digests and counts are checked in; copyrighted
  media and decoded audio stay in the local test installation.
- **Fallback.** If V7 finds the compiled files' licence unusable, PRD Q1 returns to the
  owner, and the clean-room route and its source rules (published descriptions, reproducible
  black-box observations, no disassembly of a game or vendor binary) are the alternative.
