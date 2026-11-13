//! Census every music source in a load order — the numbers behind the Game Music PRD
//! (`docs/prd/game-music.md` §3.4, #6865): cell `XCMT` / `XCMO`, worldspace `SNAM` / `ZNAM`,
//! region `RDMD` / `RDMO` / `RDSB` / `RDSI`, the `DOBJ` music slots, `MUSC`, `MSET`, `ALOC` and
//! New Vegas audio-marker `REFR`s, plus the files under `Data\Music` and whether each music type
//! and media-set track resolves to one.
//!
//! Pass the base master first and its DLC after it: FormIDs resolve through each file's own master
//! list, so a DLC cell naming a base-game music type prints the type's EditorID. The `Data`
//! folder is the first plugin's directory.
//!
//! Usage:
//!   cargo run --release -p byroredux-plugin --example music_census -- <plugin>... [--markers <ALOC substring>]
//!
//! `--markers` lists every audio marker whose controller's EditorID contains the substring, with
//! its FormID and position (PRD §7.2 scenario 6 names one).

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use byroredux_plugin::esm::reader::{EsmReader, GameKind};
use byroredux_plugin::esm::records::{
    parse_mset, parse_musc, MediaSetKind, MediaSetRecord, MuscRecord,
};

/// A FormID made load-order independent: the lower-cased name of the file that owns it, and its
/// low 24 bits.
type Key = (String, u32);

/// The Fallout 3 / New Vegas `DOBJ` `DATA` indices of the five music objects (xEdit
/// `wbDefinitionsFO3.pas:5672-5676`).
const DOBJ_MUSIC: [(&str, usize); 5] = [
    ("Default", 9),
    ("Battle", 10),
    ("Death", 11),
    ("Success", 12),
    ("Level Up", 13),
];

/// Oblivion's music category enum on `XCMT`, `SNAM` and `RDMD` (xEdit `wbDefinitionsCommon.pas:7976-7983`).
fn category(value: u32) -> &'static str {
    match value {
        0 => "Default",
        1 => "Public",
        2 => "Dungeon",
        _ => "?",
    }
}

const PLAYABLE: [&str; 4] = ["mp3", "wav", "ogg", "flac"];

#[derive(Default)]
struct Census {
    file: String,
    game: Option<GameKind>,
    cells: usize,
    xcmt: BTreeMap<u32, usize>,
    xcmo: BTreeMap<Key, usize>,
    worldspaces: Vec<(Key, Option<u32>, Option<Key>)>,
    rdmd: BTreeMap<u32, usize>,
    region_forms: BTreeMap<&'static str, BTreeMap<Key, usize>>,
    dobj_music: Option<Vec<Key>>,
    musc: Vec<(Key, MuscRecord)>,
    mset: Vec<(Key, MediaSetRecord)>,
    aloc: Vec<Key>,
    markers: Vec<(Key, Key, [f32; 3])>,
}

fn u32_at(data: &[u8], at: usize) -> Option<u32> {
    data.get(at..at + 4)
        .map(|b| u32::from_le_bytes(b.try_into().unwrap()))
}

/// The child of `dir` named `name`, matched case-insensitively.
fn child_ci(dir: &Path, name: &str) -> Option<PathBuf> {
    std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .find(|p| {
            p.file_name()
                .is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case(name))
        })
}

/// `rel` (either separator) under `base`, every component matched case-insensitively.
fn resolve_ci(base: &Path, rel: &str) -> Option<PathBuf> {
    rel.split(['\\', '/'])
        .filter(|c| !c.is_empty())
        .try_fold(base.to_path_buf(), |dir, c| child_ci(&dir, c))
}

fn playable_files(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let path = entry.path();
        if path.is_dir() {
            out.extend(playable_files(&path));
        } else if path.extension().is_some_and(|e| {
            PLAYABLE
                .iter()
                .any(|p| e.to_string_lossy().eq_ignore_ascii_case(p))
        }) {
            out.push(path);
        }
    }
    out
}

/// What a music path resolves to under `Data\Music`: a file, a folder of playable files (folder
/// form, PRD MUS-SRC-010), nothing (Missing), or no path at all (Silent).
fn outcome(music: Option<&Path>, path: &str) -> &'static str {
    if path.is_empty() {
        return "silent";
    }
    let Some(found) = music.and_then(|m| resolve_ci(m, path)) else {
        return "missing";
    };
    let folder_form = path.ends_with(['\\', '/']);
    match (folder_form, found.is_dir()) {
        (true, true) if !playable_files(&found).is_empty() => "folder",
        (false, false) => "file",
        _ => "missing",
    }
}

