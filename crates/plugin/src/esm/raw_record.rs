//! One record of one plugin, read raw from the file, and the sub-records two such records disagree on (PRD
//! legacy-mod-compatibility LMC-REC-007, "the fields that differ").
//!
//! The merged index keeps only the winning record of each form, decoded; the losers' records are gone by the end of
//! the fold. The conflict report reads them back from the plugin files on demand: [`find_raw_record`] seeks over the
//! top-level groups the record type cannot be in and loads only the ones it can (a `WEAP` reads the `WEAP` group; a
//! `REFR` the `CELL` and `WRLD` groups), and [`diff_fields`] compares two records sub-record by sub-record.
//!
//! The comparison is of bytes. A sub-record that holds a Form ID stores it plugin-locally (its top byte indexes the
//! plugin's own master list), so two plugins with different master lists can store the same form in different bytes;
//! [`diff_fields`] reports such a field as different, and the caller says so when the master lists differ.

use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use anyhow::{Context, Result};

use super::reader::{EsmReader, EsmVariant, FormIdRemap, SubRecord, FLAG_COMPRESSED};

/// A record as its plugin stores it, decompressed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawRecord {
    pub record_type: [u8; 4],
    pub flags: u32,
    pub subrecords: Vec<(/* tag */ [u8; 4], /* data */ Vec<u8>)>,
}

/// The top-level groups a record of `record_type` can sit in: its own, or for the records that live under a cell, a
/// worldspace or a topic, those.
pub fn top_groups_of(record_type: [u8; 4]) -> Vec<[u8; 4]> {
    match &record_type {
        b"CELL" | b"REFR" | b"ACHR" | b"ACRE" | b"PGRE" | b"PMIS" | b"PHZD" | b"PARW" | b"PBAR" | b"PBEA" | b"PCON"
        | b"PFLA" | b"NAVM" | b"LAND" | b"PGRD" => vec![*b"CELL", *b"WRLD"],
        b"INFO" => vec![*b"DIAL"],
        _ => vec![record_type],
    }
}

/// The record `form_id` of type `record_type` in the plugin at `path`, read with `remap` so `form_id` is the
/// load-order-global id the merged index uses (`None` reads the plugin's own ids). `Ok(None)` when the plugin does not
/// carry it.
pub fn find_raw_record(path: &Path, remap: Option<FormIdRemap>, record_type: [u8; 4], form_id: u32) -> Result<Option<RawRecord>> {
    let mut file = std::fs::File::open(path).with_context(|| format!("open {}", path.display()))?;
    let length = file.metadata().map(|meta| meta.len()).unwrap_or(0);
    let mut head = [0u8; 24];
    file.read_exact(&mut head).with_context(|| format!("read {}", path.display()))?;
    let variant = EsmVariant::detect(&head);
    let header_size = variant.record_header_size() as u64;
    let groups = top_groups_of(record_type);
    // Skip the TES4 record, then visit the top-level groups in turn.
    let mut offset = header_size + u64::from(u32::from_le_bytes(head[4..8].try_into().expect("four bytes")));
    let mut group_header = vec![0u8; variant.group_header_size()];
    while offset + group_header.len() as u64 <= length {
        file.seek(SeekFrom::Start(offset))?;
        file.read_exact(&mut group_header)?;
        anyhow::ensure!(&group_header[..4] == b"GRUP", "{}: expected a top-level GRUP at {offset}", path.display());
        let total = u64::from(u32::from_le_bytes(group_header[4..8].try_into().expect("four bytes")));
        anyhow::ensure!(total >= group_header.len() as u64, "{}: GRUP at {offset} is shorter than its header", path.display());
        let label: [u8; 4] = group_header[8..12].try_into().expect("four bytes");
        if groups.contains(&label) {
            let mut group = vec![0u8; (total.min(length - offset)) as usize];
            file.seek(SeekFrom::Start(offset))?;
            file.read_exact(&mut group)?;
            let mut reader = EsmReader::with_variant(&group, variant);
            if let Some(remap) = remap.clone() {
                reader.set_form_id_remap(remap);
            }
            if let Some(found) = find_in(&mut reader, group.len(), record_type, form_id, 0)? {
                return Ok(Some(found));
            }
        }
        offset += total;
    }
    Ok(None)
}

