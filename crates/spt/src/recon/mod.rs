//! Reverse-engineering harness for SpeedTree `.spt` binaries.
//!
//! Feature-gated under `recon` because it isn't part of the runtime
//! parser path — these are tools we run once per session against a
//! mounted vanilla BSA to learn the on-wire format. Findings get
//! written to `crates/spt/docs/format-notes.md` as observation-only
//! prose. No SDK code, no SDK paraphrasing — black-box only.
//!
//! ## What we capture per file
//!
//! - File path inside the BSA.
//! - File size in bytes.
//! - First 32 bytes (the likely-header window).
//! - Last 16 bytes (likely-footer / trailing-section terminator).
//! - Printable ASCII runs of length ≥ 4 (file paths, version strings,
//!   shader / texture names embedded by the SpeedTree exporter).
//! - Byte-frequency histogram bucketed into 16-bin summaries (helps
//!   spot uniformly-zero padding regions vs encoded float/index runs).
//!
//! ## Aggregate roll-up
//!
//! Files get bucketed by their first 16 bytes (assumed to be a stable
//! magic + version prefix). The aggregate prints one row per bucket
//! with the file count + median / min / max sizes. That should reveal
//! both the distinct SpeedTree wire variants and their relative
//! prevalence in each game's corpus.

use std::collections::BTreeMap;

/// One file's observation record. Cheap to construct (clones a few
/// hundred bytes per file) — designed to be collected across an entire
/// archive in memory before the aggregate roll-up.
#[derive(Debug, Clone)]
pub struct FileStats {
    /// Path of the `.spt` inside its BSA. Lower-cased for stable
    /// sort / dedupe.
    pub path: String,
    /// Total file length in bytes.
    pub size: usize,
    /// First 32 bytes of the file (likely-header window). Padded
    /// with zeros if the file is shorter than 32 bytes.
    pub head: [u8; 32],
    /// Last 16 bytes of the file (likely-footer window).
    pub tail: [u8; 16],
    /// Printable ASCII runs ≥ 4 chars found anywhere in the file.
    /// Capped at 32 entries per file to bound memory.
    pub strings: Vec<String>,
    /// Byte-frequency histogram bucketed into 16 bins of 16 byte
    /// values each. Bin `i` counts bytes in [i*16, i*16+15].
    pub histogram_buckets: [u32; 16],
}

impl FileStats {
    /// Compute the per-file stats from a raw `.spt` byte slice and
    /// the path it was loaded from.
    pub fn capture(path: impl Into<String>, bytes: &[u8]) -> Self {
        let mut head = [0u8; 32];
        for (i, b) in bytes.iter().take(32).enumerate() {
            head[i] = *b;
        }
        let mut tail = [0u8; 16];
        let tail_start = bytes.len().saturating_sub(16);
        for (i, b) in bytes[tail_start..].iter().enumerate() {
            tail[i] = *b;
        }
        Self {
            path: path.into().to_ascii_lowercase(),
            size: bytes.len(),
            head,
            tail,
            strings: extract_printable_runs(bytes, 4, 32),
            histogram_buckets: histogram_buckets(bytes),
        }
    }

    /// First 16 bytes of the head — the bucketing key in
    /// [`Aggregate::insert`]. Magic + version live here in nearly
    /// every chunked binary format we've seen.
    pub fn magic16(&self) -> [u8; 16] {
        let mut out = [0u8; 16];
        out.copy_from_slice(&self.head[..16]);
        out
    }
}

/// Extract every contiguous run of printable ASCII (32..=126) of length
/// `>= min_len` from `bytes`. Caps the result at `max_count` runs to
/// keep the report readable on very large files.
fn extract_printable_runs(bytes: &[u8], min_len: usize, max_count: usize) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut current: Vec<u8> = Vec::new();
    for &b in bytes {
        if (32..=126).contains(&b) {
            current.push(b);
        } else {
            if current.len() >= min_len {
                if let Ok(s) = std::str::from_utf8(&current) {
                    out.push(s.to_string());
                    if out.len() >= max_count {
                        return out;
                    }
                }
            }
            current.clear();
        }
    }
    if current.len() >= min_len {
        if let Ok(s) = std::str::from_utf8(&current) {
            out.push(s.to_string());
        }
    }
    out
}

fn histogram_buckets(bytes: &[u8]) -> [u32; 16] {
    let mut h = [0u32; 16];
    for &b in bytes {
        h[(b as usize) / 16] += 1;
    }
    h
}

/// Aggregate roll-up across an entire archive — files grouped by their
/// leading 16-byte signature, sizes summarized per bucket.
#[derive(Debug, Default)]
pub struct Aggregate {
    /// Buckets keyed by (first 16 bytes of head). BTreeMap so the
    /// output ordering is stable across runs.
    pub buckets: BTreeMap<[u8; 16], BucketStats>,
    /// Total file count seen so far.
    pub total_files: u32,
    /// Total bytes seen across all files.
    pub total_bytes: u64,
}

