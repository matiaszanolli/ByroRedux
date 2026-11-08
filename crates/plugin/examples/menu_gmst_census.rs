//! M48 Phase 0 (#6643) measurement tool for the pre-Skyrim MenuXml corpus.
//!
//! Two read-only modes over a `Misc` BSA (`Oblivion - Misc.bsa` /
//! `Fallout - Misc.bsa`):
//!
//! * `strings <Misc.bsa> <master.esm>` — GUI-FONT-005. Censuses every
//!   `&-<name>;` entity (the FO3 / FNV "dash-prefixed" string reference,
//!   e.g. `&-sContinue;`) in the `menus\` XML documents and looks the
//!   un-dashed name up among the master's `GMST` EditorIDs. TSV on stdout:
//!   `name  occurrences  documents  example_document  gmst_match
//!   gmst_exact_case  gmst_value`.
//! * `inventory <Misc.bsa>` — GUI-REG-006. One row per `menus\**` file:
//!   `path  kind  root_tag  root_name  class_entity  includes`
//!   (`kind` = `menu` when the root element is `<menu>`, `prefab` under
//!   `menus\prefabs\`, `other` for `.txt`/`.bat`/`.scc`/`.html` support
//!   files and XML that is neither).
//!
//! * `gmst <master.esm>` — every `GMST` as `EditorID<TAB>value` (GUI-PAGE-005:
//!   candidates for engine strings that have a vanilla GMST equivalent).
//!
//! Usage:
//!   cargo run -p byroredux-plugin --example menu_gmst_census -- \
//!       strings "<Data>/Fallout - Misc.bsa" "<Data>/FalloutNV.esm"
//!   cargo run -p byroredux-plugin --example menu_gmst_census -- \
//!       inventory "<Data>/Fallout - Misc.bsa"

use std::collections::BTreeMap;

use byroredux_bsa::BsaArchive;
use byroredux_plugin::esm::reader::EsmReader;

fn gmst_table(esm: &str) -> anyhow::Result<BTreeMap<String, (String, String)>> {
    let bytes = std::fs::read(esm)?;
    let mut reader = EsmReader::new(&bytes);
    let _header = reader.read_file_header()?;
    let mut out: BTreeMap<String, (String, String)> = BTreeMap::new();
    reader.for_each_record(bytes.len(), Some(b"GMST"), |_header, subs| {
        let mut edid = String::new();
        let mut data: &[u8] = &[];
        for sub in &subs {
            match &sub.sub_type {
                b"EDID" => {
                    edid = String::from_utf8_lossy(&sub.data)
                        .trim_end_matches('\0')
                        .to_owned()
                }
                b"DATA" => data = &sub.data,
                _ => {}
            }
        }
        // FO3 / FNV / Oblivion: the first EditorID letter picks the type.
        let value = match edid.chars().next() {
            Some('s') => String::from_utf8_lossy(data)
                .trim_end_matches('\0')
                .replace(['\t', '\n', '\r'], " "),
            Some('f') if data.len() >= 4 => {
                format!("{}", f32::from_le_bytes([data[0], data[1], data[2], data[3]]))
            }
            Some('i') if data.len() >= 4 => {
                format!("{}", i32::from_le_bytes([data[0], data[1], data[2], data[3]]))
            }
            _ => format!("<{} bytes>", data.len()),
        };
        out.insert(edid.to_ascii_lowercase(), (edid, value));
    })?;
    Ok(out)
}

fn menu_paths(a: &BsaArchive) -> Vec<String> {
    let mut v: Vec<String> = a
        .list_files()
        .into_iter()
        .filter(|p| p.to_ascii_lowercase().starts_with("menus\\"))
        .map(str::to_owned)
        .collect();
    v.sort();
    v
}

fn is_xml(p: &str) -> bool {
    p.to_ascii_lowercase().ends_with(".xml")
}

/// Every `&...;` token of `text` whose name starts with `-`.
fn dash_entities(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(amp) = rest.find('&') {
        rest = &rest[amp + 1..];
        if let Some(semi) = rest.find(';') {
            let name = &rest[..semi];
            if semi <= 64 && name.starts_with('-') && !name.contains(char::is_whitespace) {
                out.push(name.to_owned());
            }
        }
    }
    out
}

