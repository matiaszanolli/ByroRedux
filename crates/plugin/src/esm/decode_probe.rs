//! Which sub-records the parsers decode, found by asking the parsers (PRD legacy-mod-compatibility LMC-REC-010).
//!
//! No parser records what it reads: each is a `match` over the sub-record tags it cares about, with a wildcard arm for
//! the rest. So "decoded" is measured instead. For one record type the probe picks real records from a plugin so that
//! every sub-record tag in them appears with up to [`ProbeLimits::samples_per_tag`] different payloads, rebuilds a
//! small plugin of just those records (with the group structure they need), and parses it for a baseline. Then, for each
//! tag, it removes that sub-record from every record of the type, parses again, and compares the two indexes with
//! [`structural_digest`]. A tag whose removal changes the index (or makes the parse fail) is decoded; one whose removal
//! changes nothing never reached the index.
//!
//! A tag is judged on the payloads sampled, so one that a parser reads only for a rare value could be called undecoded.
//! The error is in the safe direction for what uses the table ([`super::records::opaque`]): such a sub-record is kept as
//! opaque bytes as well, never dropped. The table is regenerated, not edited, by `examples/inspect.rs decoded-subrecords`.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::hash::{Hash, Hasher};

use anyhow::{ensure, Context, Result};

use super::opaque::encode_tag;
use super::raw_record::top_groups_of;
use super::reader::{EsmReader, EsmVariant, FLAG_COMPRESSED};
use super::records::parse_esm_with_load_order;
use super::structural_digest::structural_digest;

/// The record flag of a deleted record: such a record's body is a stub, so it says nothing about what a parser reads.
const FLAG_DELETED: u32 = 0x20;

/// Per record type, the sub-record tags its parsers decode.
pub type Decoded = BTreeMap<[u8; 4], BTreeSet<[u8; 4]>>;

/// How much of a plugin the probe samples per record type.
#[derive(Debug, Clone, Copy)]
pub struct ProbeLimits {
    /// Distinct payloads sampled per sub-record tag.
    pub samples_per_tag: usize,
    /// The most records of one type the rebuilt plugin holds.
    pub max_records_per_type: usize,
}

impl Default for ProbeLimits {
    fn default() -> Self {
        Self { samples_per_tag: 6, max_records_per_type: 300 }
    }
}

/// What the probe found in one plugin.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Derivation {
    /// Per record type probed, the sub-record tags whose removal changed the parsed index. A type whose list is empty
    /// has no decoder for anything the sampled records carry.
    pub decoded: Decoded,
    /// Record types the probe could not judge, with why (the baseline plugin did not parse).
    pub skipped: Vec<([u8; 4], String)>,
}

/// The header sizes of the file's variant.
#[derive(Clone, Copy)]
pub(crate) struct Layout {
    pub(crate) variant: EsmVariant,
    pub(crate) record: usize,
    pub(crate) group: usize,
}

/// A record's header fields.
pub(crate) struct Head {
    pub(crate) record_type: [u8; 4],
    pub(crate) data_size: usize,
    pub(crate) flags: u32,
    pub(crate) form_id: u32,
}

fn u32_at(data: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(data[at..at + 4].try_into().expect("four bytes"))
}

fn head_at(data: &[u8], at: usize) -> Head {
    Head {
        record_type: data[at..at + 4].try_into().expect("four bytes"),
        data_size: u32_at(data, at + 4) as usize,
        flags: u32_at(data, at + 8),
        form_id: u32_at(data, at + 12),
    }
}

/// A group header's `(total size, label, group type)`.
fn group_at(data: &[u8], at: usize) -> (usize, [u8; 4], u32) {
    (u32_at(data, at + 4) as usize, data[at + 8..at + 12].try_into().expect("four bytes"), u32_at(data, at + 12))
}

pub(crate) fn layout_of(data: &[u8]) -> Result<Layout> {
    ensure!(data.len() >= 24 && &data[..4] == b"TES4", "not a plugin: no TES4 record");
    let variant = EsmVariant::detect(&data[..24]);
    Ok(Layout { variant, record: variant.record_header_size(), group: variant.group_header_size() })
}

