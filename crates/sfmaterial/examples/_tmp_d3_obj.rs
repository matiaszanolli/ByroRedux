use byroredux_bsa::Ba2Archive;
use byroredux_sfmaterial::{ComponentDatabaseFile, Value};

fn show(v: &Value, d: usize) -> String {
    let pad = "  ".repeat(d);
    match v {
        Value::Object(o) => {
            let mut s = format!("{}<{}>\n", pad, o.class_name);
            for (k, fv) in &o.fields { s += &format!("{}  .{} =\n{}", pad, k, show(fv, d+2)); }
            s
        }
        Value::List(items) => { let mut s=format!("{}[{}]\n", pad, items.len()); for i in items.iter().take(4) { s += &show(i, d+1); } s }
        Value::Map(p) => { let mut s=format!("{}{{{}}}\n", pad, p.len()); for (k,vv) in p.iter().take(3) { s += &show(k,d+1); s += &show(vv,d+1); } s }
        Value::Ref(r) => format!("{}Ref(type {})\n{}", pad, r.type_ref.id, show(&r.inner, d+1)),
        other => format!("{}{:?}\n", pad, other),
    }
}

fn main() {
    let arg = std::env::args().nth(1).unwrap_or_else(|| "/mnt/data/SteamLibrary/steamapps/common/Starfield/Data/Starfield - Materials.ba2".into());
    let inner = std::env::args().nth(2).unwrap_or_else(|| "materials\\materialsbeta.cdb".into());
    let ba2 = Ba2Archive::open(&arg).unwrap();
    let bytes = ba2.extract(&inner).unwrap();
    let cdb = ComponentDatabaseFile::parse(&bytes).unwrap();
    println!("instances: {}", cdb.instances.len());
    for (i, inst) in cdb.instances.iter().enumerate().take(3) {
        println!("=== instance {i}\n{}", show(inst, 0).lines().take(80).collect::<Vec<_>>().join("\n"));
    }
    // Search the STRT table for a .mat path
    let raw = cdb.strings.raw();
    let s = String::from_utf8_lossy(raw);
    let mats: Vec<&str> = s.split('\0').filter(|x| x.to_lowercase().ends_with(".mat")).collect();
    println!("STRT entries ending .mat: {} (total strt bytes {})", mats.len(), raw.len());
    for m in mats.iter().take(10) { println!("   {m}"); }
}
