//! `spt_dissect` — single-file structural recon for a SpeedTree
//! `.spt` binary. Companion to `spt_recon` (which is corpus-wide).
//!
//! Use this to drill into one file once `spt_recon` has identified
//! it as representative of a bucket. Outputs:
//!
//! 1. **Header confirmation** — pins the 20-byte magic.
//! 2. **Post-magic hex dump** — bytes 20..min(len, 256) in 16-byte
//!    rows so wire-layout patterns become eyeball-visible.
//! 3. **Tail hex dump** — last 64 bytes (potential footer / section
//!    terminator).
//! 4. **Printable ASCII runs ≥ 4 chars with offsets.** Lets us locate
//!    every Family-A authoring path, Family-B `BezierSpline` label,
//!    Family-C control-point quintet (see `format-notes.md`) by file
//!    offset rather than just by content.
//! 5. **Length-prefix string candidates.** For every offset, treat
//!    `u32 LE` as a candidate string length and check if the
//!    following bytes are predominantly printable. Recovers every
//!    SpeedTree-style length-prefixed string (the `__IdvSpt_02_`
//!    magic itself is one of these — the byte-2 u32 = 12 + 12-byte
//!    payload).
//!
//! ## Usage
//!
//! ```text
//! # From a BSA (typical case):
//! cargo run -p byroredux-spt --features recon --example spt_dissect -- \
//!     "/path/to/Fallout - Meshes.bsa" "trees/euonymusbush01.spt"
//!
//! # From a loose file:
//! cargo run -p byroredux-spt --features recon --example spt_dissect -- \
//!     /path/to/dumped.spt
//! ```

use byroredux_bsa::BsaArchive;
use byroredux_spt::version::{detect_variant, MAGIC_HEAD};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let bytes = match args.len() {
        1 => std::fs::read(&args[0]).expect("read loose .spt file"),
        2 => {
            let archive = BsaArchive::open(&args[0]).expect("open BSA");
            archive.extract(&args[1]).expect("extract from BSA")
        }
        _ => {
            eprintln!(
                "usage: spt_dissect <loose-path>\n   or: spt_dissect <bsa-path> <path-in-bsa>"
            );
            std::process::exit(2);
        }
    };
    let label = if args.len() == 2 {
        format!("{} :: {}", args[0], args[1])
    } else {
        args[0].clone()
    };

    println!("# `{}`", label);
    println!("- size: {} bytes", bytes.len());

    // 1) Header confirmation.
    let variant = detect_variant(&bytes);
    let recognized = bytes.starts_with(MAGIC_HEAD);
    println!(
        "- magic: {} ({})",
        if recognized { "OK" } else { "**MISSING**" },
        variant.tag(),
    );

    // 2) Post-magic hex dump.
    println!("\n## Post-magic hex (offset 20..)\n");
    println!("```");
    let dump_end = bytes.len().min(20 + 256);
    hex_dump_with_ascii(&bytes[20..dump_end], 20);
    println!("```");

    // 3) Tail hex dump.
    if bytes.len() > 20 + 256 {
        println!("\n## Tail hex (last 64 bytes)\n");
        println!("```");
        let tail_start = bytes.len().saturating_sub(64);
        hex_dump_with_ascii(&bytes[tail_start..], tail_start);
        println!("```");
    }

    // 4) Printable runs with offsets.
    println!("\n## Printable ASCII runs (≥ 4 chars), with byte offset\n");
    let runs = printable_runs_with_offsets(&bytes, 4);
    println!("- {} run(s) total", runs.len());
    for (offset, s) in &runs {
        println!("  - `{:6}`  `{}`", offset, escape_for_md(s));
    }

    // 5) Length-prefix string candidates.
    println!("\n## Length-prefix string candidates\n");
    println!(
        "Heuristic: at every offset N, if `u32 LE` at N is in [1, 256] AND \
         the following bytes are ≥ 75 % printable ASCII, treat N as a \
         length-prefixed string header. Reports the length, the string, \
         and the next-section offset.\n"
    );
    println!("```");
    for (offset, len, s) in length_prefixed_string_candidates(&bytes) {
        println!(
            "{:6}  len={:>3}  next={:6}  {:?}",
            offset,
            len,
            offset + 4 + len,
            s,
        );
    }
    println!("```");
}

/// Print 16-byte rows of hex + ASCII, with an offset prefix per row.
fn hex_dump_with_ascii(bytes: &[u8], base_offset: usize) {
    for (i, chunk) in bytes.chunks(16).enumerate() {
        let off = base_offset + i * 16;
        let hex: String = chunk
            .iter()
            .map(|b| format!("{:02x} ", b))
            .collect::<String>();
        let ascii: String = chunk
            .iter()
            .map(|&b| {
                if (32..=126).contains(&b) {
                    b as char
                } else {
                    '.'
                }
            })
            .collect();
        println!("{:6}: {:<48}  {}", off, hex, ascii);
    }
}

fn printable_runs_with_offsets(bytes: &[u8], min_len: usize) -> Vec<(usize, String)> {
    let mut out: Vec<(usize, String)> = Vec::new();
    let mut start: Option<usize> = None;
    for (i, &b) in bytes.iter().enumerate() {
        let printable = (32..=126).contains(&b);
        if printable {
            if start.is_none() {
                start = Some(i);
            }
        } else if let Some(s) = start.take() {
            if i - s >= min_len {
                if let Ok(text) = std::str::from_utf8(&bytes[s..i]) {
                    out.push((s, text.to_string()));
                }
            }
        }
    }
    if let Some(s) = start {
        if bytes.len() - s >= min_len {
            if let Ok(text) = std::str::from_utf8(&bytes[s..]) {
                out.push((s, text.to_string()));
            }
        }
    }
    out
}

fn length_prefixed_string_candidates(bytes: &[u8]) -> Vec<(usize, usize, String)> {
    let mut out: Vec<(usize, usize, String)> = Vec::new();
    if bytes.len() < 8 {
        return out;
    }
    let mut i = 0;
    while i + 4 <= bytes.len() {
        let len_u32 = u32::from_le_bytes([bytes[i], bytes[i + 1], bytes[i + 2], bytes[i + 3]]);
        if (1..=256).contains(&len_u32) {
            let len = len_u32 as usize;
            if i + 4 + len <= bytes.len() {
                let payload = &bytes[i + 4..i + 4 + len];
                let printable_count = payload.iter().filter(|&&b| (32..=126).contains(&b)).count();
                let frac = printable_count as f32 / len as f32;
                if frac >= 0.75 {
                    if let Ok(s) = std::str::from_utf8(payload) {
                        out.push((i, len, s.to_string()));
                        // Advance past the candidate to avoid emitting
                        // overlapping reports for the same string region.
                        i += 4 + len;
                        continue;
                    }
                }
            }
        }
        i += 1;
    }
    out
}

fn escape_for_md(s: &str) -> String {
    s.replace('`', "\\`")
}
