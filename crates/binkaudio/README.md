# byroredux-binkaudio

A decoder for Bink Audio, the sound of Fallout 4's, Fallout 76's and Starfield's Bink 2 movies,
written without reading anyone else's decoder (PRD
[`bink-video-playback`](../../docs/prd/bink-video-playback.md), BIK-DEC-002, BIK-DEC-003). Pure Rust,
no dependencies, MIT. The container — track headers and per-frame packets — is
[`byroredux-bink2`](../bink2/README.md)'s `Bk2`.

**Read [`CLEANROOM.md`](CLEANROOM.md) before changing anything here**, including its disclosure:
every rule carries the tag of the experiment that established it, in
[`docs/spec/probes.md`](docs/spec/probes.md).

## What it decodes
Every track of the 17 game files with sound (80 tracks, 938 million samples) decodes to the games'
own `bink2w64.dll` output: the same sample count everywhere, 0.017% of samples off by one where the
DLL's 32-bit float rounding lands on the other side of a .5 (measured 2026-11-13). About 490 times real
time per stereo track on one core. Only what the games use is accepted: 48 kHz, stereo, the DCT
variant; anything else is `Error::Unsupported`. A file's tracks are language versions of one mix
(A9); which ID is which language is not measured.

## Use

```rust
let mut file = byroredux_bink2::Bk2::open(path)?;
let track = file.tracks[0];
let mut decoder = byroredux_binkaudio::Decoder::new(track.rate, track.stereo(), track.dct())?;
let mut pcm = Vec::new(); // interleaved 16-bit samples
for i in 0..file.frames {
    if let Some(packet) = &file.audio(i)?[0] {
        decoder.decode(packet.decoded_bytes, &packet.data, &mut pcm)?;
    }
}
```

## Changing it
A new rule needs an experiment against the DLL, recorded in `docs/spec/probes.md`, and every game
track must stay within one of it. Both need the research repository, `binkaudio-clean`: the oracle
that runs the game's DLL under Wine through its exported track API (`BinkOpenTrack`,
`BinkGetTrackData`), the packet builder and probe tools, and `bacmp`, which builds against this crate
and compares whole files with the oracle. The crate's own test decodes a handcrafted stream to the
DLL's exact samples, without game data.
