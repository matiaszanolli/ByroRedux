//! #3398 Phase-2 RE spike: answer "can we look a `.mat` path up in the CDB,
//! and what fields does the resulting material carry?" — the two questions
//! that block writing the extraction consumer.
//!
//! Prints, in one parse:
//!   1. STRT `.mat` string inventory (are paths stored literally?)
//!   2. `CompiledDB.HashMap` shape + whether `BSResource::ID` is derivable
//!      from a path by the candidate hash functions
//!   3. `DBFileIndex::ObjectInfo` layout (what keys an object)
//!   4. A fully-expanded material object: every component + field + value

use byroredux_bsa::Ba2Archive;
use byroredux_sfmaterial::{ComponentDatabaseFile, Value};
use std::collections::BTreeMap;

fn as_u32(v: &Value) -> Option<u32> {
    match v {
        Value::U32(x) => Some(*x),
        _ => None,
    }
}
fn as_u64(v: &Value) -> Option<u64> {
    match v {
        Value::U64(x) => Some(*x),
        _ => None,
    }
}
#[allow(dead_code)]
fn as_str(v: &Value) -> Option<&str> {
    match v {
        Value::String(s) => Some(s),
        _ => None,
    }
}
fn obj<'a>(v: &'a Value, class: &str) -> Option<&'a BTreeMap<String, Value>> {
    match v {
        Value::Object(o) if o.class_name == class => Some(&o.fields),
        _ => None,
    }
}

/// Compact one-line rendering for leaf-ish values.
fn brief(v: &Value, d: usize) -> String {
    let pad = "  ".repeat(d);
    match v {
        Value::Object(o) => {
            let mut s = format!("{pad}<{}>\n", o.class_name);
            for (k, fv) in &o.fields {
                s += &format!("{pad}  .{k} = {}", brief(fv, d + 2).trim_start_matches(' '));
            }
            s
        }
        Value::List(items) => {
            let mut s = format!("{pad}[{}]\n", items.len());
            for i in items.iter().take(6) {
                s += &brief(i, d + 1);
            }
            s
        }
        Value::Map(p) => {
            let mut s = format!("{pad}{{{}}}\n", p.len());
            for (k, vv) in p.iter().take(4) {
                s += &brief(k, d + 1);
                s += &brief(vv, d + 1);
            }
            s
        }
        Value::Ref(r) => format!(
            "{pad}Ref(type {})\n{}",
            r.type_ref.id,
            brief(&r.inner, d + 1)
        ),
        other => format!("{pad}{other:?}\n"),
    }
}