/// A record's sub-records, decompressed, as `(tag, bytes)`.
pub(crate) fn sub_records(record: &[u8], layout: Layout) -> Result<Vec<([u8; 4], Vec<u8>)>> {
    let mut reader = EsmReader::with_variant(record, layout.variant);
    let header = reader.read_record_header()?;
    Ok(reader.read_sub_records(&header)?.into_iter().map(|sub| (sub.sub_type, sub.data)).collect())
}

fn payload_hash(data: &[u8]) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    data.hash(&mut hasher);
    hasher.finish()
}

/// What the sampling pass keeps of one record type.
#[derive(Default)]
struct Sample {
    /// Per tag, the payloads sampled so far.
    seen: HashMap<[u8; 4], HashSet<u64>>,
    /// The file offsets of the records chosen.
    chosen: BTreeSet<usize>,
}

/// Visit every record of the plugin in file order, descending into groups.
pub(crate) fn for_each_record(data: &[u8], layout: Layout, mut visit: impl FnMut(usize, &Head)) {
    let head = head_at(data, 0);
    let mut at = layout.record + head.data_size;
    while at + layout.group <= data.len() {
        if &data[at..at + 4] == b"GRUP" {
            at += layout.group;
            continue;
        }
        let head = head_at(data, at);
        if at + layout.record + head.data_size > data.len() {
            break;
        }
        visit(at, &head);
        at += layout.record + head.data_size;
    }
}

/// Pick the records of each type that, together, show every sub-record tag with several different payloads.
fn sample(data: &[u8], layout: Layout, limits: &ProbeLimits) -> BTreeMap<[u8; 4], Sample> {
    let mut samples: BTreeMap<[u8; 4], Sample> = BTreeMap::new();
    for_each_record(data, layout, |at, head| {
        if head.flags & FLAG_DELETED != 0 {
            return;
        }
        let Ok(subs) = sub_records(&data[at..at + layout.record + head.data_size], layout) else {
            return;
        };
        let sample = samples.entry(head.record_type).or_default();
        let hashes: Vec<([u8; 4], u64)> = subs.iter().map(|(tag, bytes)| (*tag, payload_hash(bytes))).collect();
        let adds = hashes
            .iter()
            .any(|(tag, hash)| sample.seen.get(tag).is_none_or(|seen| seen.len() < limits.samples_per_tag && !seen.contains(hash)));
        if adds && sample.chosen.len() < limits.max_records_per_type {
            sample.chosen.insert(at);
            for (tag, hash) in hashes {
                sample.seen.entry(tag).or_default().insert(hash);
            }
        }
    });
    samples
}

/// `record` (header and body) re-encoded uncompressed without the sub-records tagged `strip`.
fn rewrite_record(record: &[u8], layout: Layout, strip: Option<[u8; 4]>) -> Result<Vec<u8>> {
    let mut body = Vec::new();
    for (tag, bytes) in sub_records(record, layout)? {
        if Some(tag) == strip {
            continue;
        }
        if bytes.len() > usize::from(u16::MAX) {
            // A sub-record past 64 KiB is announced by an `XXXX` sub-record carrying its true size.
            body.extend_from_slice(b"XXXX");
            body.extend_from_slice(&4u16.to_le_bytes());
            body.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
            body.extend_from_slice(&tag);
            body.extend_from_slice(&0u16.to_le_bytes());
        } else {
            body.extend_from_slice(&tag);
            body.extend_from_slice(&(bytes.len() as u16).to_le_bytes());
        }
        body.extend_from_slice(&bytes);
    }
    let mut out = record[..layout.record].to_vec();
    out[4..8].copy_from_slice(&(body.len() as u32).to_le_bytes());
    let flags = u32_at(record, 8) & !FLAG_COMPRESSED;
    out[8..12].copy_from_slice(&flags.to_le_bytes());
    out.extend_from_slice(&body);
    Ok(out)
}

/// What the rebuilt plugin changes about each record.
struct Rebuild<'a> {
    layout: Layout,
    /// The offsets of the records to keep.
    keep: &'a BTreeSet<usize>,
    /// The record type to re-encode, and the tag to leave out of it (`None`: re-encode all of it).
    record_type: [u8; 4],
    strip: Option<[u8; 4]>,
}

