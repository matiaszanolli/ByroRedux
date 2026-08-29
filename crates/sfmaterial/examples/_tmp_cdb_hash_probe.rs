//! #3398 Phase-2 RE spike, part 2: settle the `BSResource::ID` hash.
//!
//! The CDB stores no material paths (STRT holds 0 `.mat` strings) — only
//! `BSResource::ID { Dir, Ext, File }` triples, where the `File` column is
//! the constant `0x0074616d` ("mat"). So a Phase-2 consumer must be able to
//! *compute* the key from the path a NIF names. This brute-forces candidate
//! hash functions against the 48,749 real keys and reports the match rate.
//!
//! Usage: `_tmp_cdb_hash_probe <paths.txt>` — one material path per line, as
//! produced by `crates/nif/examples/_tmp_sf_matpath_dump.rs`. The paths come
//! off real Starfield NIFs so the test is against strings the engine will
//! actually be asked to resolve. (Split across two examples so neither crate
//! needs a new dependency on the other.)

use byroredux_bsa::Ba2Archive;
use byroredux_sfmaterial::{ComponentDatabaseFile, Value};
use std::collections::HashSet;

/// Reflected CRC-32, poly 0xEDB88320. `init`/`xorout` parameterised because
/// Bethesda's variant historically drops both.
fn crc32(data: &[u8], init: u32, xorout: u32) -> u32 {
    let mut crc = init;
    for &b in data {
        crc ^= b as u32;
        for _ in 0..8 {
            crc = if crc & 1 != 0 { (crc >> 1) ^ 0xEDB8_8320 } else { crc >> 1 };
        }
    }
    crc ^ xorout
}

