//! `spt_tagmap` — corpus-wide tag dictionary enumerator.
//!
//! Walks every `.spt` in one or more BSAs as a TLV stream:
//!
//! ```text
//! magic (20 bytes) | (u32 tag, payload)*  | binary geometry tail
//! ```
//!
//! At each offset the walker tries to classify the 4-byte tag's
//! payload as either:
//!
//! - **string** — next 4 bytes are a u32 length in `[4, 256]` and the
//!   following `len` bytes are ≥ 75 % printable ASCII;
//! - **inline** — payload is the next 4 bytes (treated as both u32
//!   and f32, both forms recorded for sample roll-up).
//!
//! Once the walker enters a region where neither classification fits
//! cleanly (consecutive failures), it bails — that's the binary
//! geometry tail past the parameter section, deferred to a follow-up
//! sub-phase. The tail-region cutoff offset is recorded per file so
//! we can later enumerate the geometry tags too.
//!
//! Output: a Markdown report grouped by tag value, with occurrence
//! count, dominant payload kind, sample u32 / f32 values, and a
//! sample of any string payload observed. Drops straight into
//! `crates/spt/docs/format-notes.md` to grow the tag dictionary.
//!
//! ## Usage
//!
//! ```text
//! cargo run -p byroredux-spt --features recon --example spt_tagmap -- \
//!     "/path/to/Fallout - Meshes.bsa" \
//!     "/path/to/Oblivion - Meshes.bsa" \
//!     > /tmp/spt_tagmap.md
//! ```

use byroredux_bsa::BsaArchive;
use byroredux_spt::version::MAGIC_HEAD;
use std::collections::BTreeMap;

/// Per-tag aggregated stats.
#[derive(Default, Debug, Clone)]
struct TagStats {
    /// Total occurrences across the corpus.
    count: u32,
    /// Times the payload was classified as a length-prefixed string.
    as_string: u32,
    /// Times the payload was classified as inline 4-byte.
    as_inline: u32,
    /// Times the tag was classified as bare-marker (no payload bytes
    /// — followed directly by another tag).
    as_bare: u32,
    /// Up to 8 unique sample u32 values observed when the payload was
    /// inline (deduped, in arrival order).
    sample_u32: Vec<u32>,
    /// Up to 8 unique sample f32 values (printed alongside u32).
    sample_f32: Vec<f32>,
    /// Up to 4 sample strings observed (truncated to 80 chars each).
    sample_strings: Vec<String>,
}

impl TagStats {
    fn record_string(&mut self, s: &str) {
        self.count += 1;
        self.as_string += 1;
        if self.sample_strings.len() < 4 {
            let trimmed: String = s.chars().take(80).collect();
            if !self.sample_strings.contains(&trimmed) {
                self.sample_strings.push(trimmed);
            }
        }
    }

    fn record_inline(&mut self, raw: u32) {
        self.count += 1;
        self.as_inline += 1;
        if self.sample_u32.len() < 8 && !self.sample_u32.contains(&raw) {
            self.sample_u32.push(raw);
            self.sample_f32.push(f32::from_bits(raw));
        }
    }

    fn record_bare(&mut self) {
        self.count += 1;
        self.as_bare += 1;
    }
}

#[derive(Default, Debug, Clone)]
struct TailRegion {
    /// Sample of the byte offset where the TLV walker first failed
    /// to classify a tag — start of the binary geometry tail.
    sample_offsets: Vec<usize>,
    /// File count where we observed a tail region at all.
    files_with_tail: u32,
    /// File count where the walker reached EOF without bailing.
    files_clean: u32,
}

fn main() {
    let archives: Vec<String> = std::env::args().skip(1).collect();
    if archives.is_empty() {
        eprintln!(
            "usage: spt_tagmap <bsa-path> [<bsa-path>...]\n\
             walks every `.spt` as a TLV stream and emits a Markdown\n\
             tag dictionary covering the parameter section."
        );
        std::process::exit(2);
    }

    let mut tags: BTreeMap<u32, TagStats> = BTreeMap::new();
    let mut tail = TailRegion::default();
    let mut total_files = 0u32;
    let mut total_bytes = 0u64;

    for archive_path in &archives {
        let archive = match BsaArchive::open(archive_path) {
            Ok(a) => a,
            Err(e) => {
                eprintln!("[skip] {}: {}", archive_path, e);
                continue;
            }
        };
        let spt_files: Vec<String> = archive
            .list_files()
            .into_iter()
            .filter(|f| f.to_ascii_lowercase().ends_with(".spt"))
            .map(|f| f.to_string())
            .collect();
        for path in &spt_files {
            let bytes = match archive.extract(path) {
                Ok(b) => b,
                Err(e) => {
                    eprintln!("[err] {} :: {}: {}", archive_path, path, e);
                    continue;
                }
            };
            total_files += 1;
            total_bytes += bytes.len() as u64;
            walk_one_file(&bytes, &mut tags, &mut tail);
        }
    }

    // ── Render Markdown report ────────────────────────────────────
    println!("# `.spt` parameter-section tag dictionary\n");
    println!("Generated by `spt_tagmap` over {} files ({} bytes total).\n", total_files, total_bytes);
    println!(
        "- {} files reached EOF cleanly through the TLV walker",
        tail.files_clean
    );
    println!(
        "- {} files transitioned to a binary geometry tail (sample offsets: {:?})\n",
        tail.files_with_tail,
        &tail.sample_offsets[..tail.sample_offsets.len().min(8)],
    );

    println!("| tag (dec) | tag (hex) | count | bare | string | inline | sample u32 | sample f32 | sample strings |");
    println!("|---:|---:|---:|---:|---:|---:|---|---|---|");
    for (tag, stats) in &tags {
        let u32_samples: String = stats
            .sample_u32
            .iter()
            .take(4)
            .map(|v| format!("{}", v))
            .collect::<Vec<_>>()
            .join(", ");
        let f32_samples: String = stats
            .sample_f32
            .iter()
            .take(4)
            .map(|v| format!("{:.4}", v))
            .collect::<Vec<_>>()
            .join(", ");
        let str_samples: String = stats
            .sample_strings
            .iter()
            .take(2)
            .map(|s| format!("`{}`", s.replace('`', "\\`")))
            .collect::<Vec<_>>()
            .join("<br>");
        println!(
            "| {} | 0x{:04x} | {} | {} | {} | {} | {} | {} | {} |",
            tag,
            tag,
            stats.count,
            stats.as_bare,
            stats.as_string,
            stats.as_inline,
            u32_samples,
            f32_samples,
            str_samples,
        );
    }
}

