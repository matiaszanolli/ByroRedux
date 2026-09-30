//! #3398 Phase-2 RE probe, part 3: the ComponentInfo ↔ instance-stream join
//! and the per-material component graph.
//!
//! The Phase-2 spike (`docs/audits/SF_CDB_PHASE2_SPIKE_2026-08-29.md`)
//! established the *key* (path → `BSResource::ID` pair via reflected
//! CRC-32) but left two questions open, and both decide the shape of the
//! Phase-2 index builder:
//!
//! 1. **Alignment**: the file's only object→component map is
//!    `BSComponentDB2::DBFileIndex.Components` (`ObjectID, Type, Index`
//!    rows). The top-level instance stream carries the actual class
//!    instances, with no identity field of its own. `Components` has
//!    exactly as many rows as the stream has object instances (minus the
//!    two index instances), which suggests positional alignment:
//!    `Components[j]` describes stream instance `j + 2`. This probe
//!    verifies that hypothesis class-by-class over the whole corpus
//!    instead of assuming it.
//! 2. **Role encoding**: which `BSMaterial::*` component (and which edge
//!    hop) carries the diffuse/normal/roughness data a material names.
//!    The demo join prints one resolved material's full component set and
//!    one edge hop so the encoding can be read off real data.
//!
//! Memory: this materialises the `DBFileIndex` instance as one generic
//! `Value` tree before compacting it (measured high-water mark printed at
//! exit). The production index builder will re-use this same shape, so
//! the number doubles as the Phase-2 peak-memory measurement.
//!
//! Usage:
//! `cargo run --release -p byroredux-sfmaterial --example cdb_join_probe
//!   -- <paths.txt>` — one material path per line, as dumped off real
//! Starfield NIFs by `crates/nif/examples/sf_matpath_dump.rs`.

use byroredux_bsa::Ba2Archive;
use byroredux_sfmaterial::{ComponentDatabaseFile, Value};
use std::collections::{BTreeMap, HashMap, HashSet};

/// Reflected CRC-32, poly 0xEDB88320, init 0, xorout 0 — the "Bethesda"
/// variant identified by `cdb_key_hash_probe`.
fn crc32(data: &[u8]) -> u32 {
    let mut crc: u32 = 0;
    for &b in data {
        crc ^= b as u32;
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0xEDB8_8320
            } else {
                crc >> 1
            };
        }
    }
    crc
}

/// `materials\foo\bar.<any>` → (dir, stem), backslash, lowercased, with the
/// extension dropped (the CDB key's third column is always the literal
/// "mat" — the lookup ignores the reference's own suffix).
fn split_key(p: &str) -> (String, String) {
    let t = p.trim_end_matches(|c: char| c == '\0' || c.is_ascii_whitespace());
    let mut t = t.replace('/', "\\").to_ascii_lowercase();
    if let Some(stripped) = t.strip_prefix("data\\") {
        t = stripped.to_string();
    }
    let (dir, stem_full) = match t.rsplit_once('\\') {
        Some((d, f)) => (d.to_string(), f.to_string()),
        None => (String::new(), t.clone()),
    };
    let stem = match stem_full.rsplit_once('.') {
        Some((s, _)) => s.to_string(),
        None => stem_full,
    };
    (dir, stem)
}

/// Flattened leaf value for component-field capture.
#[derive(Debug, Clone)]
#[allow(dead_code)]
enum Leaf {
    F(f32),
    U(u64),
    I(i64),
    B(bool),
    S(String),
}

