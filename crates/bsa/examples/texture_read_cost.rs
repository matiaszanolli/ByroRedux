// M39 Phase 0 measurement (docs/prd/texture-streaming.md, §9 Phase 0,
// decision Q2 (b)): the cost of a whole-file texture read against a
// floor-only read.
//
// - `ba2 <archive>...` (zlib BA2, i.e. Fallout 4): for a deterministic sample
//   of streamable DX10 records (level 0 above the floor edge, more than one
//   chunk), times `Ba2Archive::extract` (every chunk inflated, header
//   rebuilt) against reading and inflating only the last chunk (the floor,
//   decision Q3), straight from the chunk offset `dx10_layout` reports.
// - `bsa <archive>...`: times `BsaArchive::extract` for a sample of DDS
//   files whose level 0 is above the floor edge. A compressed BSA file is
//   one zlib / LZ4 stream with level 0 first, so a floor-only read inflates
//   the same bytes: the whole-file time is the floor time.
//
// Each sample is read twice and the second pass is timed, so the page cache
// is warm and the figure is the inflate, not the disk.
//
// Usage:
//   cargo run --release -p byroredux-bsa --example texture_read_cost -- ba2 <a.ba2>...
//   cargo run --release -p byroredux-bsa --example texture_read_cost -- bsa <a.bsa>...
use byroredux_bsa::{Ba2Archive, BsaArchive};
use std::io::Read;
use std::os::unix::fs::FileExt;
use std::time::{Duration, Instant};

const FLOOR_EDGE: u32 = 256;
const SAMPLE_EVERY: usize = 7;
const MAX_SAMPLES: usize = 2000;

fn read_u32(data: &[u8], off: usize) -> u32 {
    u32::from_le_bytes([data[off], data[off + 1], data[off + 2], data[off + 3]])
}

fn mib(b: u64) -> f64 {
    b as f64 / (1024.0 * 1024.0)
}

fn rate(bytes: u64, d: Duration) -> f64 {
    mib(bytes) / d.as_secs_f64().max(1e-9)
}

fn ba2(paths: &[String]) {
    let (mut n, mut whole_bytes, mut tail_bytes) = (0usize, 0u64, 0u64);
    let (mut whole_t, mut tail_t) = (Duration::ZERO, Duration::ZERO);
    for path in paths {
        let archive = Ba2Archive::open(path).expect("open ba2");
        let file = std::fs::File::open(path).expect("open ba2 file");
        let mut names: Vec<&str> = archive.list_files();
        names.sort_unstable();
        for (i, name) in names.into_iter().enumerate() {
            if n >= MAX_SAMPLES || i % SAMPLE_EVERY != 0 {
                continue;
            }
            let Some(layout) = archive.dx10_layout(name) else {
                continue;
            };
            if layout.is_cubemap
                || layout.chunks.len() < 2
                || u32::from(layout.width.max(layout.height)) <= FLOOR_EDGE
            {
                continue;
            }
            let tail = *layout.chunks.last().expect("multi-chunk");
            if tail.packed_size == 0 {
                continue; // raw tail: nothing to inflate, not this measurement
            }
            let read_tail = || {
                let mut packed = vec![0u8; tail.packed_size as usize];
                file.read_exact_at(&mut packed, tail.offset)
                    .expect("read tail");
                let mut out = Vec::with_capacity(tail.unpacked_size as usize);
                flate2::read::ZlibDecoder::new(&packed[..])
                    .read_to_end(&mut out)
                    .expect("inflate tail");
                out.len() as u64
            };
            let _ = archive.extract(name).expect("warm whole");
            let _ = read_tail();
            let t0 = Instant::now();
            let whole = archive.extract(name).expect("whole");
            whole_t += t0.elapsed();
            let t1 = Instant::now();
            let tail_len = read_tail();
            tail_t += t1.elapsed();
            whole_bytes += whole.len() as u64;
            tail_bytes += tail_len;
            n += 1;
        }
    }
    println!("## BA2 read cost (floor = last chunk)\n");
    println!("| Measure | Whole-file read | Floor-only read |");
    println!("|---|---|---|");
    println!("| Samples | {n} | {n} |");
    println!(
        "| Inflated bytes | {:.1} MiB | {:.1} MiB |",
        mib(whole_bytes),
        mib(tail_bytes)
    );
    println!(
        "| Time (warm cache) | {:.1} ms | {:.1} ms |",
        whole_t.as_secs_f64() * 1e3,
        tail_t.as_secs_f64() * 1e3
    );
    println!(
        "| Per texture | {:.3} ms | {:.3} ms |",
        whole_t.as_secs_f64() * 1e3 / n.max(1) as f64,
        tail_t.as_secs_f64() * 1e3 / n.max(1) as f64
    );
    println!(
        "| Throughput | {:.0} MiB/s | {:.0} MiB/s |",
        rate(whole_bytes, whole_t),
        rate(tail_bytes, tail_t)
    );
    println!(
        "| Floor / whole time | | {:.1} % |",
        100.0 * tail_t.as_secs_f64() / whole_t.as_secs_f64().max(1e-9)
    );
}

fn bsa(paths: &[String]) {
    let (mut n, mut bytes) = (0usize, 0u64);
    let mut t = Duration::ZERO;
    for path in paths {
        let archive = BsaArchive::open(path).expect("open bsa");
        let mut names: Vec<&str> = archive
            .list_files()
            .into_iter()
            .filter(|f| f.to_ascii_lowercase().ends_with(".dds"))
            .collect();
        names.sort_unstable();
        for (i, name) in names.into_iter().enumerate() {
            if n >= MAX_SAMPLES || i % SAMPLE_EVERY != 0 {
                continue;
            }
            let Ok(warm) = archive.extract(name) else {
                continue;
            };
            if warm.len() < 128
                || &warm[..4] != b"DDS "
                || read_u32(&warm, 12).max(read_u32(&warm, 16)) <= FLOOR_EDGE
                || read_u32(&warm, 28) <= 1
            {
                continue;
            }
            let t0 = Instant::now();
            let whole = archive.extract(name).expect("whole");
            t += t0.elapsed();
            bytes += whole.len() as u64;
            n += 1;
        }
    }
    println!("## BSA read cost (a floor-only read of a compressed file inflates the whole file)\n");
    println!("| Measure | Whole-file read = floor read |");
    println!("|---|---|");
    println!("| Samples | {n} |");
    println!("| Inflated bytes | {:.1} MiB |", mib(bytes));
    println!("| Time (warm cache) | {:.1} ms |", t.as_secs_f64() * 1e3);
    println!(
        "| Per texture | {:.3} ms |",
        t.as_secs_f64() * 1e3 / n.max(1) as f64
    );
    println!("| Throughput | {:.0} MiB/s |", rate(bytes, t));
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("ba2") => ba2(&args[1..]),
        Some("bsa") => bsa(&args[1..]),
        _ => {
            eprintln!("usage: texture_read_cost (ba2|bsa) <archive>...");
            std::process::exit(2);
        }
    }
}