impl Rebuild<'_> {
    /// The records and groups of `data[start..end]` that hold a kept record, with the owners the groups need.
    fn prune(&self, data: &[u8], start: usize, end: usize) -> Result<Vec<u8>> {
        let layout = self.layout;
        let mut out = Vec::new();
        let mut at = start;
        while at + layout.group <= end {
            if &data[at..at + 4] == b"GRUP" {
                let (total, _, _) = group_at(data, at);
                ensure!(total >= layout.group && at + total <= end, "group at {at} overruns its parent");
                self.push_group(data, at, &self.prune(data, at + layout.group, at + total)?, &mut out);
                at += total;
                continue;
            }
            let head = head_at(data, at);
            let record_end = at + layout.record + head.data_size;
            ensure!(record_end <= end, "record at {at} overruns its group");
            // The group right after a record holds that record's children: a cell's references, a topic's responses.
            let mut next = record_end;
            let mut children = Vec::new();
            if next + layout.group <= end && &data[next..next + 4] == b"GRUP" {
                let (total, label, group_type) = group_at(data, next);
                if matches!(group_type, 1 | 6 | 7) && u32_at(&label, 0) == head.form_id {
                    ensure!(total >= layout.group && next + total <= end, "group at {next} overruns its parent");
                    let content = self.prune(data, next + layout.group, next + total)?;
                    if !content.is_empty() {
                        self.push_group(data, next, &content, &mut children);
                    }
                    next += total;
                }
            }
            if self.keep.contains(&at) || !children.is_empty() {
                let bytes = &data[at..record_end];
                if head.record_type == self.record_type {
                    out.extend_from_slice(&rewrite_record(bytes, layout, self.strip)?);
                } else {
                    out.extend_from_slice(bytes);
                }
                out.extend_from_slice(&children);
            }
            at = next;
        }
        Ok(out)
    }

    /// `content` under the header of the group at `at`, when it holds anything.
    fn push_group(&self, data: &[u8], at: usize, content: &[u8], out: &mut Vec<u8>) {
        if content.is_empty() {
            return;
        }
        let mut header = data[at..at + self.layout.group].to_vec();
        header[4..8].copy_from_slice(&((self.layout.group + content.len()) as u32).to_le_bytes());
        out.extend_from_slice(&header);
        out.extend_from_slice(content);
    }

    /// The plugin: its TES4 record, then every top-level group of `labels` that holds a kept record.
    fn plugin(&self, data: &[u8], labels: &[[u8; 4]]) -> Result<Vec<u8>> {
        let layout = self.layout;
        let tes4 = layout.record + head_at(data, 0).data_size;
        let mut out = data[..tes4].to_vec();
        let mut at = tes4;
        while at + layout.group <= data.len() {
            ensure!(&data[at..at + 4] == b"GRUP", "expected a top-level group at {at}");
            let (total, label, _) = group_at(data, at);
            ensure!(total >= layout.group && at + total <= data.len(), "top-level group at {at} overruns the file");
            if labels.contains(&label) {
                let content = self.prune(data, at + layout.group, at + total)?;
                self.push_group(data, at, &content, &mut out);
            }
            at += total;
        }
        Ok(out)
    }
}

fn index_digest(plugin: &[u8]) -> Result<u64> {
    let index = parse_esm_with_load_order(plugin, None)?;
    Ok(structural_digest(&index))
}

/// Which sub-record tags the parsers decode, per record type, found in the plugin `data`.
pub fn derive_decoded_subrecords(data: &[u8], limits: &ProbeLimits) -> Result<Derivation> {
    let layout = layout_of(data)?;
    let mut derivation = Derivation::default();
    for (record_type, sample) in sample(data, layout, limits) {
        let labels = top_groups_of(record_type);
        let build = |strip: Option<[u8; 4]>| -> Result<Vec<u8>> {
            Rebuild { layout, keep: &sample.chosen, record_type, strip }.plugin(data, &labels)
        };
        let baseline = match build(None).and_then(|plugin| index_digest(&plugin)) {
            Ok(digest) => digest,
            Err(error) => {
                derivation.skipped.push((record_type, format!("{error:#}")));
                continue;
            }
        };
        let mut decoded = BTreeSet::new();
        for &tag in sample.seen.keys() {
            let changed = match build(Some(tag)).and_then(|plugin| index_digest(&plugin)) {
                Ok(digest) => digest != baseline,
                // A parse that fails without the sub-record needs it.
                Err(_) => true,
            };
            if changed {
                decoded.insert(tag);
            }
        }
        derivation.decoded.insert(record_type, decoded);
    }
    Ok(derivation)
}