fn collect_leaves(value: &Value, prefix: &str, out: &mut Vec<(String, Leaf)>) {
    match value {
        Value::Object(o) => {
            for (n, v) in &o.fields {
                collect_leaves(v, &format!("{prefix}.{n}"), out);
            }
        }
        Value::Ref(r) => collect_leaves(&r.inner, prefix, out),
        Value::Float(x) => out.push((prefix.into(), Leaf::F(*x))),
        Value::Double(x) => out.push((prefix.into(), Leaf::F(*x as f32))),
        Value::U8(x) => out.push((prefix.into(), Leaf::U(*x as u64))),
        Value::U16(x) => out.push((prefix.into(), Leaf::U(*x as u64))),
        Value::U32(x) => out.push((prefix.into(), Leaf::U(*x as u64))),
        Value::U64(x) => out.push((prefix.into(), Leaf::U(*x))),
        Value::I8(x) => out.push((prefix.into(), Leaf::I(*x as i64))),
        Value::I16(x) => out.push((prefix.into(), Leaf::I(*x as i64))),
        Value::I32(x) => out.push((prefix.into(), Leaf::I(*x as i64))),
        Value::I64(x) => out.push((prefix.into(), Leaf::I(*x))),
        Value::Bool(b) => out.push((prefix.into(), Leaf::B(*b))),
        Value::String(s) => out.push((prefix.into(), Leaf::S(s.clone()))),
        Value::List(l) => {
            for (i, v) in l.iter().enumerate() {
                collect_leaves(v, &format!("{prefix}[{i}]"), out);
            }
        }
        Value::Map(m) => {
            // Maps at component level are rare; capture keys only so the
            // demo print still shows what shape was there.
            out.push((prefix.into(), Leaf::U(m.len() as u64)));
        }
        Value::Null => {}
    }
}

/// `BSMaterial::*` classes whose leaf fields the Phase-2 index needs.
const NEEDED_CLASSES: &[&str] = &[
    "BSMaterial::MRTextureFile",
    "BSMaterial::TextureFile",
    "BSMaterial::MaterialParamFloat",
    "BSMaterial::ParamBool",
    "BSMaterial::Color",
    "BSMaterial::AlphaSettingsComponent",
    "BSMaterial::AlphaBlenderSettings",
    "BSMaterial::EffectSettingsComponent",
    "BSMaterial::TranslucencySettings",
    "BSMaterial::DecalSettingsComponent",
    "BSMaterial::EmissiveSettingsComponent",
    "BSMaterial::EmittanceSettings",
    "BSMaterial::ShaderRouteComponent",
    "BSMaterial::ShaderModelComponent",
    "BSMaterial::Offset",
    "BSMaterial::Scale",
    "BSMaterial::Channel",
    "BSMaterial::TextureAddressModeComponent",
    "BSMaterial::TextureResolutionSetting",
    "BSMaterial::MipBiasSetting",
    "BSMaterial::FlowSettingsComponent",
    "BSMaterial::OpacityComponent",
    "BSMaterial::LevelOfDetailSettings",
    "BSMaterial::LODMaterialID",
    "BSMaterial::MaterialID",
    "BSMaterial::TextureSetID",
    "BSMaterial::LayerID",
    "BSMaterial::BlenderID",
    "BSMaterial::UVStreamID",
    "BSMaterial::CollisionComponent",
];

/// Compact `DBFileIndex` tables, captured typed during the visit.
#[derive(Default)]
struct DbIndex {
    /// ComponentTypes rows: (type u16, class name).
    component_types: Vec<(u64, String)>,
    /// Components rows: (ObjectID.Value, Type, Index).
    components: Vec<(u64, u64, u64)>,
    /// Objects rows: (DBID.Value, Parent.Value, PersistentID dir/ext/file).
    objects: Vec<(u64, u64, [u32; 3])>,
    /// Edges rows: (SourceID.Value, TargetID.Value, Type leaf as u64).
    edges: Vec<(u64, u64, u64)>,
    /// Raw first-20 ComponentTypeInfo rows for schema inspection.
    component_types_raw: Vec<Vec<(String, Leaf)>>,
    /// Raw first-8 EdgeInfo rows for schema inspection.
    edges_raw: Vec<Vec<(String, Leaf)>>,
    /// Raw first-4 ObjectInfo rows for schema inspection.
    objects_raw: Vec<Vec<(String, Leaf)>>,
    /// Field-name mismatches met while capturing, once per name.
    field_misses: HashSet<String>,
}

/// `BSComponentDB2::ID`-shaped struct: `.Value` u32 (any int width).
fn id_value(obj: &Value, name: &str, rows: &mut DbIndex) -> Option<u64> {
    let Value::Object(o) = obj else { return None };
    match o.fields.get(name) {
        Some(Value::Object(inner)) => match inner.fields.get("Value") {
            Some(Value::U32(x)) => Some(*x as u64),
            Some(Value::U16(x)) => Some(*x as u64),
            Some(Value::U64(x)) => Some(*x),
            Some(Value::I32(x)) => Some(*x as u64),
            Some(Value::U8(x)) => Some(*x as u64),
            _ => {
                rows.field_misses.insert(format!("{name}.Value"));
                None
            }
        },
        _ => {
            rows.field_misses.insert(name.to_string());
            None
        }
    }
}