/// Walk a single `.spt` file as a TLV stream until the walker can
/// no longer classify the current 4 bytes as a tag → payload pair.
/// The bail-out offset gets recorded as the start of the binary
/// geometry tail.
fn walk_one_file(bytes: &[u8], tags: &mut BTreeMap<u32, TagStats>, tail: &mut TailRegion) {
    if !bytes.starts_with(MAGIC_HEAD) {
        return;
    }
    let mut i: usize = 20;
    // Parameter-section tag value range — observed via `spt_dissect`:
    // tags cluster between ~1 002 and ~13 000 (sample: 1002, 1016, 2000s,
    // 6000, 7000, ~13 000). Geometry-tail "tags" jump to 19 985+ or
    // are uniformly zero (binary data). A tag outside [TAG_MIN, TAG_MAX]
    // is the cleanest signal we've crossed into the binary tail.
    const TAG_MIN: u32 = 100;
    const TAG_MAX: u32 = 13_999;
    let in_range = |v: u32| (TAG_MIN..=TAG_MAX).contains(&v);

    while i + 4 <= bytes.len() {
        let tag = u32::from_le_bytes([bytes[i], bytes[i + 1], bytes[i + 2], bytes[i + 3]]);
        if !in_range(tag) {
            break;
        }
        let stats = tags.entry(tag).or_default();

        // Classification dispatch — try in order:
        //   1. STRING — next u32 is a plausible length and the
        //      following bytes are ≥ 75 % printable ASCII.
        //   2. BARE   — next 4 bytes form a plausible tag value
        //      (in [TAG_MIN, TAG_MAX]); current tag has 0 payload.
        //      Tag 1002 is the canonical example: dissect shows it
        //      sits immediately before tag 2000 with no intervening
        //      payload.
        //   3. INLINE — fall-back: consume 4 more bytes as either
        //      f32 or u32 raw payload.
        // The bare-vs-inline ambiguity is real: a tag that stores a
        // u32 happens to have a u32 payload that *might* land in the
        // tag range. We resolve it by giving bare priority — the
        // dissect-confirmed cases all have bare tags.

        // 1. STRING classification.
        if i + 8 <= bytes.len() {
            let len_u32 = u32::from_le_bytes([
                bytes[i + 4],
                bytes[i + 5],
                bytes[i + 6],
                bytes[i + 7],
            ]);
            if (1..=256).contains(&len_u32) {
                let len = len_u32 as usize;
                if i + 8 + len <= bytes.len() {
                    let payload = &bytes[i + 8..i + 8 + len];
                    let printable = payload.iter().filter(|&&b| (32..=126).contains(&b)).count();
                    if printable as f32 / len.max(1) as f32 >= 0.75 {
                        if let Ok(s) = std::str::from_utf8(payload) {
                            stats.record_string(s);
                            i += 8 + len;
                            continue;
                        }
                    }
                }
            }
        }

        // 2. BARE-MARKER classification — the next 4 bytes look like
        // another tag.
        if i + 8 <= bytes.len() {
            let next = u32::from_le_bytes([
                bytes[i + 4],
                bytes[i + 5],
                bytes[i + 6],
                bytes[i + 7],
            ]);
            if in_range(next) {
                stats.record_bare();
                i += 4;
                continue;
            }
        }

        // 3. INLINE 4-byte payload — fall-back.
        if i + 8 > bytes.len() {
            break;
        }
        let raw = u32::from_le_bytes([
            bytes[i + 4],
            bytes[i + 5],
            bytes[i + 6],
            bytes[i + 7],
        ]);
        stats.record_inline(raw);
        i += 8;
    }
    if i >= bytes.len() {
        tail.files_clean += 1;
    } else {
        tail.files_with_tail += 1;
        if tail.sample_offsets.len() < 16 {
            tail.sample_offsets.push(i);
        }
    }
}