fn find_in(reader: &mut EsmReader, end: usize, record_type: [u8; 4], form_id: u32, depth: u32) -> Result<Option<RawRecord>> {
    while reader.position() < end && reader.remaining() > 0 {
        if reader.is_group() {
            let group = reader.read_group_header()?;
            let Some(group_end) = reader.bounded_group_content_end(&group, depth, end, "find_raw_record") else {
                continue;
            };
            if let Some(found) = find_in(reader, group_end, record_type, form_id, depth + 1)? {
                return Ok(Some(found));
            }
            reader.seek_to(group_end);
            continue;
        }
        let header = reader.read_record_header()?;
        if header.record_type == record_type && header.form_id == form_id {
            let subrecords = reader.read_sub_records(&header)?;
            return Ok(Some(RawRecord {
                record_type,
                flags: header.flags,
                subrecords: subrecords.into_iter().map(|SubRecord { sub_type, data }| (sub_type, data)).collect(),
            }));
        }
        reader.skip_record(&header);
    }
    Ok(None)
}

/// One way two records of a form differ.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FieldDiff {
    /// The record header flags (deleted, persistent, initially disabled, …), without the compressed bit, which says
    /// how the record was stored rather than what it holds (#7248).
    Flags { winner: u32, other: u32 },
    /// A sub-record each record has once, with different bytes.
    Differs { tag: [u8; 4] },
    /// `count` instances of `tag` the winner has and the other does not: a sub-record only the winner carries, or the
    /// entries of a repeated one (a list) that are not in the other's.
    Added { tag: [u8; 4], count: usize },
    /// `count` instances of `tag` the other has and the winner does not.
    Dropped { tag: [u8; 4], count: usize },
    /// A repeated `tag` with the same entries in a different order.
    Reordered { tag: [u8; 4] },
}

impl std::fmt::Display for FieldDiff {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = |tag: &[u8; 4]| String::from_utf8_lossy(tag).into_owned();
        match self {
            Self::Flags { winner, other } => write!(f, "record flags {other:08X} -> {winner:08X}"),
            Self::Differs { tag } => write!(f, "{} differs", name(tag)),
            Self::Added { tag, count: 1 } => write!(f, "{} added", name(tag)),
            Self::Added { tag, count } => write!(f, "{} {count} added", name(tag)),
            Self::Dropped { tag, count: 1 } => write!(f, "{} dropped", name(tag)),
            Self::Dropped { tag, count } => write!(f, "{} {count} dropped", name(tag)),
            Self::Reordered { tag } => write!(f, "{} reordered", name(tag)),
        }
    }
}

/// How `winner` differs from `other`, by sub-record tag in the winner's order (then the other's leftovers). A tag
/// each record carries once is compared by bytes; a repeated tag is a list, compared as a multiset of entries, so one
/// entry inserted into a list of two hundred is one addition, not two hundred shifted differences.
pub fn diff_fields(winner: &RawRecord, other: &RawRecord) -> Vec<FieldDiff> {
    fn by_tag(record: &RawRecord) -> Vec<([u8; 4], Vec<&[u8]>)> {
        let mut tags: Vec<([u8; 4], Vec<&[u8]>)> = Vec::new();
        for (tag, data) in &record.subrecords {
            match tags.iter_mut().find(|(seen, _)| seen == tag) {
                Some((_, entries)) => entries.push(data),
                None => tags.push((*tag, vec![data])),
            }
        }
        tags
    }
    /// The entries of `a` with no partner left in `b`, each `b` entry partnering one `a` entry.
    fn unmatched(a: &[&[u8]], b: &[&[u8]]) -> usize {
        let mut used = vec![false; b.len()];
        a.iter()
            .filter(|entry| match b.iter().enumerate().position(|(i, other)| !used[i] && other == *entry) {
                Some(i) => {
                    used[i] = true;
                    false
                }
                None => true,
            })
            .count()
    }
    let mut diffs = Vec::new();
    // #7248 — the compressed bit is storage, not a value: a record one tool saved compressed and another did not,
    // with the same sub-records (already decompressed here), is identical (LMC-REC-006, xEdit's ITM).
    let (winner_flags, other_flags) = (winner.flags & !FLAG_COMPRESSED, other.flags & !FLAG_COMPRESSED);
    if winner_flags != other_flags {
        diffs.push(FieldDiff::Flags { winner: winner_flags, other: other_flags });
    }
    let (ours, theirs) = (by_tag(winner), by_tag(other));
    let empty: Vec<&[u8]> = Vec::new();
    let mut tags: Vec<[u8; 4]> = ours.iter().map(|(tag, _)| *tag).collect();
    tags.extend(theirs.iter().map(|(tag, _)| *tag).filter(|tag| !ours.iter().any(|(ours, _)| ours == tag)));
    for tag in tags {
        let mine = ours.iter().find(|(t, _)| *t == tag).map_or(&empty, |(_, entries)| entries);
        let other = theirs.iter().find(|(t, _)| *t == tag).map_or(&empty, |(_, entries)| entries);
        if mine.len() == 1 && other.len() == 1 {
            if mine[0] != other[0] {
                diffs.push(FieldDiff::Differs { tag });
            }
            continue;
        }
        let (added, dropped) = (unmatched(mine, other), unmatched(other, mine));
        if added > 0 {
            diffs.push(FieldDiff::Added { tag, count: added });
        }
        if dropped > 0 {
            diffs.push(FieldDiff::Dropped { tag, count: dropped });
        }
        if added == 0 && dropped == 0 && mine != other {
            diffs.push(FieldDiff::Reordered { tag });
        }
    }
    diffs
}