/// `BSResource::ID`-shaped struct: three u32 columns (labels rotated —
/// see the spike §1; captured positionally as [a, b, c]).
fn resource_id(obj: &Value, name: &str, rows: &mut DbIndex) -> Option<[u32; 3]> {
    let Value::Object(o) = obj else { return None };
    let Some(Value::Object(inner)) = o.fields.get(name) else {
        rows.field_misses.insert(name.to_string());
        return None;
    };
    let mut cols = [0u32; 3];
    let mut i = 0;
    for v in inner.fields.values() {
        match v {
            Value::U32(x) => {
                if i < 3 {
                    cols[i] = *x;
                }
                i += 1;
            }
            Value::U16(x) => {
                if i < 3 {
                    cols[i] = *x as u32;
                }
                i += 1;
            }
            _ => {}
        }
    }
    if i != 3 {
        rows.field_misses.insert(format!("{name}({i} cols)"));
        return None;
    }
    Some(cols)
}

fn main() {
    let started = std::time::Instant::now();
    let mut args = std::env::args().skip(1).collect::<Vec<_>>();
    let (cdb_ba2, cdb_inner) = match args.len() {
        n if n >= 2 && !args[0].ends_with(".txt") => {
            let inner = args[1].clone();
            let ba2 = args.remove(0);
            args.remove(0);
            (ba2, inner)
        }
        _ => (
            "/mnt/data/SteamLibrary/steamapps/common/Starfield/Data/Starfield - Materials.ba2"
                .to_string(),
            "materials\\materialsbeta.cdb".to_string(),
        ),
    };
    let list = args.first().cloned();
    let ba2 = Ba2Archive::open(&cdb_ba2).unwrap();
    let bytes = ba2.extract(&cdb_inner).unwrap();
    eprintln!("CDB source: {cdb_ba2} :: {cdb_inner}");
    eprintln!(
        "[{:>7.1}s] CDB extracted ({} bytes)",
        started.elapsed().as_secs_f32(),
        bytes.len()
    );

    let mut idx = DbIndex::default();
    let mut keys: HashMap<(u32, u32), u64> = HashMap::new();
    let mut class_ids: HashMap<String, u16> = HashMap::new();
    let mut class_names: Vec<String> = Vec::new();
    let mut stream_classes: Vec<u16> = Vec::with_capacity(1_500_000);
    let mut stream_counts: HashMap<u16, u64> = HashMap::new();
    // Per-class captured component fields, keyed by stream position.
    #[allow(clippy::type_complexity)]
    let mut captured: HashMap<u16, Vec<(u32, Vec<(String, Leaf)>)>> = HashMap::new();
    let needed: HashSet<&str> = NEEDED_CLASSES.iter().copied().collect();
    let mut top_level_other = 0u64;
    let mut ctname_samples: Vec<Vec<(String, Leaf)>> = Vec::new();

    let info = ComponentDatabaseFile::visit_instances_with_limits(
        &bytes,
        byroredux_sfmaterial::ParseLimits::unlimited(),
        |value| {
            let pos = stream_classes.len() as u32;
            let Value::Object(o) = value else {
                top_level_other += 1;
                stream_classes.push(u16::MAX);
                return;
            };
            let class_id = match class_ids.get(o.class_name.as_str()) {
                Some(id) => *id,
                None => {
                    let id = class_names.len() as u16;
                    class_names.push(o.class_name.clone());
                    class_ids.insert(o.class_name.clone(), id);
                    id
                }
            };
            stream_classes.push(class_id);
            *stream_counts.entry(class_id).or_insert(0) += 1;

            match o.class_name.as_str() {
                "BSComponentDB::CTName" => {
                    if ctname_samples.len() < 8 {
                        let mut leaves = Vec::new();
                        collect_leaves(value, "", &mut leaves);
                        ctname_samples.push(leaves);
                    }
                }
                "BSMaterial::Internal::CompiledDB" => {
                    if let Some(Value::Map(pairs)) = o.fields.get("HashMap") {
                        for (k, v) in pairs.iter() {
                            let (Value::Object(id), val) = (k, v) else { continue };
                            let g = |n: &str| match id.fields.get(n) {
                                Some(Value::U32(x)) => *x,
                                _ => 0,
                            };
                            let dbid = match val {
                                Value::U64(x) => *x,
                                Value::U32(x) => *x as u64,
                                _ => continue,
                            };
                            keys.insert((g("Dir"), g("Ext")), dbid);
                        }
                    }
                }
                "BSComponentDB2::DBFileIndex" => capture_db_file_index(o, &mut idx),
                other => {
                    if needed.contains(other) {
                        let mut leaves = Vec::new();
                        collect_leaves(value, "", &mut leaves);
                        captured.entry(class_id).or_default().push((pos, leaves));
                    }
                }
            }
        },
    )
    .unwrap();
    eprintln!(
        "[{:>7.1}s] visited {} values ({} classes, {} non-object top-levels); \
         HWM {} MB",
        started.elapsed().as_secs_f32(),
        info.value_count,
        class_names.len(),
        top_level_other,
        hwm_mb(),
    );
    eprintln!(
        "captured: {} keys, {} components, {} objects, {} edges, {} component types; \
         field misses: {:?}",
        keys.len(),
        idx.components.len(),
        idx.objects.len(),
        idx.edges.len(),
        idx.component_types.len(),
        idx.field_misses,
    );

    println!(
        "== ComponentTypes raw rows (first {}) ==",
        idx.component_types_raw.len()
    );
    for row in &idx.component_types_raw {
        println!("   {row:?}");
    }
    println!("== Objects raw rows (first {}) ==", idx.objects_raw.len());
    for row in &idx.objects_raw {
        println!("   {row:?}");
    }
    println!("== Edges raw rows (first {}) ==", idx.edges_raw.len());
    for row in &idx.edges_raw {
        println!("   {row:?}");
    }
    println!("== CTName raw rows (first {}) ==", ctname_samples.len());
    for row in &ctname_samples {
        println!("   {row:?}");
    }

    // ── alignment hypothesis: Components[j] ↔ stream instance j + 2 ──
    let mut checked = 0u64;
    let mut aligned = 0u64;
    let mut mismatch_samples: Vec<(usize, String, String)> = Vec::new();
    for (j, (obj_id, ty, _index)) in idx.components.iter().enumerate() {
        let stream_pos = j + 2;
        let Some(&sid) = stream_classes.get(stream_pos) else { break };
        checked += 1;
        let expected = idx
            .component_types
            .iter()
            .find(|(id, _)| *id == *ty)
            .map(|(_, n)| n.clone())
            .unwrap_or_default();
        let actual = class_names
            .get(sid as usize)
            .cloned()
            .unwrap_or_default();
        if actual == expected {
            aligned += 1;
        } else if mismatch_samples.len() < 12 {
            mismatch_samples.push((stream_pos, expected, actual));
        }
        let _ = obj_id;
    }
    println!("\n== alignment Components[j] ↔ stream[j+2]: {aligned}/{checked} ==");
    for (pos, exp, act) in &mismatch_samples {
        println!("   MISMATCH stream[{pos}]: expected {exp:?}, got {act:?}");
    }
    let mut top: Vec<(u64, &String)> = stream_counts
        .iter()
        .filter(|(id, _)| **id != u16::MAX)
        .map(|(id, c)| (*c, &class_names[*id as usize]))
        .collect();
    top.sort_by_key(|&(c, _)| std::cmp::Reverse(c));
    println!("\n== top stream classes ==");
    for (c, n) in top.iter().take(12) {
        println!("   {c:>9}  {n}");
    }
    // Secondary hypothesis: stream[j] (index instances included) ↔
    // Components[j-2] from the other side — same test; skip.

    // Per-type stream counts vs ComponentTypes for any mismatched type.
    let mut type_by_class: HashMap<&str, u64> = HashMap::new();
    for (id, n) in &idx.component_types {
        type_by_class.insert(Box::leak(n.clone().into_boxed_str()), *id);
    }

    // ── key resolution over the NIF-named path corpus ──
    let mut resolved = 0usize;
    let mut missed = 0usize;
    let mut demo_paths: Vec<(String, u64)> = Vec::new();
    if let Some(list) = &list {
        let paths: Vec<String> = std::fs::read_to_string(list)
            .expect("read path list")
            .lines()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty())
            .collect();
        println!("\n== key resolution over {} NIF-named paths ==", paths.len());
        for p in &paths {
            let (dir, stem) = split_key(p);
            let key = (crc32(stem.as_bytes()), crc32(dir.as_bytes()));
            match keys.get(&key) {
                Some(dbid) => {
                    resolved += 1;
                    if demo_paths.len() < 3 {
                        demo_paths.push((p.clone(), *dbid));
                    }
                }
                None => missed += 1,
            }
        }
        println!("   resolved {resolved}, missed {missed}");
        for p in paths.iter().filter(|p| {
            let (dir, stem) = split_key(p);
            !keys.contains_key(&(crc32(stem.as_bytes()), crc32(dir.as_bytes())))
        }).take(12) {
            println!("      MISS {p}");
        }
    }

    // ── demo join: components + one edge hop per resolved path ──
    let mut obj_row_by_dbid: HashMap<u64, usize> = HashMap::new();
    for (i, (dbid, _parent, _pid)) in idx.objects.iter().enumerate() {
        obj_row_by_dbid.entry(*dbid).or_insert(i);
    }
    // PersistentID (Dir, Ext) → object row, both column orders.
    let mut obj_row_by_pid_he: HashMap<(u32, u32), usize> = HashMap::new();
    let mut obj_row_by_pid_le: HashMap<(u32, u32), usize> = HashMap::new();
    for (i, (_dbid, _parent, pid)) in idx.objects.iter().enumerate() {
        obj_row_by_pid_he.entry((pid[0], pid[1])).or_insert(i);
        obj_row_by_pid_le.entry((pid[1], pid[0])).or_insert(i);
    }
    let mut comps_by_object: HashMap<u64, Vec<usize>> = HashMap::new();
    for (j, (obj_id, _ty, _index)) in idx.components.iter().enumerate() {
        comps_by_object.entry(*obj_id).or_default().push(j);
    }
    let mut edges_by_source: HashMap<u64, Vec<usize>> = HashMap::new();
    for (i, (src, _dst, _ty)) in idx.edges.iter().enumerate() {
        edges_by_source.entry(*src).or_default().push(i);
    }

    // ── census 1: MRTextureFile per-object Index → filename suffix ──
    {
        use std::collections::BTreeMap;
        let tex_id = class_ids.get("BSMaterial::MRTextureFile").copied();
        let mut by_index: BTreeMap<u64, BTreeMap<String, u64>> = BTreeMap::new();
        if let Some(tid) = tex_id {
            if let Some(recs) = captured.get(&tid) {
                for (pos, leaves) in recs {
                    let (_obj, _ty, index) = idx.components[*pos as usize - 2];
                    let file = leaves.iter().find_map(|(n, l)| match (n.as_str(), l) {
                        (".FileName", Leaf::S(s)) => Some(s.clone()),
                        _ => None,
                    });
                    let Some(file) = file else { continue };
                    let lower = file.replace('/', "\\").to_ascii_lowercase();
                    let stem = lower.rsplit_once('.').map(|(s, _)| s).unwrap_or(&lower);
                    let suffix = stem.rsplit('_').next().unwrap_or("").to_string();
                    *by_index
                        .entry(index)
                        .or_default()
                        .entry(suffix)
                        .or_insert(0) += 1;
                }
            }
        }
        println!("\n== MRTextureFile Index → top filename suffixes ==");
        for (index, suffixes) in &by_index {
            let mut top: Vec<(u64, &String)> =
                suffixes.iter().map(|(s, c)| (*c, s)).collect();
            top.sort_by_key(|&(c, _)| std::cmp::Reverse(c));
            let total: u64 = top.iter().map(|(c, _)| c).sum();
            let heads: Vec<String> = top
                .iter()
                .take(5)
                .map(|(c, s)| format!("{s}:{c}"))
                .collect();
            println!("   slot {index}: {total} textures — {}", heads.join(", "));
        }
    }

    // ── census 2: MaterialParamFloat Index → value stats ──
    {
        let fid = class_ids.get("BSMaterial::MaterialParamFloat").copied();
        let mut by_index: BTreeMap<u64, (u64, f32, f32, f64)> = BTreeMap::new();
        if let Some(fid) = fid {
            if let Some(recs) = captured.get(&fid) {
                for (pos, leaves) in recs {
                    let (_obj, _ty, index) = idx.components[*pos as usize - 2];
                    let val = leaves.iter().find_map(|(n, l)| match (n.as_str(), l) {
                        (".Value", Leaf::F(x)) => Some(*x),
                        _ => None,
                    });
                    let Some(val) = val else { continue };
                    let e = by_index.entry(index).or_insert((0, f32::MAX, f32::MIN, 0.0));
                    e.0 += 1;
                    e.1 = e.1.min(val);
                    e.2 = e.2.max(val);
                    e.3 += val as f64;
                }
            }
        }
        println!("\n== MaterialParamFloat Index → stats ==");
        for (index, (n, min, max, sum)) in &by_index {
            println!(
                "   param {index}: {n} values, min {min:.3}, max {max:.3}, mean {:.3}",
                sum / *n as f64
            );
        }
    }

    // ── census 3: ParamBool Index → count ──
    {
        let bid = class_ids.get("BSMaterial::ParamBool").copied();
        let mut by_index: BTreeMap<u64, u64> = BTreeMap::new();
        if let Some(bid) = bid {
            if let Some(recs) = captured.get(&bid) {
                for (pos, _leaves) in recs {
                    let (_obj, _ty, index) = idx.components[*pos as usize - 2];
                    *by_index.entry(index).or_insert(0) += 1;
                }
            }
        }
        println!("\n== ParamBool Index → count ==");
        for (index, n) in &by_index {
            println!("   bool {index}: {n}");
        }
    }

    for (path, dbid) in &demo_paths {
        let (dir, stem) = split_key(path);
        let key = (crc32(stem.as_bytes()), crc32(dir.as_bytes()));
        let via_pid_he = obj_row_by_pid_he.get(&key);
        let via_pid_le = obj_row_by_pid_le.get(&key);
        let via_dbid = obj_row_by_dbid.get(dbid);
        println!("\n== {path} → CompiledDB value {dbid} ==");
        println!("   path key (stem_crc, dir_crc) = ({}, {})", key.0, key.1);
        println!(
            "   row via pid(stem,dir): {via_pid_he:?}  via pid(dir,stem): {via_pid_le:?}  via dbid: {via_dbid:?}"
        );
        let obj_row = via_pid_he.or(via_pid_le).copied();
        let Some(obj_row) = obj_row else {
            println!("   (no ObjectInfo row — cannot join)");
            continue;
        };
        let dbid = idx.objects[obj_row].0;
        println!(
            "   ObjectInfo row {obj_row}: dbid {} parent {} persistent {:?}",
            idx.objects[obj_row].0,
            idx.objects[obj_row].1,
            idx.objects[obj_row].2
        );
        let mut visited = HashSet::new();
        walk_object(
            dbid,
            &comps_by_object,
            &idx.components,
            &stream_classes,
            &class_names,
            &captured,
            0,
            4,
            &mut visited,
        );
        if let Some(edge_rows) = edges_by_source.get(&dbid) {
            for &er in edge_rows.iter().take(8) {
                let (src, dst, ty) = idx.edges[er];
                let dst_components = comps_by_object.get(&dst).map(|v| v.len()).unwrap_or(0);
                println!(
                    "   edge src {src} → dst {dst} (type {ty}); dst has {dst_components} components"
                );
                if dst_components > 0 {
                    print_object_components(
                        dst,
                        &comps_by_object,
                        &idx.components,
                        &stream_classes,
                        &class_names,
                        &captured,
                        1,
                    );
                }
            }
        } else {
            println!("   (no outgoing edges from {dbid})");
        }
    }

    println!(
        "\n[{:>7.1}s] done; HWM {} MB",
        started.elapsed().as_secs_f32(),
        hwm_mb()
    );
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn walk_object(
    dbid: u64,
    comps_by_object: &HashMap<u64, Vec<usize>>,
    components: &[(u64, u64, u64)],
    stream_classes: &[u16],
    class_names: &[String],
    captured: &HashMap<u16, Vec<(u32, Vec<(String, Leaf)>)>>,
    depth: usize,
    max_depth: usize,
    visited: &mut HashSet<u64>,
) {
    if !visited.insert(dbid) || depth > max_depth {
        return;
    }
    let pad = "  ".repeat(depth + 1);
    println!("{pad}── object {dbid} (depth {depth}) ──");
    let Some(rows) = comps_by_object.get(&dbid) else {
        println!("{pad}(no components)");
        return;
    };
    let mut child_refs: Vec<u64> = Vec::new();
    for &row in rows.iter().take(40) {
        let (_obj, ty, index) = components[row];
        let stream_pos = row + 2;
        let sid = stream_classes.get(stream_pos).copied().unwrap_or(u16::MAX);
        let class = class_names.get(sid as usize).cloned().unwrap_or_default();
        println!("{pad}[{stream_pos}] {class} (Type {ty}, Index {index})");
        if let Some(id) = class_names.iter().position(|n| *n == class) {
            if let Some(recs) = captured.get(&(id as u16)) {
                if let Some((_pos, leaves)) = recs.iter().find(|(p, _)| *p as usize == stream_pos) {
                    for (n, leaf) in leaves.iter().take(14) {
                        println!("{pad}    {n} = {leaf:?}");
                        if n == ".ID.Value" {
                            if let Leaf::U(v) = leaf {
                                if *v != 0 {
                                    child_refs.push(*v);
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    if rows.len() > 40 {
        println!("{pad}… ({} components total)", rows.len());
    }
    child_refs.sort_unstable();
    child_refs.dedup();
    if depth < max_depth {
        for child in child_refs.into_iter().take(16) {
            walk_object(
                child,
                comps_by_object,
                components,
                stream_classes,
                class_names,
                captured,
                depth + 1,
                max_depth,
                visited,
            );
        }
    }
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn print_object_components(
    dbid: u64,
    comps_by_object: &HashMap<u64, Vec<usize>>,
    components: &[(u64, u64, u64)],
    stream_classes: &[u16],
    class_names: &[String],
    captured: &HashMap<u16, Vec<(u32, Vec<(String, Leaf)>)>>,
    indent: usize,
) {
    let pad = "  ".repeat(indent + 1);
    let Some(rows) = comps_by_object.get(&dbid) else {
        println!("{pad}(object {dbid}: no components)");
        return;
    };
    for &row in rows.iter().take(24) {
        let (_obj, ty, index) = components[row];
        let stream_pos = row + 2;
        let sid = stream_classes.get(stream_pos).copied().unwrap_or(u16::MAX);
        let class = class_names.get(sid as usize).cloned().unwrap_or_default();
        println!("{pad}[{stream_pos}] {class} (Type {ty}, Index {index})");
        // Leaves live in `captured` only for needed classes; print what's there.
        if let Some(id) = class_names.iter().position(|n| *n == class) {
            if let Some(recs) = captured.get(&(id as u16)) {
                if let Some((_pos, leaves)) = recs.iter().find(|(p, _)| *p as usize == stream_pos) {
                    for (n, leaf) in leaves.iter().take(12) {
                        println!("{pad}    {n} = {leaf:?}");
                    }
                }
            }
        }
    }
    if rows.len() > 24 {
        println!("{pad}… ({} components total)", rows.len());
    }
}

fn capture_db_file_index(o: &byroredux_sfmaterial::ObjectInstance, idx: &mut DbIndex) {
    if idx.field_misses.insert("(dbindex-shape)".into()) {
        for (name, value) in &o.fields {
            let kind = match value {
                Value::List(l) => format!("List[{}]", l.len()),
                Value::Map(m) => format!("Map[{}]", m.len()),
                Value::Object(o) => format!("Object{{{}}}", o.class_name),
                other => format!("{other:?}"),
            };
            eprintln!("[join-probe] DBFileIndex.{name} = {kind}");
        }
    }
    for (name, value) in &o.fields {
        if name == "ComponentTypes" {
            let Value::Map(pairs) = value else {
                eprintln!("[join-probe] ComponentTypes not a Map? {name}");
                continue;
            };
            for (i, (k, v)) in pairs.iter().enumerate() {
                let mut leaves = Vec::new();
                collect_leaves(k, ".Key", &mut leaves);
                collect_leaves(v, ".Val", &mut leaves);
                if i < 6 {
                    idx.component_types_raw.push(leaves.clone());
                }
                let ty = leaves.iter().find_map(|(n, l)| match (n.as_str(), l) {
                    (".Key", Leaf::U(x)) => Some(*x),
                    _ => None,
                });
                let cname = leaves
                    .iter()
                    .find_map(|(n, l)| match (n.as_str(), l) {
                        (".Val.Class", Leaf::S(s)) => Some(s.clone()),
                        (".Val.TypeName", Leaf::S(s)) => Some(s.clone()),
                        (".Val.ClassName", Leaf::S(s)) => Some(s.clone()),
                        (".Val.Name", Leaf::S(s)) => Some(s.clone()),
                        _ => None,
                    });
                if let (Some(ty), Some(cname)) = (ty, cname) {
                    idx.component_types.push((ty, cname));
                }
            }
            continue;
        }
        let Value::List(items) = value else {
            continue;
        };
        match name.as_str() {
            "Components" => {
                for item in items.iter() {
                    let obj_id = id_value(item, "ObjectID", idx);
                    let mut leaves = Vec::new();
                    collect_leaves(item, "", &mut leaves);
                    let ty = leaves.iter().find_map(|(n, l)| match (n.as_str(), l) {
                        (".Type", Leaf::U(x)) => Some(*x),
                        _ => None,
                    });
                    let index = leaves.iter().find_map(|(n, l)| match (n.as_str(), l) {
                        (".Index", Leaf::U(x)) => Some(*x),
                        _ => None,
                    });
                    if let (Some(obj_id), Some(ty), Some(index)) = (obj_id, ty, index) {
                        idx.components.push((obj_id, ty, index));
                    }
                }
            }
            "Objects" => {
                for (i, item) in items.iter().enumerate() {
                    let dbid = id_value(item, "DBID", idx);
                    let parent = id_value(item, "Parent", idx);
                    let pid = resource_id(item, "PersistentID", idx);
                    if i < 4 {
                        let mut leaves = Vec::new();
                        collect_leaves(item, "", &mut leaves);
                        idx.objects_raw.push(leaves);
                    }
                    if let (Some(dbid), Some(parent), Some(pid)) = (dbid, parent, pid) {
                        idx.objects.push((dbid, parent, pid));
                    }
                }
            }
            "Edges" => {
                for (i, item) in items.iter().enumerate() {
                    let src = id_value(item, "SourceID", idx);
                    let dst = id_value(item, "TargetID", idx);
                    if i < 8 {
                        let mut leaves = Vec::new();
                        collect_leaves(item, "", &mut leaves);
                        idx.edges_raw.push(leaves);
                    }
                    let mut leaves = Vec::new();
                    collect_leaves(item, "", &mut leaves);
                    let ty = leaves.iter().find_map(|(n, l)| match (n.as_str(), l) {
                        (".Type", Leaf::U(x)) => Some(*x),
                        (".Type", Leaf::S(s)) => Some(s.len() as u64),
                        _ => None,
                    });
                    if let (Some(src), Some(dst)) = (src, dst) {
                        idx.edges.push((src, dst, ty.unwrap_or(u64::MAX)));
                    }
                }
            }
            other => {
                if idx.field_misses.insert(format!("(list) {other}")) {
                    eprintln!(
                        "[join-probe] DBFileIndex field {other:?}: List of {}                          (not captured)",
                        items.len()
                    );
                }
            }
        }
    }
}

fn hwm_mb() -> u64 {
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|s| {
            s.lines()
                .find(|l| l.starts_with("VmHWM:"))
                .and_then(|l| l.split_whitespace().nth(1).map(|v| v.parse::<u64>().ok()))
                .flatten()
        })
        .unwrap_or(0)
}