fn main() {
    let cdb_ba2 = "/mnt/data/SteamLibrary/steamapps/common/Starfield/Data/Starfield - Materials.ba2";
    let ba2 = Ba2Archive::open(cdb_ba2).unwrap();
    let bytes = ba2.extract("materials\\materialsbeta.cdb").unwrap();
    let cdb = ComponentDatabaseFile::parse(&bytes).unwrap();

    // Collect the real keys: (col_a, col_b) ignoring the constant ext col.
    let mut keys: HashSet<(u32, u32)> = HashSet::new();
    let mut col_a: HashSet<u32> = HashSet::new();
    let mut col_b: HashSet<u32> = HashSet::new();
    for inst in &cdb.instances {
        let Value::Object(o) = inst else { continue };
        if o.class_name != "BSMaterial::Internal::CompiledDB" { continue }
        let Some(Value::Map(pairs)) = o.fields.get("HashMap") else { continue };
        for (k, _v) in pairs.iter() {
            let Value::Object(id) = k else { continue };
            let g = |n: &str| match id.fields.get(n) { Some(Value::U32(x)) => *x, _ => 0 };
            let (a, b) = (g("Dir"), g("Ext"));
            keys.insert((a, b));
            col_a.insert(a);
            col_b.insert(b);
        }
        break;
    }
    println!("CDB keys: {} pairs, col Dir={} distinct, col Ext={} distinct\n",
        keys.len(), col_a.len(), col_b.len());

    // Real material paths, one per line (see the module doc).
    let list = std::env::args().nth(1).expect("usage: _tmp_cdb_hash_probe <paths.txt>");
    let paths: HashSet<String> = std::fs::read_to_string(&list)
        .expect("read path list")
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect();
    let paths: Vec<String> = paths.into_iter().collect();
    println!("collected {} distinct material paths off NIFs", paths.len());
    for p in paths.iter().take(10) { println!("     {p}"); }
    println!();

    // Candidate normalisations: the CDB is keyed on `.mat`, NIFs name
    // `.bgsm`/`.bgem`/`.mat`, so try swapping the extension for "mat" and
    // splitting dir/stem on both separators.
    let variants: Vec<(&str, fn(&str) -> (String, String))> = vec![
        ("dir+stem, backslash, no data prefix", |p| split(p, '\\', false)),
        ("dir+stem, forward slash, no data prefix", |p| split(p, '/', false)),
        ("dir+stem, backslash, data\\ prefix", |p| split(p, '\\', true)),
    ];
    let hashes: Vec<(&str, fn(&[u8]) -> u32)> = vec![
        ("crc32 std (init FFFFFFFF, xor FFFFFFFF)", |d| crc32(d, 0xFFFF_FFFF, 0xFFFF_FFFF)),
        ("crc32 bethesda (init 0, xor 0)", |d| crc32(d, 0, 0)),
        ("crc32 (init FFFFFFFF, xor 0)", |d| crc32(d, 0xFFFF_FFFF, 0)),
    ];

    for (vname, vf) in &variants {
        for (hname, hf) in &hashes {
            let (mut hit_ab, mut hit_ba, mut tried) = (0usize, 0usize, 0usize);
            for p in &paths {
                let (dir, stem) = vf(p);
                if stem.is_empty() { continue }
                tried += 1;
                let (hd, hs) = (hf(dir.as_bytes()), hf(stem.as_bytes()));
                if keys.contains(&(hs, hd)) { hit_ab += 1; }   // Dir col = stem
                if keys.contains(&(hd, hs)) { hit_ba += 1; }   // Dir col = dir
                }
            if hit_ab > 0 || hit_ba > 0 {
                println!("MATCH  {vname} | {hname}");
                println!("       stem->Dir col: {hit_ab}/{tried}   dir->Dir col: {hit_ba}/{tried}");
            }
        }
    }
    println!("\n(no MATCH lines above = none of the candidate hashes reproduce the keys)");

    // Break the misses down by extension: the hypothesis is that every miss
    // is a `.bgsm`/`.bgem`-named reference, which the audit already measured
    // as having no backing file anywhere in the 129 vanilla archives.
    let mut miss_by_ext: std::collections::BTreeMap<String, Vec<&String>> = Default::default();
    let mut hit_by_ext: std::collections::BTreeMap<String, usize> = Default::default();
    for p in &paths {
        let (dir, stem) = split(p, '\\', false);
        if stem.is_empty() { continue }
        let ext = p.trim().rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase())
            .unwrap_or_else(|| "<none>".into());
        if keys.contains(&(crc32(stem.as_bytes(), 0, 0), crc32(dir.as_bytes(), 0, 0))) {
            *hit_by_ext.entry(ext).or_default() += 1;
        } else {
            miss_by_ext.entry(ext).or_default().push(p);
        }
    }
    println!("\n## hit/miss by extension (bethesda-crc32, stem->Dir col, dir->Ext col)");
    for (e, n) in &hit_by_ext { println!("     HIT  .{e}: {n}"); }
    for (e, v) in &miss_by_ext {
        println!("     MISS .{e}: {}", v.len());
        for x in v.iter().take(6) { println!("            {x}"); }
    }

    // Also report bare column membership, which localises a partial match.
    let (dir_col, _) = (&col_a, &col_b);
    let mut stem_in_dircol = 0usize;
    for p in paths.iter().take(500) {
        let (_d, s) = split(p, '\\', false);
        if dir_col.contains(&crc32(s.as_bytes(), 0, 0)) { stem_in_dircol += 1 }
    }
    println!("bethesda-crc32(stem) present in Dir column: {stem_in_dircol}/500");
}

/// `materials\foo\bar.bgsm` -> ("materials\foo", "bar") with the extension
/// dropped (the CDB key's third column is always the literal "mat").
fn split(p: &str, sep: char, data_prefix: bool) -> (String, String) {
    let t = p.trim_end_matches(|c: char| c == '\0' || c.is_ascii_whitespace());
    let mut t = t.replace('/', &sep.to_string()).replace('\\', &sep.to_string());
    t = t.to_ascii_lowercase();
    if data_prefix && !t.starts_with(&format!("data{sep}")) {
        t = format!("data{sep}{t}");
    }
    let stem_full = match t.rsplit_once(sep) { Some((_, f)) => f, None => &t };
    let dir = match t.rsplit_once(sep) { Some((d, _)) => d.to_string(), None => String::new() };
    let stem = match stem_full.rsplit_once('.') { Some((s, _)) => s.to_string(), None => stem_full.to_string() };
    (dir, stem)
}