fn main() {
    let ba2_path = std::env::args().nth(1).unwrap_or_else(|| {
        "/mnt/data/SteamLibrary/steamapps/common/Starfield/Data/Starfield - Materials.ba2".into()
    });
    let inner = std::env::args()
        .nth(2)
        .unwrap_or_else(|| "materials\\materialsbeta.cdb".into());
    let ba2 = Ba2Archive::open(&ba2_path).unwrap();
    let bytes = ba2.extract(&inner).unwrap();
    let cdb = ComponentDatabaseFile::parse(&bytes).unwrap();
    println!(
        "## parsed: {} classes, {} instances\n",
        cdb.classes.len(),
        cdb.instances.len()
    );

    // ---- 0. class field layout: declaration order vs `offset` order ----
    // `read_user_class` reads values sequentially in declaration order and
    // ignores `Field::offset`. If the two orders disagree for any class,
    // every field name in that class is bound to the wrong value.
    println!("## 0. declaration order vs offset order");
    let mut disagree = 0usize;
    for c in &cdb.classes {
        let offs: Vec<u16> = c.fields.iter().map(|f| f.offset).collect();
        let sorted = {
            let mut v = offs.clone();
            v.sort_unstable();
            v
        };
        if offs != sorted {
            disagree += 1;
            if disagree <= 8 {
                println!("     MISMATCH {} ({} fields)", c.name, c.fields.len());
                for f in &c.fields {
                    println!(
                        "        decl .{:<24} offset={:<4} size={}",
                        f.name, f.offset, f.size
                    );
                }
            }
        }
    }
    println!(
        "     classes whose declaration order != offset order: {disagree}/{}",
        cdb.classes.len()
    );
    for c in cdb.classes.iter().filter(|c| c.name == "BSResource::ID") {
        println!("     BSResource::ID layout:");
        for f in &c.fields {
            println!(
                "        decl .{:<8} offset={:<4} size={}",
                f.name, f.offset, f.size
            );
        }
    }
    println!();

    // ---- 1. STRT `.mat` inventory -------------------------------------
    let raw = cdb.strings.raw();
    let s = String::from_utf8_lossy(raw);
    let mats: Vec<&str> = s
        .split('\0')
        .filter(|x| x.to_lowercase().ends_with(".mat"))
        .collect();
    println!(
        "## 1. STRT: {} bytes, {} strings ending `.mat`",
        raw.len(),
        mats.len()
    );
    for m in mats.iter().take(15) {
        println!("     {m}");
    }
    println!();

    // ---- 2. CompiledDB.HashMap ----------------------------------------
    let mut hashmap_pairs: Vec<(u32, u32, u32, u64)> = Vec::new();
    for inst in &cdb.instances {
        let Some(f) = obj(inst, "BSMaterial::Internal::CompiledDB") else {
            continue;
        };
        println!(
            "## 2. CompiledDB fields: {:?}",
            f.keys().collect::<Vec<_>>()
        );
        if let Some(Value::Map(pairs)) = f.get("HashMap") {
            println!("     HashMap entries: {}", pairs.len());
            for (k, v) in pairs.iter() {
                let Some(id) = obj(k, "BSResource::ID") else {
                    continue;
                };
                let (Some(dir), Some(ext), Some(file)) = (
                    id.get("Dir").and_then(as_u32),
                    id.get("Ext").and_then(as_u32),
                    id.get("File").and_then(as_u32),
                ) else {
                    continue;
                };
                if let Some(val) = as_u64(v) {
                    hashmap_pairs.push((dir, ext, file, val));
                }
            }
        }
        for other in ["Circular", "Collisions"] {
            if let Some(v) = f.get(other) {
                println!("     {other} = {}", brief(v, 0).trim_end());
            }
        }
        break;
    }
    println!(
        "     decoded {} (Dir,Ext,File)->u64 pairs",
        hashmap_pairs.len()
    );
    // Is `Ext` (or `File`) constant? A constant column is the extension hash.
    for (label, idx) in [("Dir", 0usize), ("Ext", 1), ("File", 2)] {
        let mut distinct = std::collections::HashSet::new();
        for p in &hashmap_pairs {
            distinct.insert(match idx {
                0 => p.0,
                1 => p.1,
                _ => p.2,
            });
        }
        println!(
            "     column {label}: {} distinct value(s){}",
            distinct.len(),
            if distinct.len() == 1 {
                format!(
                    "  == {:#010x} (constant → this is the extension hash)",
                    distinct.iter().next().unwrap()
                )
            } else {
                String::new()
            }
        );
    }
    for p in hashmap_pairs.iter().take(5) {
        println!(
            "     dir={:#010x} ext={:#010x} file={:#010x} -> {:#018x}",
            p.0, p.1, p.2, p.3
        );
    }
    println!();

    // ---- 3. ObjectInfo layout ------------------------------------------
    println!("## 3. DBFileIndex::ObjectInfo");
    'idx: for inst in &cdb.instances {
        let Some(f) = obj(inst, "BSComponentDB2::DBFileIndex") else {
            continue;
        };
        println!(
            "     DBFileIndex fields: {:?}",
            f.keys().collect::<Vec<_>>()
        );
        if let Some(Value::List(objs)) = f.get("Objects") {
            println!("     Objects: {}", objs.len());
            for o in objs.iter().take(4) {
                print!("{}", brief(o, 2));
            }
        }
        break 'idx;
    }
    println!();

    // ---- 4. A fully-expanded material object ---------------------------
    // Group every component instance by the class it belongs to, then print
    // one complete example of each material-relevant class.
    println!("## 4. one complete example per BSMaterial::* class");
    let mut seen: BTreeMap<String, &Value> = BTreeMap::new();
    fn collect<'a>(v: &'a Value, seen: &mut BTreeMap<String, &'a Value>, d: usize) {
        if d > 4 {
            return;
        }
        match v {
            Value::Object(o) => {
                if o.class_name.starts_with("BSMaterial::") && !seen.contains_key(&o.class_name) {
                    // Prefer an example with at least one non-default-looking field.
                    seen.insert(o.class_name.clone(), v);
                }
                for (_k, fv) in &o.fields {
                    collect(fv, seen, d + 1);
                }
            }
            Value::List(items) => {
                for i in items {
                    collect(i, seen, d + 1)
                }
            }
            Value::Map(p) => {
                for (k, vv) in p {
                    collect(k, seen, d + 1);
                    collect(vv, seen, d + 1)
                }
            }
            Value::Ref(r) => collect(&r.inner, seen, d + 1),
            _ => {}
        }
    }
    for inst in &cdb.instances {
        collect(inst, &mut seen, 0);
    }
    let want = [
        "BSMaterial::MRTextureFile",
        "BSMaterial::TextureFile",
        "BSMaterial::TextureSetID",
        "BSMaterial::MaterialID",
        "BSMaterial::LayerID",
        "BSMaterial::BlenderID",
        "BSMaterial::MaterialParamFloat",
        "BSMaterial::Color",
        "BSMaterial::ParamBool",
        "BSMaterial::AlphaSettingsComponent",
        "BSMaterial::AlphaBlenderSettings",
        "BSMaterial::ShaderRouteComponent",
        "BSMaterial::ShaderModelComponent",
        "BSMaterial::EmissiveSettingsComponent",
        "BSMaterial::OpacityComponent",
        "BSMaterial::DecalSettingsComponent",
        "BSMaterial::TextureReplacement",
        "BSMaterial::TranslucencySettings",
        "BSMaterial::FlowSettingsComponent",
        "BSMaterial::TextureAddressModeComponent",
        "BSMaterial::TextureResolutionSetting",
        "BSMaterial::MaterialOverrideColorTypeComponent",
        "BSMaterial::Offset",
        "BSMaterial::Scale",
        "BSMaterial::UVStreamID",
        "BSMaterial::UVStreamParamBool",
        "BSMaterial::Channel",
        "BSMaterial::LevelOfDetailSettings",
        "BSMaterial::EffectSettingsComponent",
        "BSMaterial::DetailBlenderSettings",
        "BSMaterial::CollisionComponent",
    ];
    for w in want {
        match seen.get(w) {
            Some(v) => print!("{}", brief(v, 1)),
            None => println!("  <{w}>  (no instance reached at depth<=4)"),
        }
    }
    println!(
        "\n     total distinct BSMaterial::* classes reached: {}",
        seen.len()
    );
}
