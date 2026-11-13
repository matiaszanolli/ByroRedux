//! End to end against the DLL without game data: a handcrafted 96x32 stream of four keyframes (the
//! research repository's `probes/files/pr.bk2`) decodes to the planes the game's `bink2w64.dll` gave
//! for it, here as the FNV-1a 64 hash of each whole padded plane (Y, Cr, Cb). Scalar and AVX2 paths,
//! one thread and several.

use byroredux_bink2::{set_simd, Bk2, Decoder};

/// The probe stream (`tests/data/probe_pr.bk2`, 160 bytes).
const PR: &[u8] = include_bytes!("data/probe_pr.bk2");

/// The oracle's planes of `PR`, per frame.
const ORACLE_FNV: [[u64; 3]; 4] = [
    [0x65961a2b50454325, 0xdb3681d9568edf25, 0xdb3681d9568edf25],
    [0x236bade221aa1325, 0xdb3681d9568edf25, 0xdb3681d9568edf25],
    [0x56d32832fcba1325, 0xdb3681d9568edf25, 0xdb3681d9568edf25],
    [0xa7ce3197110b4325, 0xdb3681d9568edf25, 0xdb3681d9568edf25],
];

fn fnv(data: &[u8]) -> u64 {
    data.iter().fold(0xcbf29ce484222325, |h, &b| (h ^ b as u64).wrapping_mul(0x100000001b3))
}

#[test]
fn a_probe_stream_decodes_to_the_dlls_planes() {
    let path = std::env::temp_dir().join(format!("byroredux-bink2-pr-{}.bk2", std::process::id()));
    std::fs::write(&path, PR).unwrap();
    let mut bk = Bk2::open(&path).unwrap();
    assert_eq!((bk.width, bk.height, bk.frames), (96, 32, 4));
    for (simd, threads) in [(true, 1), (true, 4), (false, 1), (false, 4)] {
        set_simd(simd);
        let mut dec = Decoder::for_file(&bk);
        dec.options.threads = threads;
        for (i, want) in ORACLE_FNV.iter().enumerate() {
            let packet = bk.video(i).unwrap();
            let frame = dec.decode(&packet, bk.is_key(i)).unwrap();
            let got: Vec<u64> = frame.planes.iter().map(|p| fnv(&p.px)).collect();
            assert_eq!(got, want, "frame {i}, simd {simd}, threads {threads}");
        }
    }
    set_simd(true);
    let _ = std::fs::remove_file(&path);
}
