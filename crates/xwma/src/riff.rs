use std::{
    io::{Read, Seek, SeekFrom},
    ops::Range,
};

use crate::{Error, Metadata};

const WMAUDIO2: u16 = 0x0161;
const MAX_SOURCE_BYTES: u64 = 64 * 1024 * 1024; // V7 harness bound.
const MAX_PACKET_BYTES: u16 = 32_768; // wmav2's compressed superframe limit.
const MAX_PACKETS: u64 = 1_000_000;
const MAX_DURATION_SECONDS: u32 = 60 * 60;
// Keep the codec's rate-dependent arithmetic within supported input ranges.
const SAMPLE_RATES: Range<u32> = 8_000..48_001;
const MAX_AVG_BYTES_PER_SEC: u32 = 125_000;

pub(crate) struct Stream {
    pub metadata: Metadata,
    pub avg_bytes_per_sec: u32,
    pub block_align: u16,
    pub data_len: u64,
}

pub(crate) fn open(source: &mut (impl Read + Seek)) -> Result<Stream, Error> {
    let start = source.stream_position()?;
    let end = source.seek(SeekFrom::End(0))?;
    let len = end
        .checked_sub(start)
        .ok_or(Error::InvalidRiff("source starts past its end"))?;
    if !(12..=MAX_SOURCE_BYTES).contains(&len) {
        return Err(Error::InvalidRiff("source size outside supported bounds"));
    }
    source.seek(SeekFrom::Start(start))?;
    let mut header = [0; 12];
    source.read_exact(&mut header)?;
    if &header[..4] != b"RIFF" || &header[8..] != b"XWMA" {
        return Err(Error::InvalidRiff("expected RIFF/XWMA form"));
    }
    if u64::from(le_u32(&header, 4)) + 8 != len {
        return Err(Error::InvalidRiff("RIFF length does not match source"));
    }

    let mut fmt = None;
    let mut dpds = None;
    let mut data = None;
    let mut pos = start + 12;
    while pos < end {
        if end - pos < 8 {
            return Err(Error::InvalidRiff("truncated chunk header"));
        }
        source.seek(SeekFrom::Start(pos))?;
        let mut chunk = [0; 8];
        source.read_exact(&mut chunk)?;
        let len = u64::from(le_u32(&chunk, 4));
        let payload = pos + 8;
        // Compare against the remaining range before computing the offsets.
        let padded_len = len + (len & 1);
        if padded_len > end - payload {
            return Err(Error::InvalidRiff(
                "chunk or its padding exceeds RIFF bounds",
            ));
        }
        let (range, duplicate) = match &chunk[..4] {
            b"fmt " => (&mut fmt, "duplicate fmt chunk"),
            b"dpds" => (&mut dpds, "duplicate dpds chunk"),
            b"data" => (&mut data, "duplicate data chunk"),
            _ => {
                pos = payload + padded_len;
                continue;
            }
        };
        if range.replace(payload..payload + len).is_some() {
            return Err(Error::InvalidRiff(duplicate));
        }
        pos = payload + padded_len;
    }

    let fmt = fmt.ok_or(Error::InvalidRiff("missing fmt chunk"))?;
    let dpds = dpds.ok_or(Error::InvalidRiff("missing dpds chunk"))?;
    let data = data.ok_or(Error::InvalidRiff("missing data chunk"))?;
    let (channels, sample_rate, avg_bytes_per_sec, block_align) = read_format(source, fmt)?;
    let data_len = data.end - data.start;
    let packets = data_len.div_ceil(u64::from(block_align));
    if packets == 0 || packets > MAX_PACKETS {
        return Err(Error::InvalidRiff("packet count outside supported bounds"));
    }
    let frame_count = read_packet_table(source, dpds, packets, u32::from(channels) * 2)?;
    if frame_count > sample_rate * MAX_DURATION_SECONDS {
        return Err(Error::InvalidRiff("declared duration exceeds one hour"));
    }

    source.seek(SeekFrom::Start(data.start))?;
    Ok(Stream {
        metadata: Metadata {
            channels,
            sample_rate,
            frame_count,
        },
        avg_bytes_per_sec,
        block_align,
        data_len,
    })
}

fn read_format(
    source: &mut (impl Read + Seek),
    range: Range<u64>,
) -> Result<(u16, u32, u32, u16), Error> {
    let len = range.end - range.start;
    if len < 2 {
        return Err(Error::InvalidRiff("missing format tag"));
    }
    let mut fmt = [0; 18];
    source.seek(SeekFrom::Start(range.start))?;
    source.read_exact(&mut fmt[..2])?;
    let tag = le_u16(&fmt, 0);
    if tag != WMAUDIO2 {
        return Err(Error::UnsupportedTag(tag));
    }
    if len != fmt.len() as u64 {
        return Err(Error::InvalidRiff("expected an 18-byte WMAUDIO2 fmt chunk"));
    }
    source.read_exact(&mut fmt[2..])?;

    let channels = le_u16(&fmt, 2);
    let rate = le_u32(&fmt, 4);
    let avg_bytes = le_u32(&fmt, 8);
    let align = le_u16(&fmt, 12);
    if !matches!(channels, 1 | 2) {
        return Err(Error::InvalidRiff("expected mono or stereo"));
    }
    if !SAMPLE_RATES.contains(&rate) || !(1..=MAX_AVG_BYTES_PER_SEC).contains(&avg_bytes) {
        return Err(Error::InvalidRiff(
            "sample rate or bit rate outside supported bounds",
        ));
    }
    if !(1..=MAX_PACKET_BYTES).contains(&align) {
        return Err(Error::InvalidRiff(
            "block alignment outside supported bounds",
        ));
    }
    if le_u16(&fmt, 14) != 16 || le_u16(&fmt, 16) != 0 {
        return Err(Error::InvalidRiff("expected 16-bit PCM and zero cbSize"));
    }
    Ok((channels, rate, avg_bytes, align))
}

fn read_packet_table(
    source: &mut (impl Read + Seek),
    range: Range<u64>,
    packets: u64,
    bytes_per_frame: u32,
) -> Result<u32, Error> {
    let mut remaining = range.end - range.start;
    if remaining != packets * 4 {
        return Err(Error::InvalidRiff("dpds needs one 32-bit entry per packet"));
    }
    source.seek(SeekFrom::Start(range.start))?;
    let mut buf = [0; 4096];
    let mut previous = 0;
    while remaining > 0 {
        let len = remaining.min(buf.len() as u64) as usize;
        source.read_exact(&mut buf[..len])?;
        for entry in buf[..len].as_chunks::<4>().0 {
            let count = u32::from_le_bytes(*entry);
            if count < previous || count % bytes_per_frame != 0 {
                return Err(Error::InvalidRiff(
                    "dpds counts must be monotone and frame-aligned",
                ));
            }
            previous = count;
        }
        remaining -= len as u64;
    }
    Ok(previous / bytes_per_frame)
}

fn le_u16(bytes: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([bytes[at], bytes[at + 1]])
}

fn le_u32(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
}
