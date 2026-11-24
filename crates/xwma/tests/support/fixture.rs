//! Tiny synthetic xWMA fixture shared by decoder and playback tests.

pub fn format(channels: u16) -> [u8; 18] {
    let mut fmt = [0; 18];
    fmt[..2].copy_from_slice(&0x0161u16.to_le_bytes());
    fmt[2..4].copy_from_slice(&channels.to_le_bytes());
    fmt[4..8].copy_from_slice(&48_000u32.to_le_bytes());
    fmt[8..12].copy_from_slice(&6_000u32.to_le_bytes());
    fmt[12..14].copy_from_slice(&32u16.to_le_bytes());
    fmt[14..16].copy_from_slice(&16u16.to_le_bytes());
    fmt
}

pub fn set_riff_len(bytes: &mut [u8]) {
    let len = (bytes.len() - 8) as u32;
    bytes[4..8].copy_from_slice(&len.to_le_bytes());
}

pub fn riff(chunks: &[(&[u8; 4], &[u8])]) -> Vec<u8> {
    let mut bytes = b"RIFF\0\0\0\0XWMA".to_vec();
    for &(name, payload) in chunks {
        bytes.extend_from_slice(name);
        bytes.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        bytes.extend_from_slice(payload);
        if payload.len() % 2 != 0 {
            bytes.push(0);
        }
    }
    set_riff_len(&mut bytes);
    bytes
}

pub fn stream(fmt: &[u8], frames: u32) -> Vec<u8> {
    let channels = u32::from(u16::from_le_bytes([fmt[2], fmt[3]]));
    // Four silent codec frames before priming. FFmpeg drops the first two
    // and emits one on drain. The zero-coded channels use the packet's padding.
    riff(&[
        (b"JUNK", &[1, 2, 3]),
        (b"data", &[5]),
        (b"dpds", &(frames * channels * 2).to_le_bytes()),
        (b"fmt ", fmt),
    ])
}