/// Whether two plugins store a form ID with the same bytes: every plugin both can address (their masters and
/// themselves) sits at the same master index in each. When they do not, a sub-record holding a form ID can differ in
/// bytes while naming the same form. `masters` are each plugin's `MAST` list, `own` its own name, all [`plugin_key`]ed.
///
/// [`plugin_key`]: byroredux_core::form_id::plugin_key
pub fn same_form_id_encoding(masters: &[String], own: &str, other_masters: &[String], other_own: &str) -> bool {
    let table = |masters: &[String], own: &str| -> Vec<String> { masters.iter().cloned().chain(std::iter::once(own.to_owned())).collect() };
    let (ours, theirs) = (table(masters, own), table(other_masters, other_own));
    ours.iter().enumerate().all(|(index, plugin)| theirs.iter().position(|other| other == plugin).is_none_or(|at| at == index))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(flags: u32, subs: &[(&[u8; 4], &[u8])]) -> RawRecord {
        RawRecord { record_type: *b"WEAP", flags, subrecords: subs.iter().map(|(tag, data)| (**tag, data.to_vec())).collect() }
    }

    /// A tag each record has once is compared by bytes; a repeated tag is a list, compared as a multiset, so one
    /// inserted entry is one addition rather than a shift of every later entry; flags are compared too.
    #[test]
    fn single_fields_compare_by_bytes_and_lists_as_multisets() {
        let master = record(0, &[(b"EDID", b"Gun\0"), (b"DATA", &[1, 2]), (b"QSTI", &[1]), (b"QSTI", &[2]), (b"QSTI", &[3]), (b"KWDA", &[7]), (b"KWDA", &[8])]);
        let patch = record(
            0x400,
            &[(b"EDID", b"Gun\0"), (b"DATA", &[1, 3]), (b"QSTI", &[1]), (b"QSTI", &[9]), (b"QSTI", &[2]), (b"QSTI", &[3]), (b"KWDA", &[8]), (b"KWDA", &[7]), (b"DNAM", &[9])],
        );
        let diffs = diff_fields(&patch, &master);
        assert_eq!(
            diffs,
            vec![
                FieldDiff::Flags { winner: 0x400, other: 0 },
                FieldDiff::Differs { tag: *b"DATA" },
                FieldDiff::Added { tag: *b"QSTI", count: 1 },
                FieldDiff::Reordered { tag: *b"KWDA" },
                FieldDiff::Added { tag: *b"DNAM", count: 1 },
            ]
        );
        assert_eq!(
            diffs.iter().map(ToString::to_string).collect::<Vec<_>>(),
            ["record flags 00000000 -> 00000400", "DATA differs", "QSTI added", "KWDA reordered", "DNAM added"]
        );
        let trimmed = record(0, &[(b"EDID", b"Gun\0"), (b"DATA", &[1, 2]), (b"QSTI", &[3])]);
        assert_eq!(
            diff_fields(&trimmed, &master).iter().map(ToString::to_string).collect::<Vec<_>>(),
            ["QSTI 2 dropped", "KWDA 2 dropped"]
        );
        assert!(diff_fields(&master, &master.clone()).is_empty(), "an identical override differs in nothing");
        // #7248 — the same values stored compressed by one tool and uncompressed by another are identical; a real
        // flag change beside the compressed bit still shows, without it.
        let stored_compressed = RawRecord { flags: FLAG_COMPRESSED, ..master.clone() };
        assert!(diff_fields(&stored_compressed, &master).is_empty(), "compression is storage, not a difference");
        let persistent_and_compressed = RawRecord { flags: FLAG_COMPRESSED | 0x400, ..master.clone() };
        assert_eq!(diff_fields(&persistent_and_compressed, &master), vec![FieldDiff::Flags { winner: 0x400, other: 0 }]);
    }

    /// Two plugins encode form IDs alike when every plugin both address sits at the same index in each.
    #[test]
    fn form_id_encodings_agree_when_shared_plugins_share_an_index() {
        let s = |names: &[&str]| names.iter().map(|n| (*n).to_owned()).collect::<Vec<_>>();
        assert!(same_form_id_encoding(&s(&[]), "falloutnv.esm", &s(&["falloutnv.esm"]), "deadmoney.esm"));
        assert!(same_form_id_encoding(&s(&["falloutnv.esm"]), "deadmoney.esm", &s(&["falloutnv.esm", "deadmoney.esm"]), "patch.esp"));
        assert!(!same_form_id_encoding(&s(&["falloutnv.esm", "deadmoney.esm"]), "a.esp", &s(&["falloutnv.esm", "honesthearts.esm", "deadmoney.esm"]), "b.esp"));
    }

    /// A placed reference is looked for under the cells and worldspaces, a topic's response under the topics.
    #[test]
    fn records_under_a_cell_are_looked_for_in_the_cell_and_world_groups() {
        assert_eq!(top_groups_of(*b"REFR"), [*b"CELL", *b"WRLD"]);
        assert_eq!(top_groups_of(*b"INFO"), [*b"DIAL"]);
        assert_eq!(top_groups_of(*b"WEAP"), [*b"WEAP"]);
    }

    /// A two-group Tes5+ plugin on disk: the finder seeks past the group the type cannot be in, finds the record in
    /// its own group, decodes it, and answers `None` for a form the plugin does not carry.
    #[test]
    fn the_finder_reads_the_record_from_its_group() {
        fn sub(tag: &[u8; 4], data: &[u8]) -> Vec<u8> {
            let mut out = tag.to_vec();
            out.extend_from_slice(&(data.len() as u16).to_le_bytes());
            out.extend_from_slice(data);
            out
        }
        fn rec(tag: &[u8; 4], form_id: u32, body: &[u8]) -> Vec<u8> {
            let mut out = tag.to_vec();
            out.extend_from_slice(&(body.len() as u32).to_le_bytes());
            out.extend_from_slice(&0u32.to_le_bytes());
            out.extend_from_slice(&form_id.to_le_bytes());
            out.extend_from_slice(&[0; 8]);
            out.extend_from_slice(body);
            out
        }
        fn grup(label: &[u8; 4], content: &[u8]) -> Vec<u8> {
            let mut out = b"GRUP".to_vec();
            out.extend_from_slice(&((24 + content.len()) as u32).to_le_bytes());
            out.extend_from_slice(label);
            out.extend_from_slice(&[0; 12]);
            out.extend_from_slice(content);
            out
        }
        let mut file = rec(b"TES4", 0, &sub(b"HEDR", &[0x9A, 0x99, 0xD9, 0x3F, 0, 0, 0, 0, 0, 0, 0, 0]));
        let mut misc = rec(b"MISC", 0x0000_0A01, &[sub(b"EDID", b"Wrong\0"), sub(b"DATA", &[1])].concat());
        misc.extend(rec(b"MISC", 0x0000_0A02, &sub(b"EDID", b"AlsoWrong\0")));
        file.extend(grup(b"MISC", &misc));
        let mut weapons = rec(b"WEAP", 0x0000_0A01, &sub(b"EDID", b"Before\0"));
        weapons.extend(rec(b"WEAP", 0x0000_0B02, &[sub(b"EDID", b"Gun\0"), sub(b"DATA", &[5, 6])].concat()));
        file.extend(grup(b"WEAP", &weapons));

        let dir = std::env::temp_dir().join(format!("byro-raw-record-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("Mod.esp");
        std::fs::write(&path, &file).unwrap();
        let found = find_raw_record(&path, None, *b"WEAP", 0x0000_0B02).unwrap().expect("found");
        assert_eq!(found.subrecords, vec![(*b"EDID", b"Gun\0".to_vec()), (*b"DATA", vec![5, 6])]);
        assert_eq!(find_raw_record(&path, None, *b"WEAP", 0x0000_0A02).unwrap(), None, "the MISC with that id is not a WEAP");
        assert_eq!(find_raw_record(&path, None, *b"ARMO", 0x0000_0B02).unwrap(), None);
        std::fs::remove_dir_all(&dir).ok();
    }
}
