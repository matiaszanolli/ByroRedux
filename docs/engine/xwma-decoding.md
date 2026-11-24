# xWMA Decoding

**Status**: PROPOSED (2026-11-24).

This design covers the in-process decoder option in the
[xWMA Audio Decoding PRD](../prd/xwma-audio.md). The
[Phase 0 findings](xwma-phase0-findings.md) record the evidence
for the container, oracle and unresolved codec details. Implementability
depends on a sufficient and reviewable source for the WMA v2 bitstream.

## Decoder boundary

An independent Rust codec crate would take a bounded, seekable byte source
and expose stream metadata and incremental PCM output. The crate owns RIFF
parsing, packet framing and codec state. The audio crate owns asset paths,
conversion to playback frames, and the kira adapters. The game layer owns
music requests and their translation to directives. The same codec entry
point consumes loose and archived assets from either game.

| Stage | Contract |
|---|---|
| Open | Validate the RIFF `XWMA` form and the `fmt `, `dpds` and `data` ranges. Check little-endian lengths, odd-chunk padding, overflow and enclosing source bounds. Skip well-formed unknown chunks and reject ambiguous duplicate required chunks. A supported stream reports channels, sample rate and a validated presentation frame count. |
| Configure | Support `WAVE_FORMAT_WMAUDIO2` (`0x0161`), mono and stereo. Verify codec configuration and block alignment before allocating. Report unsupported tags with the tag value. Accept only source-backed codec variants. |
| Read | Use the declared compressed packet boundaries and the packet table's cumulative decoded byte counts. Check the table shape, count, monotonicity and PCM-frame alignment. Keep table access over the source range or in a fixed-size cache. |
| Decode | Maintain bounded bit-reservoir, coefficient, transform and overlap state. Decode one coded frame at a time, preserving continuation across calls so multiple frames in one compressed packet never cause a packet-sized PCM allocation. Implement each bitstream field and reconstruction step only once its provenance is recorded. |
| Drain | Account for codec priming, trailing padding and overlap in one presentation timeline. Ordinary EOF requires the validated presentation count to have been delivered. Early EOF, excess output under the verified count rules, and an incomplete packet become terminal errors. |

Microsoft's `dpds` table supplies a decoded-size declaration, and its byte
units must be converted using the validated PCM width and channel count.
The [V3 radio fixture](xwma-phase0-findings.md#v3--candidate-oracle-and-tolerance)
shows a 512-frame difference between the declaration and FFmpeg output from
a complete file. Establish its presentation-length rule with an independent
oracle before freezing the open contract. An unexplained discrepancy cannot
pass by adding silence, trimming to the shorter result or widening the
sample tolerance.

The WMA v2 parser needs a checked bit reader and fixed, source-backed limits
for code lengths, block sizes, packet and reservoir storage, and work per
packet. Every decode step consumes input, advances a bounded state machine,
emits PCM or terminates. An error invalidates the decoder instance. A
discarded instance can be reopened from its original source. This gives
static and streamed callers the same error semantics and prevents continued
playback from partial state. [PRD decoder requirements](../prd/xwma-audio.md#51-the-decoder-xwma-dec).

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

For seek and loop restart, reset reservoir, transform/overlap and pending
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

## Provenance and oracle

If the clean-room route is selected, the codec crate's `CLEANROOM.md`
records every source, its version or snapshot and the facts it supports.
Maintain per-fact references in the codec spec and tie tables, constants
and bit layout to those references. Use published format descriptions and
papers for supported facts, and reproducible black-box observations for
experimentally established facts. Capture probe inputs or generators, the
oracle and version, expected results, and author access or prior exposure.

Use the [Bink Audio provenance record](../../crates/binkaudio/CLEANROOM.md)
and its `binkaudio-clean` comparator as the model. Decoder implementations,
their tables and proprietary game executable disassembly are outside the
allowed source set. Run oracle binaries as external tools with controlled
inputs and outputs. Keep their implementation, linking and installation
outside engine playback. xEdit and CK material documents the music record
boundary. Microsoft and WMA format sources document the audio boundary.

Record the corpus input hash, stream format, oracle version and invocation,
reported and actual frames, first differing sample and maximum absolute
signed-16-bit difference per file. Check exact frame count, channel order,
sample rate and sample alignment before testing the provisional one-count
PCM tolerance. Capture digests and counts in the repository and keep
copyrighted media and decoded audio in the local test installation.
Missing bitstream facts and unresolved oracle length discrepancies stay
visible in the spec and gate implementation or acceptance as applicable.
[Phase 0 oracle evidence](xwma-phase0-findings.md#v3--candidate-oracle-and-tolerance).