#[derive(Debug, Default, Clone)]
pub struct BucketStats {
    /// File count in this bucket.
    pub count: u32,
    /// Min / median (approximation — we sort once at print time)
    /// / max sizes in bytes.
    pub sizes: Vec<usize>,
    /// First file path observed in this bucket (sample for the
    /// format-notes write-up).
    pub sample_path: String,
    /// Set of unique printable ASCII runs ≥ 8 chars across this
    /// bucket. Capped at 32 unique strings (across all files in the
    /// bucket) so the report stays readable.
    pub strings: Vec<String>,
}

impl Aggregate {
    /// Fold a single file's stats into the aggregate.
    pub fn insert(&mut self, stats: FileStats) {
        self.total_files += 1;
        self.total_bytes += stats.size as u64;

        let key = stats.magic16();
        let bucket = self.buckets.entry(key).or_default();
        bucket.count += 1;
        bucket.sizes.push(stats.size);
        if bucket.sample_path.is_empty() {
            bucket.sample_path = stats.path;
        }
        for s in stats.strings {
            if s.len() >= 8 && bucket.strings.len() < 32 && !bucket.strings.contains(&s) {
                bucket.strings.push(s);
            }
        }
    }

    /// Format a Markdown table summarizing every bucket. One header
    /// row + one row per bucket. Output is plain markdown so the
    /// recon binary can pipe it straight into `format-notes.md`.
    pub fn render_markdown_table(&self) -> String {
        let mut out = String::new();
        out.push_str("| count | min | median | max | magic (hex) | sample path |\n");
        out.push_str("|------:|----:|-------:|----:|-------------|-------------|\n");
        for (magic, bucket) in &self.buckets {
            let mut sizes = bucket.sizes.clone();
            sizes.sort_unstable();
            let min = sizes.first().copied().unwrap_or(0);
            let max = sizes.last().copied().unwrap_or(0);
            let median = sizes.get(sizes.len() / 2).copied().unwrap_or(0);
            let magic_hex: String = magic.iter().map(|b| format!("{:02x}", b)).collect();
            out.push_str(&format!(
                "| {} | {} | {} | {} | `{}` | `{}` |\n",
                bucket.count, min, median, max, magic_hex, bucket.sample_path,
            ));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capture_preserves_head_tail_size() {
        let bytes: Vec<u8> = (0..200u8).collect();
        let stats = FileStats::capture("trees/foo.spt", &bytes);
        assert_eq!(stats.size, 200);
        assert_eq!(stats.path, "trees/foo.spt");
        // Head: bytes 0..32 verbatim.
        for i in 0..32 {
            assert_eq!(stats.head[i], i as u8);
        }
        // Tail: bytes 184..200 verbatim.
        for i in 0..16 {
            assert_eq!(stats.tail[i], (184 + i) as u8);
        }
    }

    #[test]
    fn capture_pads_short_files_safely() {
        let bytes = b"abc";
        let stats = FileStats::capture("tiny.spt", bytes);
        assert_eq!(stats.size, 3);
        assert_eq!(&stats.head[..3], b"abc");
        // Bytes past the file length stay zero — no panic on the bounds.
        for i in 3..32 {
            assert_eq!(stats.head[i], 0);
        }
        // Tail covers the whole file when len < 16.
        assert_eq!(&stats.tail[..3], b"abc");
    }

    #[test]
    fn extract_printable_runs_finds_embedded_strings() {
        let mut bytes = vec![0u8; 4];
        bytes.extend_from_slice(b"trees/oak.dds");
        bytes.extend_from_slice(&[0u8; 8]);
        bytes.extend_from_slice(b"BR");
        bytes.extend_from_slice(&[0u8; 4]);
        bytes.extend_from_slice(b"Branch_Lod0");
        let runs = extract_printable_runs(&bytes, 4, 32);
        assert!(runs.iter().any(|s| s == "trees/oak.dds"));
        assert!(runs.iter().any(|s| s == "Branch_Lod0"));
        // Two-byte run "BR" is below the min_len threshold.
        assert!(!runs.iter().any(|s| s == "BR"));
    }

    #[test]
    fn aggregate_buckets_by_magic16() {
        let mut agg = Aggregate::default();
        let mut a = vec![0u8; 64];
        a[..4].copy_from_slice(b"SPTN"); // synthetic magic A
        let mut b = vec![0u8; 128];
        b[..4].copy_from_slice(b"SPT5"); // synthetic magic B
        agg.insert(FileStats::capture("trees/a.spt", &a));
        agg.insert(FileStats::capture("trees/b.spt", &b));
        agg.insert(FileStats::capture("trees/a2.spt", &a));
        assert_eq!(agg.total_files, 3);
        assert_eq!(agg.buckets.len(), 2, "two distinct magic16 prefixes");
        let table = agg.render_markdown_table();
        assert!(table.contains("count"));
        assert!(table.contains("`trees/a.spt`"));
        assert!(table.contains("`trees/b.spt`"));
    }

    #[test]
    fn histogram_buckets_sums_to_byte_length() {
        let bytes: Vec<u8> = (0..=255u8).collect();
        let h = histogram_buckets(&bytes);
        let total: u32 = h.iter().sum();
        assert_eq!(total as usize, bytes.len());
        // Uniform input (one of each byte value) yields 16 per bucket.
        for &c in &h {
            assert_eq!(c, 16);
        }
    }
}
