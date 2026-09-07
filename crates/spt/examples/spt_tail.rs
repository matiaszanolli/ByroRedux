//! `spt_tail` — geometry-tail structure analyzer (EX-14/15 Phase 2.1).
//!
//! Companion to `spt_transitions`, which deliberately caps its scan at
//! `TAG_MAX = 13_999` and so can say nothing about the region past
//! `SptScene::tail_offset`. This tool starts *at* `tail_offset` — obtained
//! from the real parser, not re-derived — and asks the one question
//! `exal-trees.md` §10 lists as blocking everything else:
//!
//! > Are `19985`/`19989` real geometry-section tags, or float-data
//! > coincidences?
//!
//! ## Method
//!
//! The premise that makes this decidable without any SDK reference is a
//! background-rate argument. If the tail were pure float payload, a u32
//! read at an arbitrary offset would have to land in a narrow high band by
//! chance. That band is ~51.5 k values wide out of 2^32, so the expected
//! number of hits across the whole corpus is small and computable — and
//! reported below next to the observed count. A value that shows up in
//! most *files* rather than in proportion to total *bytes* is structure,
//! not coincidence.
//!
//! Deliberately **not** hypothesis-first: every high-band value is ranked
//! by the number of distinct files it appears in, so `19985`/`19989` have
//! to earn their place against every other candidate rather than being
//! the only two examined. If they are ordinary noise, they will sit in the
//! long tail next to hundreds of one-off values, and that is the finding.
//!
//! ## Usage
//!
//! ```text
//! cargo run -p byroredux-spt --features recon --example spt_tail -- \
//!     "/path/to/Fallout - Meshes.bsa" \
//!     "/path/to/Oblivion - Meshes.bsa" \
//!     > /tmp/spt_tail.md
//! ```

use byroredux_bsa::BsaArchive;
use byroredux_spt::parse_spt;
use byroredux_spt::parser::{TAG_MAX, TAG_MIN};
use byroredux_spt::tag::{dispatch_tag, SptTagKind};
use std::collections::{BTreeMap, BTreeSet, HashMap};

/// Values below this are the parameter section's own band, already
/// dictionaried by `spt_transitions`. Starting above it keeps the two
/// tools' findings disjoint.
const HIGH_TAG_MIN: u32 = 14_000;
/// Upper bound of the band. Chosen so the band stays narrow enough for
/// the background-rate argument in the module doc to have force; every
/// candidate marker named by `format-notes.md` sits well inside it.
const HIGH_TAG_MAX: u32 = 65_535;

/// How far ahead to look for a successor when measuring the transition
/// distance that separates a real TLV tag from a chance hit.
const MAX_SUCCESSOR_DISTANCE: usize = 4_096;

#[derive(Default, Clone)]
struct ValueStats {
    /// Total occurrences across the whole corpus.
    total: u32,
    /// Distinct files the value appears in at least once.
    files: BTreeSet<String>,
    /// `occurrences_in_one_file → number_of_files`.
    per_file_histogram: BTreeMap<u32, u32>,
    /// `(offset - tail_offset) % 4 → count`. A real TLV stream should be
    /// consistently aligned; chance hits straddle float boundaries and
    /// smear across all four residues.
    alignment: BTreeMap<usize, u32>,
    /// Byte distance to the next high-band value in the same tail.
    successor_distance: BTreeMap<usize, u32>,
    /// Distinct successor values, capped.
    successors: BTreeSet<u32>,
}

impl ValueStats {
    fn modal<T: Copy + Ord>(hist: &BTreeMap<T, u32>) -> Option<(T, u32)> {
        hist.iter().max_by_key(|(_, n)| **n).map(|(k, n)| (*k, *n))
    }

    /// Share of observations sitting on the single most common alignment
    /// residue. 100 % = perfectly aligned stream.
    fn alignment_purity(&self) -> f32 {
        Self::modal(&self.alignment).map_or(0.0, |(_, n)| n as f32 / self.total.max(1) as f32)
    }
}

