**Severity**: LOW · **Dimension**: BSA v105 (LZ4)
**Location**: `crates/bsa/src/archive/tests.rs:533-655` (the 5 real-archive tests carry `#[ignore]`)
**Source**: AUDIT_SKYRIM_2026-06-14 (SK-D5-01)

## Description
The five tests that exercise the real Skyrim v105 archives (frame-codec LZ4 decompression, 24-byte folder-record stride, embed-name path) are all `#[ignore]`'d (they require on-disk game data), so default `cargo test` never gates them. The unconditional synthetic tests encode+decode with the *same* `lz4_flex::frame` codec, so they cannot catch a wrong-codec regression (e.g. an accidental swap to the block codec that Starfield's BA2 uses).

## Evidence
`tests.rs` carries `#[ignore]` on the real-archive tests (confirmed at lines 534, 561, 591; in-source comment at `:520` documents the CI-green rationale); ran manually `cargo test -p byroredux-bsa --lib -- --ignored skyrim` → 5/5 pass against real archives; full lib suite 50 passed / 0 failed. The frame-vs-block distinction is real: raw inspection of `Skyrim - Meshes0.bsa` shows the LZ4 frame magic `0x184D2204` after the 4-byte size prefix, and the reader correctly uses `lz4_flex::frame::FrameDecoder` (`extract.rs:128-132`) — Starfield's BA2 (`ba2.rs:694`) correctly uses the block codec.

## Impact
A future refactor swapping the v105 codec would pass default CI and only surface when a real Skyrim BSA loads. Latent regression hazard, no current defect.

## Suggested Fix
Either commit a tiny synthetic v105 archive fixture that round-trips through the frame codec unconditionally, or add a CI job that runs the `--ignored skyrim` subset when game data is mounted.

## Completeness Checks
- [ ] **TESTS**: An unconditional fixture pins the frame codec (vs the block codec) for v105
