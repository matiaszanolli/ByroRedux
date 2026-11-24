use std::io::Cursor;

use crate::{Decoder, Error, Metadata, to_s16};

fn format() -> [u8; 18] {
    let mut fmt = [0; 18];
    fmt[..2].copy_from_slice(&0x0161u16.to_le_bytes());
    fmt[2..4].copy_from_slice(&2u16.to_le_bytes());
    fmt[4..8].copy_from_slice(&48_000u32.to_le_bytes());
    fmt[8..12].copy_from_slice(&6_000u32.to_le_bytes());
    fmt[12..14].copy_from_slice(&32u16.to_le_bytes());
    fmt[14..16].copy_from_slice(&16u16.to_le_bytes());
    fmt
}

fn set_riff_len(bytes: &mut [u8]) {
    let len = (bytes.len() - 8) as u32;
    bytes[4..8].copy_from_slice(&len.to_le_bytes());
}

fn riff(chunks: &[(&[u8; 4], &[u8])]) -> Vec<u8> {
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

fn stream(fmt: &[u8], frames: u32) -> Vec<u8> {
    // Four silent codec frames before priming. FFmpeg drops the first two
    // and emits one on drain. The zero-coded channels use the packet's padding.
    riff(&[
        (b"JUNK", &[1, 2, 3]),
        (b"data", &[5]),
        (b"dpds", &(frames * 4).to_le_bytes()),
        (b"fmt ", fmt),
    ])
}

fn open_error(bytes: &[u8]) -> Error {
    match Decoder::open(bytes) {
        Err(error) => error,
        Ok(_) => panic!("malformed xWMA accepted"),
    }
}

#[test]
fn decodes_and_drains_a_padded_packet_from_a_seekable_reader() {
    let mut bytes = b"prefix".to_vec();
    bytes.extend(stream(&format(), 6144));
    let mut source = Cursor::new(bytes);
    source.set_position(6);
    let mut decoder = Decoder::from_reader(source).unwrap();
    assert_eq!(
        decoder.metadata(),
        Metadata {
            channels: 2,
            sample_rate: 48_000,
            frame_count: 6144,
        }
    );
    assert_eq!(decoder.next_s16().unwrap(), Some(&[0; 4096][..]));
    assert_eq!(decoder.next_f32().unwrap(), Some(&[0.0; 4096][..]));
    assert_eq!(decoder.next_s16().unwrap(), Some(&[0; 4096][..]));
    assert!(decoder.next_s16().unwrap().is_none());
    assert!(decoder.next_f32().unwrap().is_none());
}

#[test]
fn short_codec_output_returns_its_samples_then_invalidates_the_decoder() {
    let bytes = stream(&format(), 6656);
    let mut decoder = Decoder::open(&bytes).unwrap();
    let mut frames = 0;
    loop {
        match decoder.next_s16() {
            Ok(Some(pcm)) => frames += pcm.len() / 2,
            Err(Error::FrameCountMismatch {
                declared: 6656,
                decoded: 6144,
            }) => break,
            _ => panic!("expected a terminal frame-count mismatch"),
        }
    }
    assert_eq!(frames, 6144);
    assert!(matches!(decoder.next_f32(), Err(Error::Failed)));
}

#[test]
fn malformed_container_bounds_return_errors() {
    let bytes = stream(&format(), 6144);
    for cut in 0..bytes.len() {
        open_error(&bytes[..cut]);
    }

    let mut wrong_form = bytes.clone();
    wrong_form[8..12].copy_from_slice(b"WAVE");
    open_error(&wrong_form);

    let mut huge_riff = bytes.clone();
    huge_riff[4..8].copy_from_slice(&u32::MAX.to_le_bytes());
    open_error(&huge_riff);

    let mut huge_chunk = bytes.clone();
    huge_chunk[16..20].copy_from_slice(&u32::MAX.to_le_bytes());
    open_error(&huge_chunk);

    let mut missing_pad = riff(&[(b"fmt ", &format()), (b"dpds", &[0; 4]), (b"data", &[3])]);
    missing_pad.pop();
    set_riff_len(&mut missing_pad);
    assert!(matches!(
        open_error(&missing_pad),
        Error::InvalidRiff("chunk or its padding exceeds RIFF bounds")
    ));

    let mut truncated_chunk_header = bytes.clone();
    truncated_chunk_header.extend_from_slice(b"data\0\0\0");
    set_riff_len(&mut truncated_chunk_header);
    open_error(&truncated_chunk_header);
}

#[test]
fn required_chunks_are_unique_and_present() {
    let fmt = format();
    let mut chunks: Vec<(&[u8; 4], &[u8])> =
        vec![(b"fmt ", &fmt), (b"dpds", &[0; 4]), (b"data", &[3])];
    for index in 0..chunks.len() {
        let chunk = chunks.remove(index);
        open_error(&riff(&chunks));
        chunks.insert(index, chunk);
        chunks.push(chunk);
        open_error(&riff(&chunks));
        chunks.pop();
    }
}

#[test]
fn rejects_unrecognized_or_unbounded_format_fields() {
    let mut wrong_tag = format();
    wrong_tag[..2].copy_from_slice(&0x0162u16.to_le_bytes());
    let error = open_error(&stream(&wrong_tag, 6144));
    assert!(matches!(error, Error::UnsupportedTag(0x0162)));
    assert!(error.to_string().contains("0x0162"));

    open_error(&stream(&format()[..17], 6144));
    for (offset, value) in [
        (2, 0),
        (2, 3),
        (4, 0),
        (4, u32::MAX),
        (8, 0),
        (8, u32::MAX),
        (12, 0),
        (12, u16::MAX.into()),
        (14, 24),
        (16, 1),
    ] {
        let mut bad_format = format();
        let len = if matches!(offset, 4 | 8) { 4 } else { 2 };
        bad_format[offset..offset + len].copy_from_slice(&value.to_le_bytes()[..len]);
        open_error(&stream(&bad_format, 6144));
    }
}

#[test]
fn rejects_inconsistent_packet_tables_and_unbounded_duration() {
    let fmt = format();
    open_error(&riff(&[
        (b"fmt ", &fmt),
        (b"dpds", &[0; 3]),
        (b"data", &[3]),
    ]));
    open_error(&riff(&[
        (b"fmt ", &fmt),
        (b"dpds", &[2, 0, 0, 0]),
        (b"data", &[3]),
    ]));
    open_error(&riff(&[
        (b"fmt ", &fmt),
        (b"dpds", &[4, 0, 0, 0, 0, 0, 0, 0]),
        (b"data", &[0; 33]),
    ]));
    open_error(&stream(&fmt, 48_000 * 3600 + 1));
}

#[test]
fn s16_conversion_saturates_and_rounds_ties_to_even() {
    assert_eq!(to_s16(1.25), i16::MAX);
    assert_eq!(to_s16(-1.25), i16::MIN);
    assert_eq!(to_s16(0.5 / 32768.0), 0);
    assert_eq!(to_s16(1.5 / 32768.0), 2);
    assert_eq!(to_s16(-1.5 / 32768.0), -2);
    assert_eq!(to_s16(-2.5 / 32768.0), -2);
}