/// `[game]` and one `TYPE = ["TAG", …]` line per record type, the way `data/decoded_subrecords.toml` stores them.
pub fn render_table_section(game: &str, decoded: &Decoded) -> String {
    let mut out = format!("[{game}]\n");
    for (record_type, tags) in decoded {
        let tags: Vec<String> = tags.iter().map(|tag| format!("\"{}\"", encode_tag(tag))).collect();
        out.push_str(&format!("\"{}\" = [{}]\n", encode_tag(record_type), tags.join(", ")));
    }
    out
}

/// Merge `more` into `into`: a tag is decoded when it is in either.
pub fn union_into(into: &mut Decoded, more: &Decoded) {
    for (record_type, tags) in more {
        into.entry(*record_type).or_default().extend(tags.iter().copied());
    }
}

/// Where one game's plugins are found: the directory and the plugin files in it.
pub type SectionDir = (fn() -> std::path::PathBuf, &'static [&'static str]);

/// The plugins each section of `data/decoded_subrecords.toml` is derived from, in the file's order. The vanilla plugins of
/// each game, so the table says what the parsers decode of the content the games shipped; `fallout3nv` is Fallout 3 and
/// New Vegas together, as their records share one `GameKind`.
pub const TABLE_SOURCES: &[(&str, &[SectionDir])] = &[
    (
        "oblivion",
        &[(
            super::test_paths::oblivion_data_dir,
            &[
                "Oblivion.esm", "Knights.esp", "DLCShiveringIsles.esp", "DLCMehrunesRazor.esp", "DLCBattlehornCastle.esp",
                "DLCSpellTomes.esp", "DLCVileLair.esp", "DLCThievesDen.esp", "DLCFrostcrag.esp", "DLCHorseArmor.esp", "DLCOrrery.esp",
            ],
        )],
    ),
    (
        "fallout3nv",
        &[
            (
                super::test_paths::fnv_data_dir,
                &[
                    "FalloutNV.esm", "DeadMoney.esm", "HonestHearts.esm", "OldWorldBlues.esm", "LonesomeRoad.esm", "GunRunnersArsenal.esm",
                    "ClassicPack.esm", "CaravanPack.esm", "MercenaryPack.esm", "TribalPack.esm",
                ],
            ),
            (
                super::test_paths::fo3_data_dir,
                &["Fallout3.esm", "Anchorage.esm", "BrokenSteel.esm", "PointLookout.esm", "ThePitt.esm", "Zeta.esm"],
            ),
        ],
    ),
    (
        "skyrim",
        &[(super::test_paths::skyrim_se_data_dir, &["Skyrim.esm", "Update.esm", "Dawnguard.esm", "HearthFires.esm", "Dragonborn.esm"])],
    ),
    (
        "fallout4",
        &[(
            super::test_paths::fo4_data_dir,
            &["Fallout4.esm", "DLCRobot.esm", "DLCworkshop01.esm", "DLCworkshop02.esm", "DLCworkshop03.esm", "DLCCoast.esm", "DLCNukaWorld.esm"],
        )],
    ),
    ("fallout76", &[(super::test_paths::fo76_data_dir, &["SeventySix.esm", "NW.esm"])]),
    (
        "starfield",
        &[(
            super::test_paths::starfield_data_dir,
            &[
                "Starfield.esm", "ShatteredSpace.esm", "BlueprintShips-Starfield.esm", "SFBGS003.esm", "SFBGS006.esm", "SFBGS008.esm",
                "SFBGS00D.esm", "SFBGS050.esm",
            ],
        )],
    ),
];