fn main() {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    // `--dump <archive> <inner-path>` annotates one file's tail instead of
    // running the corpus sweep. Kept in the same binary so the dump and the
    // statistics can never disagree about where `tail_offset` is.
    if args.first().map(|a| a == "--dump").unwrap_or(false) && args.len() >= 3 {
        dump_one(&args[1], &args[2]);
        return;
    }
    let archives: Vec<String> = std::mem::take(&mut args);
    if archives.is_empty() {
        eprintln!(
            "usage: spt_tail <bsa-path> [<bsa-path>...]\n\
             ranks every high-band u32 in the region past tail_offset by\n\
             the number of files it appears in, to separate real geometry\n\
             section tags from float-data coincidences."
        );
        std::process::exit(2);
    }

    let mut stats: HashMap<u32, ValueStats> = HashMap::new();
    // Files whose tail contains at least one 4-byte-aligned value that the
    // EXISTING parameter dictionary already knows. If the tail were a
    // geometry section this should be ~0; if the tail is just the parameter
    // stream past the walker's TAG_MAX cutoff it should be ~100%.
    let mut files_with_known_tag_in_tail = 0u32;
    let mut known_tag_hits: BTreeMap<u32, u32> = BTreeMap::new();
    // `best shift → files`. A non-zero mode means `tail_offset` routinely
    // lands mid-payload rather than on a section boundary.
    let mut desync_shift: BTreeMap<usize, u32> = BTreeMap::new();
    let mut file_sizes: Vec<usize> = Vec::new();
    let mut files_seen = 0u32;
    let mut files_parsed = 0u32;
    let mut files_with_tail = 0u32;
    let mut total_tail_bytes = 0u64;
    let mut tail_len_histogram: BTreeMap<u32, u32> = BTreeMap::new();
    // Per-file: how many distinct high-band values its tail contains.
    let mut distinct_per_file: BTreeMap<u32, u32> = BTreeMap::new();

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
            files_seen += 1;
            let scene = match parse_spt(&bytes) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("[parse-fail] {}: {}", path, e);
                    continue;
                }
            };
            files_parsed += 1;

            let tail_offset = scene.tail_offset;
            if tail_offset >= bytes.len() {
                continue;
            }
            files_with_tail += 1;
            file_sizes.push(bytes.len());
            // Re-scan with the existing dictionary at each of the four
            // possible byte shifts. `tail_offset` is where the parameter
            // walker lost sync, which need not be a 4-byte boundary of the
            // real stream — so the shift that maximises known-tag hits is
            // itself the measurement of how far the walker desynced.
            let mut best_shift = 0usize;
            let mut best_hits = 0u32;
            for shift in 0..4usize {
                let mut hits = 0u32;
                let mut j = tail_offset + shift;
                while j + 4 <= bytes.len() {
                    let v =
                        u32::from_le_bytes([bytes[j], bytes[j + 1], bytes[j + 2], bytes[j + 3]]);
                    if (TAG_MIN..=TAG_MAX).contains(&v)
                        && !matches!(dispatch_tag(v), SptTagKind::Unknown)
                    {
                        hits += 1;
                    }
                    j += 4;
                }
                if hits > best_hits {
                    best_hits = hits;
                    best_shift = shift;
                }
            }
            *desync_shift.entry(best_shift).or_insert(0) += 1;
            if best_hits > 0 {
                files_with_known_tag_in_tail += 1;
                let mut j = tail_offset + best_shift;
                while j + 4 <= bytes.len() {
                    let v =
                        u32::from_le_bytes([bytes[j], bytes[j + 1], bytes[j + 2], bytes[j + 3]]);
                    if (TAG_MIN..=TAG_MAX).contains(&v)
                        && !matches!(dispatch_tag(v), SptTagKind::Unknown)
                    {
                        *known_tag_hits.entry(v).or_insert(0) += 1;
                    }
                    j += 4;
                }
            }
            let tail_len = bytes.len() - tail_offset;
            total_tail_bytes += tail_len as u64;
            // Bucket to the nearest 512 B for a readable distribution.
            *tail_len_histogram
                .entry((tail_len as u32 / 512) * 512)
                .or_insert(0) += 1;

            let key = format!("{}::{}", archive_path, path);
            let hits = scan_tail(&bytes, tail_offset);
            *distinct_per_file
                .entry(hits.iter().map(|(_, v)| *v).collect::<BTreeSet<_>>().len() as u32)
                .or_insert(0) += 1;

            let mut per_file_counts: HashMap<u32, u32> = HashMap::new();
            for (i, (offset, value)) in hits.iter().enumerate() {
                let entry = stats.entry(*value).or_default();
                entry.total += 1;
                entry.files.insert(key.clone());
                *entry
                    .alignment
                    .entry((offset - tail_offset) % 4)
                    .or_insert(0) += 1;
                if let Some((next_offset, next_value)) = hits.get(i + 1) {
                    let distance = next_offset - offset;
                    if distance <= MAX_SUCCESSOR_DISTANCE {
                        *entry.successor_distance.entry(distance).or_insert(0) += 1;
                        if entry.successors.len() < 12 {
                            entry.successors.insert(*next_value);
                        }
                    }
                }
                *per_file_counts.entry(*value).or_insert(0) += 1;
            }
            for (value, count) in per_file_counts {
                *stats
                    .entry(value)
                    .or_default()
                    .per_file_histogram
                    .entry(count)
                    .or_insert(0) += 1;
            }
        }
    }

    report(
        &stats,
        files_seen,
        files_parsed,
        files_with_tail,
        total_tail_bytes,
        &tail_len_histogram,
        &distinct_per_file,
    );
    report_resync(
        files_with_tail,
        files_with_known_tag_in_tail,
        &known_tag_hits,
        &file_sizes,
        &desync_shift,
    );
}