fn main() -> anyhow::Result<()> {
    let mut plugins = Vec::new();
    let mut marker_filter = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--markers" {
            marker_filter = args.next().map(|s| s.to_ascii_lowercase());
        } else {
            plugins.push(PathBuf::from(arg));
        }
    }
    anyhow::ensure!(
        !plugins.is_empty(),
        "usage: music_census <plugin>... [--markers <ALOC substring>]"
    );

    let mut edids: HashMap<Key, String> = HashMap::new();
    let mut censuses = Vec::new();
    for path in &plugins {
        let bytes = std::fs::read(path)?;
        let file = path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .to_ascii_lowercase();
        let mut reader = EsmReader::new(&bytes);
        let variant = reader.variant();
        let header = reader.read_file_header()?;
        let game = GameKind::from_header(variant, header.hedr_version, header.record_version);
        let masters: Vec<String> = header
            .master_files
            .iter()
            .map(|m| m.to_ascii_lowercase())
            .collect();
        let key = |fid: u32| -> Key {
            let owner = masters
                .get((fid >> 24) as usize)
                .cloned()
                .unwrap_or_else(|| file.clone());
            (owner, fid & 0x00FF_FFFF)
        };
        let mut c = Census {
            file: file.clone(),
            game: Some(game),
            ..Default::default()
        };
        reader.for_each_record(bytes.len(), None, |h, subs| {
            let me = key(h.form_id);
            let field = |t: &[u8; 4]| {
                subs.iter()
                    .find(|s| &s.sub_type == t)
                    .map(|s| s.data.as_slice())
            };
            if let Some(edid) = field(b"EDID") {
                let edid =
                    String::from_utf8_lossy(edid.split(|b| *b == 0).next().unwrap_or_default());
                edids.insert(me.clone(), edid.into_owned());
            }
            match &h.record_type {
                b"CELL" => {
                    c.cells += 1;
                    if let Some(&value) = field(b"XCMT").and_then(|d| d.first()) {
                        *c.xcmt.entry(u32::from(value)).or_default() += 1;
                    }
                    if let Some(form) = field(b"XCMO").and_then(|d| u32_at(d, 0)) {
                        *c.xcmo.entry(key(form)).or_default() += 1;
                    }
                }
                b"WRLD" => {
                    let snam = field(b"SNAM")
                        .filter(|d| d.len() == 4)
                        .and_then(|d| u32_at(d, 0));
                    let znam = field(b"ZNAM").and_then(|d| u32_at(d, 0)).map(key);
                    if snam.is_some() || znam.is_some() {
                        c.worldspaces
                            .push((me, snam.filter(|_| game == GameKind::Oblivion), znam));
                    }
                }
                b"REGN" => {
                    for sub in &subs {
                        let Some(value) = u32_at(&sub.data, 0) else {
                            continue;
                        };
                        let kind = match &sub.sub_type {
                            b"RDMD" => {
                                *c.rdmd.entry(value).or_default() += 1;
                                continue;
                            }
                            b"RDMO" => "RDMO",
                            b"RDSB" => "RDSB",
                            b"RDSI" => "RDSI",
                            _ => continue,
                        };
                        *c.region_forms
                            .entry(kind)
                            .or_default()
                            .entry(key(value))
                            .or_default() += 1;
                    }
                }
                b"DOBJ" if game != GameKind::Oblivion => {
                    if let Some(data) = field(b"DATA") {
                        let slots = DOBJ_MUSIC
                            .iter()
                            .filter_map(|(_, i)| u32_at(data, i * 4))
                            .map(key)
                            .collect();
                        c.dobj_music = Some(slots);
                    }
                }
                b"MUSC" => c.musc.push((me, parse_musc(h.form_id, &subs, game, &None))),
                b"MSET" => c.mset.push((me, parse_mset(h.form_id, &subs, &None))),
                b"ALOC" => c.aloc.push(me),
                b"REFR" if field(b"MMRK").is_some() => {
                    // The marker's controller is the `CNAM` that follows `MMRK` (xEdit
                    // `wbDefinitionsFNV.pas:7616-7623`).
                    let aloc = subs
                        .iter()
                        .skip_while(|s| &s.sub_type != b"MMRK")
                        .find(|s| &s.sub_type == b"CNAM")
                        .and_then(|s| u32_at(&s.data, 0))
                        .map(key);
                    let pos = field(b"DATA").filter(|d| d.len() >= 12).map(|d| {
                        let f =
                            |i: usize| f32::from_le_bytes(d[i * 4..i * 4 + 4].try_into().unwrap());
                        [f(0), f(1), f(2)]
                    });
                    if let (Some(aloc), Some(pos)) = (aloc, pos) {
                        c.markers.push((me, aloc, pos));
                    }
                }
                _ => {}
            }
        })?;
        censuses.push(c);
    }

    let name = |k: &Key| {
        edids
            .get(k)
            .cloned()
            .unwrap_or_else(|| format!("{}:{:06X}", k.0, k.1))
    };
    let data = plugins[0].parent().unwrap_or(Path::new("."));
    let music = child_ci(data, "Music");

    println!("=== Data\\Music under {}", data.display());
    match &music {
        None => println!("  no Music folder"),
        Some(dir) => {
            let files = playable_files(dir);
            let mut by_folder: BTreeMap<String, usize> = BTreeMap::new();
            for f in &files {
                let rel = f.strip_prefix(dir).unwrap();
                let top = rel
                    .components()
                    .next()
                    .filter(|_| rel.components().count() > 1);
                let top = top.map_or("(root)".to_string(), |c| {
                    c.as_os_str().to_string_lossy().to_ascii_lowercase()
                });
                *by_folder.entry(top).or_default() += 1;
            }
            println!("  {} playable files: {by_folder:?}", files.len());
        }
    }

    for c in &censuses {
        println!("\n=== {} ({:?})", c.file, c.game.unwrap());
        let xcmt: BTreeMap<_, _> = c.xcmt.iter().map(|(v, n)| (category(*v), *n)).collect();
        let xcmo: BTreeMap<_, _> = c.xcmo.iter().map(|(k, n)| (name(k), *n)).collect();
        println!(
            "  CELL {}: XCMT {} {xcmt:?}",
            c.cells,
            c.xcmt.values().sum::<usize>()
        );
        println!("    XCMO {} {xcmo:?}", c.xcmo.values().sum::<usize>());
        let worlds: Vec<String> = c
            .worldspaces
            .iter()
            .map(|(w, snam, znam)| match (snam, znam) {
                (Some(s), _) => format!("{}={}", name(w), category(*s)),
                (_, Some(z)) => format!("{}={}", name(w), name(z)),
                _ => unreachable!(),
            })
            .collect();
        println!("  WRLD with music {}: {worlds:?}", worlds.len());
        let rdmd: BTreeMap<_, _> = c.rdmd.iter().map(|(v, n)| (category(*v), *n)).collect();
        println!("  REGN RDMD {} {rdmd:?}", c.rdmd.values().sum::<usize>());
        for (kind, forms) in &c.region_forms {
            let forms: BTreeMap<_, _> = forms.iter().map(|(k, n)| (name(k), *n)).collect();
            println!("    {kind} {} {forms:?}", forms.values().sum::<usize>());
        }
        if let Some(slots) = &c.dobj_music {
            let slots: Vec<String> = DOBJ_MUSIC
                .iter()
                .zip(slots)
                .map(|((label, _), k)| format!("{label}={}", name(k)))
                .collect();
            println!("  DOBJ music: {slots:?}");
        }
        if !c.musc.is_empty() {
            let mut outcomes: BTreeMap<&str, usize> = BTreeMap::new();
            println!("  MUSC {}:", c.musc.len());
            for (k, m) in &c.musc {
                let o = outcome(music.as_deref(), &m.file);
                *outcomes.entry(o).or_default() += 1;
                println!(
                    "    {:<28} {:<8} ANAM {:>6.1}  {:?}",
                    name(k),
                    o,
                    m.decibels,
                    m.file
                );
            }
            println!("    outcomes {outcomes:?}");
        }
        if !c.mset.is_empty() {
            let mut kinds: BTreeMap<String, usize> = BTreeMap::new();
            let mut outcomes: BTreeMap<&str, usize> = BTreeMap::new();
            for (_, m) in &c.mset {
                *kinds.entry(format!("{:?}", m.kind)).or_default() += 1;
                if m.kind == MediaSetKind::Incidental {
                    continue; // its slots are SOUN-driven, not paths (PRD MUS-NG-004)
                }
                for t in m.tracks.iter().filter(|t| !t.is_empty()) {
                    *outcomes.entry(outcome(music.as_deref(), t)).or_default() += 1;
                }
            }
            println!(
                "  MSET {} {kinds:?}; non-incidental track paths {outcomes:?}",
                c.mset.len()
            );
        }
        if !c.aloc.is_empty() || !c.markers.is_empty() {
            let mut per_aloc: BTreeMap<&Key, usize> = BTreeMap::new();
            for (_, aloc, _) in &c.markers {
                *per_aloc.entry(aloc).or_default() += 1;
            }
            println!(
                "  ALOC {}; audio markers {} pointing at {} controllers",
                c.aloc.len(),
                c.markers.len(),
                per_aloc.len()
            );
        }
        if let Some(filter) = &marker_filter {
            for (marker, aloc, pos) in &c.markers {
                let controller = name(aloc);
                if controller.to_ascii_lowercase().contains(filter) {
                    println!(
                        "    marker {:<28} {}:{:06X}  ALOC {controller}  at ({:.0}, {:.0}, {:.0})",
                        name(marker),
                        marker.0,
                        marker.1,
                        pos[0],
                        pos[1],
                        pos[2]
                    );
                }
            }
        }
    }
    Ok(())
}