/// The comment `data/decoded_subrecords.toml` opens with, printed by `inspect decoded-subrecords`.
pub const TABLE_HEADER: &str = "\
# The sub-records each game's parsers decode, per record type (PRD legacy-mod-compatibility LMC-REC-010).
#
# GENERATED, not edited: `esm::decode_probe` removes each sub-record tag from sampled real records, re-parses, and keeps
# the tags whose removal changes the parsed index. Regenerate the file with
#
#   cargo run --release -p byroredux-plugin --example inspect -- decoded-subrecords > crates/plugin/data/decoded_subrecords.toml
#
# from the plugins in `decode_probe::TABLE_SOURCES` (the vanilla plugins of each game). `fallout3nv` is the union of
# Fallout 3 and New Vegas (one GameKind). A record type with an empty list was probed and decodes nothing the sampled
# records carry; a type or game not listed here is treated the same way, so a sub-record the table does not name is kept
# as opaque bytes, never dropped (esm/opaque.rs). Tags are printable ASCII; any other byte is written %HH.
";

/// The plugin files of table section `section`, found where the games are installed; `None` when the section is unknown
/// or any file is missing (the game is not installed, so the section cannot be re-derived here).
pub fn section_plugins(section: &str) -> Option<Vec<std::path::PathBuf>> {
    let (_, dirs) = TABLE_SOURCES.iter().find(|(name, _)| *name == section)?;
    let mut paths = Vec::new();
    for (dir, files) in *dirs {
        let dir = dir();
        for file in *files {
            let path = dir.join(file);
            path.is_file().then_some(())?;
            paths.push(path);
        }
    }
    Some(paths)
}

/// The decoded-tag derivation of table section `section`: the union over its [`TABLE_SOURCES`] plugins. `None` when
/// they are not all installed.
pub fn derive_section(section: &str, limits: &ProbeLimits) -> Result<Option<Decoded>> {
    let Some(paths) = section_plugins(section) else {
        return Ok(None);
    };
    let mut decoded = BTreeMap::new();
    for path in paths {
        union_into(&mut decoded, &derive_from_path(&path, limits)?.decoded);
    }
    Ok(Some(decoded))
}