/// Every high-band u32 in `bytes[tail_offset..]`, in offset order.
///
/// Scans one byte at a time rather than in 4-byte strides: a stream whose
/// alignment is not yet known must not have an alignment assumed for it,
/// and the alignment histogram this feeds is only meaningful if every
/// residue had an equal chance of being seen.
fn scan_tail(bytes: &[u8], tail_offset: usize) -> Vec<(usize, u32)> {
    let mut hits = Vec::new();
    let mut i = tail_offset;
    while i + 4 <= bytes.len() {
        let v = u32::from_le_bytes([bytes[i], bytes[i + 1], bytes[i + 2], bytes[i + 3]]);
        if (HIGH_TAG_MIN..=HIGH_TAG_MAX).contains(&v) {
            hits.push((i, v));
        }
        i += 1;
    }
    hits
}

#[allow(clippy::too_many_arguments)]
fn report(
    stats: &HashMap<u32, ValueStats>,
    files_seen: u32,
    files_parsed: u32,
    files_with_tail: u32,
    total_tail_bytes: u64,
    tail_len_histogram: &BTreeMap<u32, u32>,
    distinct_per_file: &BTreeMap<u32, u32>,
) {
    println!("# `.spt` geometry-tail structure scan\n");
    println!(
        "Generated by `spt_tail`. Band `[{}, {}]`, scanned one byte at a \
         time from each file's parser-reported `tail_offset`.\n",
        HIGH_TAG_MIN, HIGH_TAG_MAX
    );

    println!("## Corpus reach\n");
    println!("| metric | value |\n|---|---:|");
    println!("| `.spt` files seen | {} |", files_seen);
    println!("| parsed by `parse_spt` | {} |", files_parsed);
    println!("| with a non-empty tail | {} |", files_with_tail);
    println!("| total tail bytes | {} |", total_tail_bytes);

    // The background-rate argument, computed rather than asserted.
    let band_width = (HIGH_TAG_MAX - HIGH_TAG_MIN + 1) as f64;
    let positions = total_tail_bytes as f64;
    let expected_per_value = positions / 2f64.powi(32);
    let expected_band_total = positions * band_width / 2f64.powi(32);
    println!("| scan positions (≈ tail bytes) | {:.0} |", positions);
    println!(
        "| **expected hits per single value, if the tail were uniform noise** | {:.4} |",
        expected_per_value
    );
    println!(
        "| expected hits across the whole band, if uniform noise | {:.1} |",
        expected_band_total
    );
    println!(
        "\nA value observed far above `{:.4}` occurrences — and especially \
         one present in a large *fraction of files* rather than in \
         proportion to total bytes — is structure. This is the discriminator \
         `exal-trees.md` §3.2 asks for, stated quantitatively.\n",
        expected_per_value
    );

    println!("### Tail length distribution (512 B buckets)\n");
    println!("| tail bytes ≥ | files |\n|---:|---:|");
    for (bucket, n) in tail_len_histogram {
        println!("| {} | {} |", bucket, n);
    }

    println!("\n### Distinct high-band values per file\n");
    println!("| distinct values in tail | files |\n|---:|---:|");
    for (distinct, n) in distinct_per_file {
        println!("| {} | {} |", distinct, n);
    }

    let mut ranked: Vec<(&u32, &ValueStats)> = stats.iter().collect();
    ranked.sort_by(|a, b| {
        b.1.files
            .len()
            .cmp(&a.1.files.len())
            .then(b.1.total.cmp(&a.1.total))
            .then(a.0.cmp(b.0))
    });

    println!(
        "\n## Ranked high-band values (all, by file coverage)\n\n\
         `files` is the discriminating column: a section tag appears in \
         most files, a chance hit in one or two.\n"
    );
    println!(
        "| value | hex | files | % of tails | total | per-file mode | \
         align purity | modal successor distance | sample successors |"
    );
    println!("|---:|---:|---:|---:|---:|---:|---:|---:|---|");
    for (value, s) in ranked.iter() {
        print_row(**value, s, files_with_tail);
    }

    println!("\n## The two candidates named by `format-notes.md`\n");
    println!(
        "| value | hex | files | % of tails | total | per-file mode | \
         align purity | modal successor distance | sample successors |"
    );
    println!("|---:|---:|---:|---:|---:|---:|---:|---:|---|");
    for candidate in [19_985u32, 19_989] {
        match stats.get(&candidate) {
            Some(s) => print_row(candidate, s, files_with_tail),
            None => println!(
                "| {} | 0x{:04x} | 0 | 0.0% | 0 | — | — | — | *absent from every tail* |",
                candidate, candidate
            ),
        }
    }

    println!(
        "\n### Rank of each candidate within the band\n\n\
         | value | rank by file coverage | of {} distinct values |\n|---:|---:|---:|",
        ranked.len()
    );
    for candidate in [19_985u32, 19_989] {
        let rank = ranked
            .iter()
            .position(|(v, _)| **v == candidate)
            .map(|p| (p + 1).to_string())
            .unwrap_or_else(|| "—".to_string());
        println!("| {} | {} | {} |", candidate, rank, ranked.len());
    }
}

