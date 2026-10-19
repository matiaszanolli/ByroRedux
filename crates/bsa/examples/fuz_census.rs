//! Census the FUZE containers in a Skyrim voice BSA: per-file .lip size
//! and the embedded RIFF's fmt tag / channel / rate / chunk layout —
//! the grounding probe for the #5912 xWMA decode question.
//!
//! Usage:
//!   cargo run -p byroredux-bsa --example fuz_census -- <Voices.bsa> [n]

use byroredux_bsa::BsaArchive;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let bsa = &args[1];
    let limit: usize = args.get(2).and_then(|v| v.parse().ok()).unwrap_or(50);
    let archive = BsaArchive::open(bsa).unwrap();
    let mut seen = 0usize;
    let mut tags: std::collections::BTreeMap<u16, usize> = std::collections::BTreeMap::new();
    for f in archive.list_files() {
        if !f.to_ascii_lowercase().ends_with(".fuz") || seen >= limit {
            continue;
        }
        seen += 1;
        let Ok(data) = archive.extract(f) else {
            println!("{}: EXTRACT FAILED", f);
            continue;
        };
        let Some(fmt) = fuz_fmt_summary(&data) else {
            println!("{}: ({}) BAD CONTAINER", f, data.len());
            continue;
        };
        *tags.entry(fmt.tag).or_default() += 1;
        println!(
            "{}: ({} B) lip={} fmt tag={:#06x} ch={} rate={} bits={} align={} byterate={} fmtlen={} chunks={}",
            f,
            data.len(),
            fmt.lip_size,
            fmt.tag,
            fmt.channels,
            fmt.rate,
            fmt.bits,
            fmt.align,
            fmt.byterate,
            fmt.fmt_len,
            fmt.chunks
        );
    }
    println!("\n{seen} .fuz scanned; fmt tags: {tags:?}");
}

struct FuzFmt {
    lip_size: u32,
    tag: u16,
    channels: u16,
    rate: u32,
    bits: u16,
    align: u16,
    byterate: u32,
    fmt_len: usize,
    chunks: String,
}

fn fuz_fmt_summary(data: &[u8]) -> Option<FuzFmt> {
    if data.len() < 16 || &data[0..4] != b"FUZE" {
        return None;
    }
    fn u32le(data: &[u8], o: usize) -> u32 {
        u32::from_le_bytes(data.get(o..o + 4).expect("in-range probe").try_into().unwrap())
    }
    fn u16le(b: &[u8]) -> u16 {
        u16::from_le_bytes(b.get(0..2).expect("in-range probe").try_into().unwrap())
    }
    let lip_size = u32le(data, 8);
    let riff_off = 12usize + lip_size as usize;
    let riff = data.get(riff_off..)?;
    if riff.len() < 12 || &riff[0..4] != b"RIFF" || &riff[8..12] != b"XWMA" {
        return None;
    }
    let mut pos = 12usize;
    let mut chunks = Vec::new();
    let (mut tag, mut channels, mut rate, mut bits, mut align, mut byterate, mut fmt_len) =
        (0u16, 0u16, 0u32, 0u16, 0u16, 0u32, 0usize);
    while pos + 8 <= riff.len() {
        let cid = &riff[pos..pos + 4];
        let sz = u32::from_le_bytes(riff[pos + 4..pos + 8].try_into().ok()?) as usize;
        let payload = riff.get(pos + 8..pos + 8 + sz)?;
        if cid == b"fmt " {
            fmt_len = sz;
            tag = u16le(payload.get(0..2).expect("fmt >= 2"));
            channels = u16le(payload.get(2..4).expect("fmt >= 4"));
            rate = u32le(payload, 4);
            byterate = u32le(payload, 8);
            align = u16le(payload.get(12..14).expect("fmt >= 14"));
            bits = u16le(payload.get(14..16).expect("fmt >= 16"));
        }
        chunks.push(String::from_utf8_lossy(cid).into_owned());
        pos += 8 + sz + (sz & 1);
    }
    Some(FuzFmt {
        lip_size,
        tag,
        channels,
        rate,
        bits,
        align,
        byterate,
        fmt_len,
        chunks: chunks.join(","),
    })
}