/// The decoded-tag derivation of the plugin file at `path`.
pub fn derive_from_path(path: &std::path::Path, limits: &ProbeLimits) -> Result<Derivation> {
    let data = std::fs::read(path).with_context(|| format!("read {}", path.display()))?;
    derive_decoded_subrecords(&data, limits).with_context(|| format!("probe {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(record_type: &[u8; 4], form_id: u32, subs: &[(&[u8; 4], &[u8])]) -> Vec<u8> {
        let mut body = Vec::new();
        for (tag, bytes) in subs {
            body.extend_from_slice(*tag);
            body.extend_from_slice(&(bytes.len() as u16).to_le_bytes());
            body.extend_from_slice(bytes);
        }
        let mut out = record_type.to_vec();
        out.extend_from_slice(&(body.len() as u32).to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
        out.extend_from_slice(&form_id.to_le_bytes());
        out.extend_from_slice(&[0u8; 8]);
        out.extend_from_slice(&body);
        out
    }

    fn group(label: &[u8; 4], group_type: u32, content: &[u8]) -> Vec<u8> {
        let mut out = b"GRUP".to_vec();
        out.extend_from_slice(&((24 + content.len()) as u32).to_le_bytes());
        out.extend_from_slice(label);
        out.extend_from_slice(&group_type.to_le_bytes());
        out.extend_from_slice(&[0u8; 8]);
        out.extend_from_slice(content);
        out
    }

    fn hedr() -> Vec<u8> {
        let mut hedr = 1.7f32.to_le_bytes().to_vec();
        hedr.extend_from_slice(&[0u8; 8]);
        hedr
    }

    /// A MISC record: its EDID and FULL reach the index (`items`), its DATA decodes into the value and weight, and a
    /// made-up `ZZZZ` reaches nothing.
    fn misc(form_id: u32, name: &str, value: u32, extra: &[u8]) -> Vec<u8> {
        let mut data = value.to_le_bytes().to_vec();
        data.extend_from_slice(&1.5f32.to_le_bytes());
        record(
            b"MISC",
            form_id,
            &[(b"EDID", format!("{name}\0").as_bytes()), (b"FULL", format!("{name} full\0").as_bytes()), (b"DATA", &data), (b"ZZZZ", extra)],
        )
    }

    fn plugin_with(top: Vec<u8>) -> Vec<u8> {
        let mut data = record(b"TES4", 0, &[(b"HEDR", &hedr())]);
        data.extend_from_slice(&top);
        data
    }

    /// The probe finds the sub-records MISC's parser reads and not the one it ignores.
    #[test]
    fn a_removal_that_changes_the_index_marks_the_sub_record_decoded() {
        let records: Vec<u8> = [misc(0x0000_0801, "Alpha", 10, b"one"), misc(0x0000_0802, "Beta", 20, b"two")].concat();
        let data = plugin_with(group(b"MISC", 0, &records));
        let found = derive_decoded_subrecords(&data, &ProbeLimits::default()).expect("probe");
        assert!(found.skipped.is_empty(), "{:?}", found.skipped);
        let misc_tags = &found.decoded[b"MISC"];
        for tag in [b"EDID", b"FULL", b"DATA"] {
            assert!(misc_tags.contains(tag), "{} should be decoded: {misc_tags:?}", String::from_utf8_lossy(tag));
        }
        assert!(!misc_tags.contains(b"ZZZZ"), "an ignored sub-record is not decoded: {misc_tags:?}");
    }

    /// Records are sampled for their different payloads, not all of them: a thousand records of one shape give a plugin
    /// of `samples_per_tag` records.
    #[test]
    fn sampling_stops_once_every_tag_has_enough_payloads() {
        let records: Vec<u8> = (0..200u32).flat_map(|i| misc(0x0000_1000 + i, "Same", 7, b"same")).collect();
        let data = plugin_with(group(b"MISC", 0, &records));
        let layout = layout_of(&data).unwrap();
        let limits = ProbeLimits::default();
        let samples = sample(&data, layout, &limits);
        // Every record has a distinct FormID but the same payloads, so one record shows each tag and no more is wanted.
        assert_eq!(samples[b"MISC"].chosen.len(), 1);
        let varied: Vec<u8> = (0..200u32).flat_map(|i| misc(0x0000_1000 + i, "Same", i, b"same")).collect();
        let data = plugin_with(group(b"MISC", 0, &varied));
        let samples = sample(&data, layout_of(&data).unwrap(), &limits);
        assert_eq!(samples[b"MISC"].chosen.len(), limits.samples_per_tag, "six distinct DATA payloads, then enough");
    }

    /// A record under a topic is kept with its topic and the topic's children group, so it parses where it parsed in the
    /// whole plugin.
    #[test]
    fn a_nested_record_keeps_the_structure_it_needs() {
        let info = record(b"INFO", 0x0000_0903, &[(b"EDID", b"Line\0"), (b"NAM1", b"Hello\0")]);
        let other = record(b"DIAL", 0x0000_0902, &[(b"EDID", b"Other\0")]);
        let topic = record(b"DIAL", 0x0000_0901, &[(b"EDID", b"Topic\0")]);
        let top = group(b"DIAL", 0, &[topic, group(&0x0000_0901u32.to_le_bytes(), 7, &info), other].concat());
        let data = plugin_with(top);
        let layout = layout_of(&data).unwrap();
        let mut offsets = Vec::new();
        for_each_record(&data, layout, |at, head| {
            if &head.record_type == b"INFO" {
                offsets.push(at);
            }
        });
        assert_eq!(offsets.len(), 1);
        let keep: BTreeSet<usize> = offsets.into_iter().collect();
        let plugin = Rebuild { layout, keep: &keep, record_type: *b"INFO", strip: None }.plugin(&data, &[*b"DIAL"]).expect("rebuild");
        let mut types = Vec::new();
        for_each_record(&plugin, layout, |_, head| types.push(head.record_type));
        assert_eq!(types, [*b"DIAL", *b"INFO"], "the topic that owns the response, not the unrelated one");
        parse_esm_with_load_order(&plugin, None).expect("the rebuilt plugin parses");
    }

    #[test]
    fn the_table_renders_one_line_per_record_type() {
        let mut decoded = BTreeMap::new();
        decoded.insert(*b"MISC", [*b"DATA", *b"EDID"].into_iter().collect());
        decoded.insert(*b"NPC_", BTreeSet::new());
        decoded.insert(*b"IMAD", [[0x14, b'I', b'A', b'D']].into_iter().collect());
        assert_eq!(
            render_table_section("fallout4", &decoded),
            "[fallout4]\n\"IMAD\" = [\"%14IAD\"]\n\"MISC\" = [\"DATA\", \"EDID\"]\n\"NPC_\" = []\n"
        );
    }
}
