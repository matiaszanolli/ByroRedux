//! Dump Starfield WTHS records and their decoded BGS reflection
//! containers (#5364) — the ground-truth probe used to pin the chunk
//! grammar before `parse_bgs_reflection` landed. Kept for the
//! instance-value follow-up (OBJT/LIST/USER): it prints the schema
//! half through the parser and the instance half as raw hex.
//!
//! Usage:
//!   cargo run -p byroredux-plugin --example dump_wths_refl -- <Starfield.esm> [substr...]

use byroredux_plugin::esm::reader::{EsmReader, RecordHeader, SubRecord};
use byroredux_plugin::esm::records::parse_bgs_reflection;

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let path = args
        .next()
        .ok_or_else(|| anyhow::anyhow!("usage: <Starfield.esm> [substr...]"))?;
    let filters: Vec<String> = args.collect();

    let bytes = std::fs::read(&path)?;
    println!("Parsing {} ({:.1} MB)\u{2026}", path, bytes.len() as f64 / 1_048_576.0);

    let mut reader = EsmReader::new(&bytes);
    let _hdr = reader.read_file_header()?;
    let mut found = 0;
    // WTHS records live in the WTHS top-level group; the bounded walk's
    // FourCC filter reaches every one of them.
    reader.for_each_record(bytes.len(), Some(b"WTHS"), |h, subs| {
        found += 1;
        let edid = subs
            .iter()
            .find(|s| s.sub_type == *b"EDID")
            .map(|s| read_zstring(&s.data))
            .unwrap_or_default();
        if filters.is_empty()
            || filters.iter().any(|f| edid.to_lowercase().contains(&f.to_lowercase()))
        {
            print_wths(h, &edid, &subs);
        }
    })?;
    println!("\ntotal WTHS records: {found}");
    Ok(())
}

fn read_zstring(bytes: &[u8]) -> String {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    String::from_utf8_lossy(&bytes[..end]).into_owned()
}

fn print_wths(h: &RecordHeader, edid: &str, subs: &[SubRecord]) {
    println!("\n== {edid} (form {:08X}) ==", h.form_id);
    for s in subs {
        println!("  {} ({} B)", String::from_utf8_lossy(&s.sub_type), s.data.len());
    }
    let Some(blob) = subs
        .iter()
        .find(|s| s.sub_type == *b"REFL")
        .or_else(|| subs.iter().find(|s| s.sub_type == *b"RDIF"))
    else {
        return;
    };
    let refl = parse_bgs_reflection(&blob.data);
    println!(
        "  reflection: version={} chunks={} classes={} strings={} \
         diff_values={} root={} unconsumed={} instance={:?} unknown={:?}",
        refl.version,
        refl.chunk_count,
        refl.classes.len(),
        refl.strings.len(),
        refl.diff_values.as_ref().map(Vec::len).map(|n| n.to_string()).as_deref().unwrap_or("-"),
        refl.root_instance
            .as_ref()
            .map(|o| format!("{} ({} fields)", o.class, o.fields.len()))
            .as_deref()
            .unwrap_or("-"),
        refl.unconsumed_instance_chunks,
        refl.instance_chunks,
        refl.unknown_chunks
    );
    for class in &refl.classes {
        println!(
            "  class {} (flags {:#06x}, {} fields)",
            class.name,
            class.flags,
            class.fields.len()
        );
        for f in &class.fields {
            println!(
                "    {} : {} (off={}, size={})",
                f.name, f.type_name, f.offset, f.size
            );
        }
    }
    // The instance half, decoded (#5627) — the value tree the
    // synthesis walks; printed compactly for grounding the field
    // mapping, with the raw hex of any chunk kept for reference.
    if let Some(root) = &refl.root_instance {
        print_value(&byroredux_plugin::esm::records::ReflectionValue::Object(
            root.clone(),
        ), 1);
    }
    if let Some(overrides) = &refl.diff_values {
        for o in overrides {
            println!(
                "  diff {} (idx {}):",
                o.field_name, o.field_index
            );
            print_value(&o.value, 2);
        }
    }
    let mut off = 0usize;
    while off + 8 <= blob.data.len() {
        let sig: [u8; 4] = blob.data[off..off + 4].try_into().unwrap();
        let size = u32::from_le_bytes(blob.data[off + 4..off + 8].try_into().unwrap()) as usize;
        if matches!(&sig, b"OBJT" | b"LIST" | b"USER" | b"USRD" | b"MAPC") {
            println!(
                "  instance chunk {} ({} B):",
                String::from_utf8_lossy(&sig),
                size
            );
            let payload = &blob.data[off + 8..(off + 8 + size).min(blob.data.len())];
            for row in payload.chunks(16) {
                let hex: Vec<String> = row.iter().map(|b| format!("{b:02x}")).collect();
                println!("    {}", hex.join(" "));
            }
        }
        off += 8 + size;
    }
}

fn print_value(v: &byroredux_plugin::esm::records::ReflectionValue, depth: usize) {
    let indent = "  ".repeat(depth + 1);
    match v {
        byroredux_plugin::esm::records::ReflectionValue::Object(o) => {
            for (name, field) in &o.fields {
                match field {
                    byroredux_plugin::esm::records::ReflectionValue::Object(_)
                    | byroredux_plugin::esm::records::ReflectionValue::List(_) => {
                        println!("{indent}{name}:");
                        print_value(field, depth + 1);
                    }
                    _ => println!("{indent}{name} = {}", summarize(field)),
                }
            }
        }
        byroredux_plugin::esm::records::ReflectionValue::List(items) => {
            for (i, item) in items.iter().enumerate() {
                println!("{indent}[{i}] {}", summarize(item));
            }
        }
        _ => println!("{indent}{}", summarize(v)),
    }
}

fn summarize(v: &byroredux_plugin::esm::records::ReflectionValue) -> String {
    use byroredux_plugin::esm::records::ReflectionValue as V;
    match v {
        V::Null => "null".into(),
        V::Bool(b) => format!("{b}"),
        V::I8(x) => format!("{x} i8"),
        V::U8(x) => format!("{x} u8"),
        V::I16(x) => format!("{x} i16"),
        V::U16(x) => format!("{x} u16"),
        V::I32(x) => format!("{x:#x} i32"),
        V::U32(x) => format!("{x:#010x} u32"),
        V::I64(x) => format!("{x:#x} i64"),
        V::U64(x) => format!("{x:#018x} u64"),
        V::F32(x) => format!("{x} f32"),
        V::F64(x) => format!("{x} f64"),
        V::String(s) => format!("{s:?}"),
        V::Ref { target_type, value } => {
            format!("ref({target_type}) = {}", summarize(value))
        }
        V::List(items) => format!("[{} items]", items.len()),
        V::Map(pairs) => format!("{{{} pairs}}", pairs.len()),
        V::Object(o) => format!("{} {{ {} fields }}", o.class, o.fields.len()),
    }
}
