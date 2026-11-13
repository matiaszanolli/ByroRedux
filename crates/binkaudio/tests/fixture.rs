//! End to end against the DLL without game data: a handcrafted stream (`tests/data/fixture.bk2`, the
//! research repository's `probes/files/fixture.bk2`) — a constant block per channel, cosines, a mix of
//! quantized coefficients across many bands, a fade that wraps (-20000 -> 20000) and a short last
//! packet — decodes to the samples the game's `bink2w64.dll` gave for it, here as the FNV-1a 64 hash of
//! each frame's interleaved 16-bit PCM. Its values were chosen away from rounding ties, where the DLL's
//! 32-bit floats may round the other way (A7).

use byroredux_binkaudio::Decoder;

const DLL_FNV: [u64; 5] =
    [0x4180fdbd6a60b725, 0x1cc4822fb3f9cb4a, 0x1d6ab9df50e972aa, 0x21dc8875e9a810ee, 0xaedc7479d626dbf9];

#[test]
fn a_handcrafted_stream_decodes_to_the_dlls_samples() {
    let path = std::env::temp_dir().join(format!("byroredux-binkaudio-fixture-{}.bk2", std::process::id()));
    std::fs::write(&path, include_bytes!("data/fixture.bk2")).unwrap();
    let mut file = byroredux_bink2::Bk2::open(&path).unwrap();
    let track = file.tracks[0];
    let mut decoder = Decoder::new(track.rate, track.stereo(), track.dct()).unwrap();
    let mut pcm = Vec::new();
    for (i, want) in DLL_FNV.iter().enumerate() {
        let before = pcm.len();
        let packet = file.audio(i).unwrap()[0].clone().unwrap();
        decoder.decode(packet.decoded_bytes, &packet.data, &mut pcm).unwrap();
        let got = pcm[before..]
            .iter()
            .flat_map(|s| s.to_le_bytes())
            .fold(0xcbf29ce484222325u64, |h, b| (h ^ b as u64).wrapping_mul(0x100000001b3));
        assert_eq!(got, *want, "frame {i}");
    }
    assert_eq!(pcm.len(), 4 * 3840 + 1280, "the short last packet keeps its stated samples");
    let _ = std::fs::remove_file(&path);
}

#[test]
fn unmeasured_tracks_are_refused() {
    assert!(Decoder::new(44100, true, true).is_err());
    assert!(Decoder::new(48000, false, true).is_err());
    assert!(Decoder::new(48000, true, false).is_err());
}