fn print_row(value: u32, s: &ValueStats, files_with_tail: u32) {
    let pct = s.files.len() as f32 / files_with_tail.max(1) as f32 * 100.0;
    let per_file_mode = ValueStats::modal(&s.per_file_histogram)
        .map(|(k, _)| k.to_string())
        .unwrap_or_else(|| "—".into());
    let succ_dist = ValueStats::modal(&s.successor_distance)
        .map(|(k, n)| format!("{} B ×{}", k, n))
        .unwrap_or_else(|| "—".into());
    let succ: String = s
        .successors
        .iter()
        .take(8)
        .map(|v| v.to_string())
        .collect::<Vec<_>>()
        .join(", ");
    println!(
        "| {} | 0x{:04x} | {} | {:.1}% | {} | {} | {:.0}% | {} | {} |",
        value,
        value,
        s.files.len(),
        pct,
        s.total,
        per_file_mode,
        s.alignment_purity() * 100.0,
        succ_dist,
        succ,
    );
}

/// Annotate one file's tail: every 4-byte word from `tail_offset`, shown as
/// hex, as a u32, and as an f32, so the TLV-vs-float question can be read
/// off directly rather than argued about.
fn dump_one(archive_path: &str, inner: &str) {
    let archive = BsaArchive::open(archive_path).expect("open archive");
    let bytes = archive.extract(inner).expect("extract");
    let scene = parse_spt(&bytes).expect("parse");
    let tail_offset = scene.tail_offset;
    println!("# tail dump — {}\n", inner);
    println!(
        "file bytes = {}, tail_offset = {}, tail bytes = {}, reached_eof = {}\n",
        bytes.len(),
        tail_offset,
        bytes.len().saturating_sub(tail_offset),
        scene.reached_eof,
    );
    println!("| offset | rel | hex | u32 | f32 |");
    println!("|---:|---:|---|---:|---:|");
    let mut i = tail_offset;
    while i + 4 <= bytes.len() {
        let w = [bytes[i], bytes[i + 1], bytes[i + 2], bytes[i + 3]];
        let v = u32::from_le_bytes(w);
        let f = f32::from_le_bytes(w);
        println!(
            "| {} | {} | {:02x} {:02x} {:02x} {:02x} | {} | {} |",
            i,
            i - tail_offset,
            w[0],
            w[1],
            w[2],
            w[3],
            v,
            if f == 0.0 || (f.abs() > 1e-6 && f.abs() < 1e9) {
                format!("{:.6}", f)
            } else {
                "~".to_string()
            },
        );
        i += 4;
    }
}