fn first_tag(text: &str) -> Option<(String, String)> {
    // Skip comments, <?xml?> and DOCTYPE; return (tag, name attr) of the
    // first real element.
    let mut rest = text;
    loop {
        let lt = rest.find('<')?;
        rest = &rest[lt..];
        if rest.starts_with("<!--") {
            let end = rest.find("-->")?;
            rest = &rest[end + 3..];
        } else if rest.starts_with("<?") || rest.starts_with("<!") {
            let end = rest.find('>')?;
            rest = &rest[end + 1..];
        } else {
            break;
        }
    }
    let end = rest.find('>')?;
    let head = &rest[1..end];
    let tag: String = head
        .chars()
        .take_while(|c| !c.is_whitespace() && *c != '/')
        .collect();
    let name = head
        .find("name=\"")
        .map(|i| {
            head[i + 6..]
                .chars()
                .take_while(|c| *c != '"')
                .collect::<String>()
        })
        .unwrap_or_default();
    Some((tag, name))
}

fn includes(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(i) = rest.find("<include") {
        rest = &rest[i + 8..];
        if let Some(s) = rest.find("src=\"") {
            let tail = &rest[s + 5..];
            let v: String = tail.chars().take_while(|c| *c != '"').collect();
            if !out.contains(&v) {
                out.push(v);
            }
        }
    }
    out
}

fn class_entity(text: &str) -> String {
    // The menu's own `<class>` trait is the first `<class>` in the file.
    if let Some(i) = text.find("<class>") {
        if let Some(j) = text[i + 7..].find("</class>") {
            return text[i + 7..i + 7 + j].trim().to_owned();
        }
    }
    String::new()
}

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let mode = args.get(1).map(String::as_str).unwrap_or("");
    match mode {
        "strings" => {
            let archive = BsaArchive::open(&args[2])?;
            let gmst = gmst_table(&args[3])?;
            // name -> (occurrences, docs, example)
            let mut table: BTreeMap<String, (usize, Vec<String>)> = BTreeMap::new();
            for p in menu_paths(&archive).iter().filter(|p| is_xml(p)) {
                let bytes = archive.extract(p)?;
                let text = String::from_utf8_lossy(&bytes);
                for name in dash_entities(&text) {
                    let e = table.entry(name).or_default();
                    e.0 += 1;
                    if !e.1.contains(p) {
                        e.1.push(p.clone());
                    }
                }
            }
            println!("name\toccurrences\tdocuments\texample_document\tgmst_match\tgmst_exact_case\tgmst_value");
            for (name, (n, docs)) in &table {
                let bare = &name[1..];
                let hit = gmst.get(&bare.to_ascii_lowercase());
                let (m, exact, v) = match hit {
                    Some((edid, v)) => ("yes", if edid == bare { "yes" } else { "no" }, v.as_str()),
                    None => ("no", "", ""),
                };
                println!("&{name};\t{n}\t{}\t{}\t{m}\t{exact}\t{v}", docs.len(), docs[0]);
            }
        }
        "gmst" => {
            for (_, (edid, value)) in gmst_table(&args[2])? {
                println!("{edid}\t{value}");
            }
        }
        "inventory" => {
            let archive = BsaArchive::open(&args[2])?;
            println!("path\tkind\troot_tag\troot_name\tclass_entity\tincludes");
            for p in menu_paths(&archive) {
                if !is_xml(&p) {
                    println!("{p}\tother\t\t\t\t");
                    continue;
                }
                let bytes = archive.extract(&p)?;
                let text = String::from_utf8_lossy(&bytes);
                let (tag, name) = first_tag(&text).unwrap_or_default();
                let kind = if tag == "menu" {
                    "menu"
                } else if p.to_ascii_lowercase().starts_with("menus\\prefabs\\") {
                    "prefab"
                } else {
                    "other"
                };
                let class = if tag == "menu" { class_entity(&text) } else { String::new() };
                println!("{p}\t{kind}\t{tag}\t{name}\t{class}\t{}", includes(&text).join(","));
            }
        }
        _ => anyhow::bail!("usage: menu_gmst_census strings <Misc.bsa> <master.esm> | inventory <Misc.bsa> | gmst <master.esm>"),
    }
    Ok(())
}
