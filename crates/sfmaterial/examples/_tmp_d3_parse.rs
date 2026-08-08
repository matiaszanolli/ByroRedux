use byroredux_bsa::Ba2Archive;
use byroredux_sfmaterial::{ComponentDatabaseFile, Value};
use std::collections::BTreeMap;

fn walk(v: &Value, counts: &mut BTreeMap<String, usize>, depth: usize, maxd: usize) {
    match v {
        Value::Object(o) => {
            *counts.entry(o.class_name.clone()).or_default() += 1;
            if depth < maxd { for (_k, fv) in &o.fields { walk(fv, counts, depth+1, maxd); } }
        }
        Value::List(items) => { if depth < maxd { for i in items { walk(i, counts, depth+1, maxd); } } }
        Value::Map(p) => { if depth < maxd { for (k,vv) in p { walk(k, counts, depth+1, maxd); walk(vv, counts, depth+1, maxd); } } }
        Value::Ref(r) => { if depth < maxd { walk(&r.inner, counts, depth+1, maxd); } }
        _ => {}
    }
}

fn main() {
    let arg = std::env::args().nth(1).unwrap_or_else(|| "/mnt/data/SteamLibrary/steamapps/common/Starfield/Data/Starfield - Materials.ba2".into());
    let inner = std::env::args().nth(2).unwrap_or_else(|| "materials\\materialsbeta.cdb".into());
    let ba2 = Ba2Archive::open(&arg).unwrap();
    let bytes = ba2.extract(&inner).unwrap();
    eprintln!("cdb bytes = {}", bytes.len());
    let t0 = std::time::Instant::now();
    let cdb = match ComponentDatabaseFile::parse(&bytes) { Ok(c)=>c, Err(e)=>{ println!("PARSE ERROR: {e}"); return } };
    eprintln!("parsed in {:?}: {} classes, {} top-level instances", t0.elapsed(), cdb.classes.len(), cdb.instances.len());

    // duplicate name_offset check
    let mut byoff: BTreeMap<i32, Vec<&str>> = BTreeMap::new();
    for c in &cdb.classes { byoff.entry(c.name_offset).or_default().push(&c.name); }
    let dups: Vec<_> = byoff.iter().filter(|(_,v)| v.len()>1).collect();
    eprintln!("duplicate name_offsets: {}", dups.len());
    for (o,v) in dups.iter().take(10) { eprintln!("   off {o} -> {v:?}"); }

    // duplicate type_id check
    let mut bytid: BTreeMap<u32, Vec<&str>> = BTreeMap::new();
    for c in &cdb.classes { bytid.entry(c.type_id).or_default().push(&c.name); }
    let dt: Vec<_> = bytid.iter().filter(|(_,v)| v.len()>1).collect();
    eprintln!("duplicate type_ids: {}", dt.len());

    let mut counts = BTreeMap::new();
    for i in &cdb.instances { walk(i, &mut counts, 0, 3); }
    let mut v: Vec<_> = counts.into_iter().collect();
    v.sort_by_key(|(_,c)| std::cmp::Reverse(*c));
    eprintln!("--- top object class counts (depth<=3)");
    for (n,c) in v.iter().take(40) { eprintln!("  {c:>8}  {n}"); }

    eprintln!("--- class names containing 'Material' or 'BSMaterial'");
    for c in cdb.classes.iter().filter(|c| c.name.to_lowercase().contains("material")).take(60) {
        eprintln!("  {} (type_id {:#x}, {} fields, flags {:?})", c.name, c.type_id, c.fields.len(), c.flags);
    }
}