/// The decisive section: is the region past `tail_offset` a *geometry*
/// section, or simply the parameter stream continuing past the walker's
/// own `TAG_MAX` cutoff?
///
/// Two independent measurements, neither of which needs a layout
/// hypothesis:
///
/// 1. **Known-tag recurrence.** Every value here is already in the
///    parameter dictionary — `dispatch_tag` classifies it — and is read at
///    4-byte alignment from `tail_offset`. Vertex/index/float payload has
///    no reason to keep reproducing the parameter section's own tag
///    vocabulary at alignment.
/// 2. **Size ceiling.** Baked branch + frond + leaf-card geometry has a
///    floor cost in bytes. The largest file in the corpus is reported
///    below; compare it against even a minimal indexed mesh.
fn report_resync(
    files_with_tail: u32,
    files_with_known_tag_in_tail: u32,
    known_tag_hits: &BTreeMap<u32, u32>,
    file_sizes: &[usize],
    desync_shift: &BTreeMap<usize, u32>,
) {
    println!("\n## Is the tail a geometry section, or the parameter stream continuing?\n");
    println!(
        "The walker stops at `parser::TAG_MAX = {}`. Every tag family found \
         above sits just past it, so this asks whether the cutoff is a real \
         section boundary or an artefact of that constant.\n",
        TAG_MAX
    );

    let pct = files_with_known_tag_in_tail as f32 / files_with_tail.max(1) as f32 * 100.0;
    println!("### 1. Known parameter tags occurring past `tail_offset`\n");
    println!("| metric | value |\n|---|---:|");
    println!("| files with a non-empty tail | {} |", files_with_tail);
    println!(
        "| …containing ≥1 aligned value already in the parameter dictionary | {} ({:.1}%) |",
        files_with_known_tag_in_tail, pct
    );
    println!(
        "| distinct known tags seen in tails | {} |",
        known_tag_hits.len()
    );
    println!(
        "\nA geometry payload has no reason to keep reproducing the parameter \
         section's own tag vocabulary at 4-byte alignment.\n"
    );
    println!("| known tag | occurrences in tails |\n|---:|---:|");
    let mut by_count: Vec<(&u32, &u32)> = known_tag_hits.iter().collect();
    by_count.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
    for (tag, n) in by_count.iter().take(25) {
        println!("| {} | {} |", tag, n);
    }

    println!("\n### 1b. Where the walker actually desynced\n");
    println!(
        "Byte shift from `tail_offset` that maximises known-tag hits. A mode \
         of 0 would mean the walker stopped on a real 4-byte boundary; \
         anything else means it stopped *inside* a payload it mis-sized, and \
         `tail_offset` is a desync point rather than a section start.\n"
    );
    println!("| shift (bytes) | files |\n|---:|---:|");
    for (shift, n) in desync_shift {
        println!("| {} | {} |", shift, n);
    }

    println!("\n### 2. Size ceiling\n");
    let max = file_sizes.iter().max().copied().unwrap_or(0);
    let min = file_sizes.iter().min().copied().unwrap_or(0);
    let mean = if file_sizes.is_empty() {
        0
    } else {
        file_sizes.iter().sum::<usize>() / file_sizes.len()
    };
    println!("| metric | bytes |\n|---|---:|");
    println!("| smallest `.spt` in corpus | {} |", min);
    println!("| mean | {} |", mean);
    println!("| **largest `.spt` in corpus** | **{}** |", max);
    println!(
        "\nFor scale, a single indexed mesh of N vertices carrying position + \
         normal + UV as `f32` costs 32 N bytes before indices. The largest \
         file in the whole corpus ({} B) is under that cost for {} vertices \
         — and that is the *entire file*, parameter section included.\n",
        max,
        max / 32
    );
}
