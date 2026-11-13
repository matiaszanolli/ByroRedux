# byroredux-bink2

A decoder for Bink Video 2 (`KB2g`, `KB2j`), the format of Fallout 4's, Fallout 76's and Starfield's
movies, written without reading anyone else's decoder (PRD
[`bink-video-playback`](../../docs/prd/bink-video-playback.md), BIK-DEC-003, decision Q2). Pure Rust,
no dependencies, MIT.

**Read [`CLEANROOM.md`](CLEANROOM.md) before changing anything here.** Every rule in the code carries
the tag of the experiment that established it; the experiments are in
[`docs/spec/probes.md`](docs/spec/probes.md), the state and the open items in
[`docs/spec/STATE.md`](docs/spec/STATE.md).

## What it decodes
Every frame of every Bink 2 file of the three games — 54 files, 74,680 frames — decodes to planes
identical to those of the games' own `bink2w64.dll` (measured 2026-11-13). Not decoded: alpha planes
and other revisions (Wolfenstein II's `KB2i`, for one). The container also reads the audio track
headers and packets (`Bk2::tracks`, `Bk2::audio`); the sound itself is
[`byroredux-binkaudio`](../binkaudio/README.md)'s. On a Ryzen 9 7950X the 4K files take 10-13 ms a
frame on one thread and about 4 ms on four.

## Use

```rust
let mut file = byroredux_bink2::Bk2::open(path)?;
let mut decoder = byroredux_bink2::Decoder::for_file(&file);
decoder.options.threads = 4; // slices at once; the frames are the same
for i in 0..file.frames {
    let packet = file.video(i)?;
    let frame = decoder.decode(&packet, file.is_key(i))?;
    // frame.planes: Y, then the two chroma planes at half size, padded to whole 32x32
    // macroblocks; frame.width x frame.height of it is the picture.
}
```

The scalar code is the reference; AVX2 kernels are chosen at run time and compute the same frames
(`set_simd(false)` turns them off).

## Changing it
A new rule needs an experiment against the DLL, recorded in `docs/spec/probes.md`, and the decoder
must stay exact on the whole corpus. Both need the research repository, `bink2-clean`: the oracle
harness that runs the game's DLL under Wine through its exported functions, the probe streams and
tools, the Python reference the rules were first written in, and `bk2cmp`, which builds against this
crate and compares it with the oracle (`bk2cmp coverage all …`, `bk2cmp bench …`). The crate's own
tests (`cargo test -p byroredux-bink2`) cover the bit reader, the transform and each AVX2 kernel
against its scalar twin.
