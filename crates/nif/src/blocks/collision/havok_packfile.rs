//! Havok "classic packfile" container decoder for the raw
//! [`BhkSystemBinary`](super::BhkSystemBinary) blob carried by FO4/FO76
//! `bhkPhysicsSystem` / `bhkRagdollSystem` blocks (#3809, EX-14/15 item
//! C4).
//!
//! This decodes ONLY the outer container: the packfile header, the
//! section table, and the `__classnames__` section's string list. The
//! `__data__` section payload — the actual `hknpCompressedMeshShapeData`
//! collision geometry — is Havok's own proprietary bit-packed encoding
//! and is **not** decoded here; it is exposed as a raw byte range for
//! future work (the real remaining blocker, per the issue: the
//! `__types__` section is empty in every sampled blob, meaning the
//! payload's field layout isn't self-describing in the file — it relies
//! on the loading application already knowing Havok's built-in `hknp`
//! class layouts by name).
//!
//! Every field name and offset below was derived by byte-offset
//! arithmetic against 25+ real FO4 `_physics.nif` samples pulled from
//! `Fallout4 - MeshesExtra.ba2`, cross-checked for internal
//! self-consistency (e.g. a section's `end_offset` added to its
//! `absolute_data_start` lands exactly on the blob's total length; the
//! `__classnames__` section's fixup-table start lands exactly on
//! `__types__`'s `absolute_data_start`). No Havok SDK source, leaked or
//! otherwise, was consulted — this project's no-guessing policy calls
//! for pure corpus-derived analysis, and the classic packfile magic and
//! section-header shape independently match decades of public
//! community reverse-engineering of Havok's `.hkx` container format.
//! A few header fields have no confirmed semantic across the corpus and
//! are kept as opaque `reserved_*` fields rather than guessed at.
//!
//! This decoder is validated against `file_version == 11` /
//! `contents_version == "hk_2014.1.0-r1"` — the only variant observed in
//! the FO4 corpus. Other Havok packfile versions may use a different
//! header shape; this module does not attempt to handle them.

use std::io;
use std::ops::Range;

/// Byte-identical prefix confirmed across every sampled
/// [`BhkSystemBinary`](super::BhkSystemBinary) blob: the classic Havok
/// packfile magic pair.
pub const HAVOK_PACKFILE_MAGIC: [u32; 2] = [0x57e0_e057, 0x10c0_c010];

/// Size of one section-table entry: a 20-byte name block (19 bytes of
/// null-padded ASCII + a `0xFF` terminator byte, verified at the same
/// relative offset regardless of name length), 7×`u32` offset fields
/// (28 bytes), then 16 bytes of `0xFFFFFFFF` padding — 64 bytes total.
const SECTION_HEADER_SIZE: usize = 64;
const SECTION_NAME_FIELD_LEN: usize = 19;
const SECTION_TABLE_START: usize = 0x40;

/// Entry stride of the local-fixup table: two `u32`s.
const LOCAL_FIXUP_SIZE: usize = 8;
/// Entry stride of the global- and virtual-fixup tables: three `u32`s.
const GLOBAL_FIXUP_SIZE: usize = 12;

/// A pointer relocation *within* one section: the `u32`/`u64` slot at
/// `src_offset` holds a pointer whose target is `dst_offset`, both
/// relative to the owning section's `absolute_data_start`.
///
/// These are what make an object's variable-length members reachable —
/// an array member's data pointer is a local fixup from the member slot
/// to the array's payload elsewhere in the same section.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocalFixup {
    pub src_offset: u32,
    pub dst_offset: u32,
}

/// A pointer relocation from this section into another section.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GlobalFixup {
    pub src_offset: u32,
    pub dst_section: u32,
    pub dst_offset: u32,
}

/// "At `data_offset` there is an object whose class name lives at
/// `class_name_offset` in section `class_section`."
///
/// This is the table that makes the `__data__` section navigable: it
/// gives the exact start offset and runtime type of every top-level
/// object in the blob, without decoding any object's fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VirtualFixup {
    pub data_offset: u32,
    pub class_section: u32,
    pub class_name_offset: u32,
}

/// A located, typed object inside the `__data__` section, resolved from
/// a [`VirtualFixup`] against the class-name table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackfileObject {
    /// e.g. `"hknpCompressedMeshShapeData"`.
    pub class_name: String,
    /// Offset relative to the owning section's `absolute_data_start`.
    pub section_offset: u32,
    /// Offset from the start of the whole blob.
    pub absolute_offset: usize,
}

/// One entry of the packfile's section table (classic layout carries
/// exactly three: `__classnames__`, `__types__`, `__data__`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackfileSection {
    pub name: String,
    /// Absolute offset (from the start of the blob) where this
    /// section's content begins.
    pub absolute_data_start: u32,
    /// Byte offsets below are relative to `absolute_data_start`.
    pub local_fixups_offset: u32,
    pub global_fixups_offset: u32,
    pub virtual_fixups_offset: u32,
    pub exports_offset: u32,
    pub imports_offset: u32,
    /// Relative offset of the end of this section's data — added to
    /// `absolute_data_start` this equals the blob's total length for
    /// the final (`__data__`) section, verified across the corpus.
    pub end_offset: u32,
    /// Intra-section pointer relocations, from
    /// `[local_fixups_offset, global_fixups_offset)`.
    pub local_fixups: Vec<LocalFixup>,
    /// Cross-section pointer relocations, from
    /// `[global_fixups_offset, virtual_fixups_offset)`.
    pub global_fixups: Vec<GlobalFixup>,
    /// Object-location/type records, from
    /// `[virtual_fixups_offset, exports_offset)`.
    pub virtual_fixups: Vec<VirtualFixup>,
}

impl PackfileSection {
    /// The section's absolute end offset within the blob.
    pub fn absolute_end(&self) -> u32 {
        self.absolute_data_start + self.end_offset
    }

    /// The section's named-content byte range — `[absolute_data_start,
    /// absolute_data_start + local_fixups_offset)` — i.e. the actual
    /// payload before its fixup tables begin. For `__classnames__` this
    /// is the record list; for `__data__` this is the serialized object
    /// image — navigable via [`virtual_fixups`](Self::virtual_fixups),
    /// which names and locates every top-level object in it. Empty for
    /// `__types__` in every sampled blob.
    pub fn content_range(&self) -> Range<usize> {
        self.absolute_data_start as usize
            ..(self.absolute_data_start + self.local_fixups_offset) as usize
    }
}

/// The fixed-layout portion of the packfile header (bytes `0..0x40`,
/// immediately followed by the section table).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HavokPackfileHeader {
    pub user_tag: u32,
    pub file_version: u32,
    /// `[bytesInPointer, littleEndian, reusePaddingOptimization,
    /// emptyBaseClassOptimization]` per the publicly documented classic
    /// Havok packfile layout rules byte; not independently decoded
    /// here beyond capturing the raw bytes.
    pub layout_rules: [u8; 4],
    pub num_sections: u32,
    /// Unconfirmed semantic; byte-identical (`2`) across every sampled
    /// blob. Kept raw rather than guessed at.
    pub reserved_after_num_sections: u32,
    /// e.g. `"hk_2014.1.0-r1"` — identical across the whole FO4 corpus.
    pub contents_version: String,
    /// Unconfirmed semantic; byte-identical (`0x4b`) across every
    /// sampled blob and doesn't match any in-header offset. Kept raw.
    pub reserved_before_version_string: u32,
    /// Unconfirmed semantic; byte-identical (`0x15`) across every
    /// sampled blob, immediately after the version string's padding.
    pub reserved_after_version_string: u32,
}

/// A parsed Havok classic-packfile container.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HavokPackfile {
    pub header: HavokPackfileHeader,
    pub sections: Vec<PackfileSection>,
    /// Class names declared in the `__classnames__` section — the
    /// `hknp` (Havok Next-gen Physics) runtime types this blob's
    /// objects were serialized against, e.g.
    /// `hknpCompressedMeshShapeData`. Resolved by name against the
    /// loading application's own type registry; FO4 physics uses the
    /// `hknp` family, not the older `hkp` rigid-body pipeline.
    pub class_names: Vec<String>,
    /// Byte offset of each [`class_names`](Self::class_names) entry's
    /// *name text*, relative to the `__classnames__` section's
    /// `absolute_data_start`. Parallel to `class_names` — a
    /// [`VirtualFixup`] identifies a class by this offset, not by index.
    pub class_name_offsets: Vec<u32>,
}

impl HavokPackfile {
    pub fn section(&self, name: &str) -> Option<&PackfileSection> {
        self.sections.iter().find(|s| s.name == name)
    }

    /// Whether `class_names` contains the given type (exact match).
    pub fn has_class(&self, name: &str) -> bool {
        self.class_names.iter().any(|c| c == name)
    }

    /// Resolve a [`VirtualFixup::class_name_offset`] to its class name.
    pub fn class_name_at(&self, offset: u32) -> Option<&str> {
        self.class_name_offsets
            .iter()
            .position(|&o| o == offset)
            .map(|i| self.class_names[i].as_str())
    }

    /// Every top-level object in `__data__`, located and typed.
    ///
    /// This is what makes the payload addressable without decoding a
    /// single object field: the virtual-fixup table states outright
    /// where each object begins and which runtime class it was
    /// serialized against. In the FO4 precombine corpus the answer is
    /// the same five objects in every blob — `hknpPhysicsSystemData`,
    /// `hknpCompressedMeshShape`, `hkRefCountedProperties`,
    /// `hknpBSMaterialProperties`, `hknpCompressedMeshShapeData`.
    ///
    /// Objects are returned in table order, which is ascending
    /// `data_offset` in every sampled blob; an object's extent is
    /// therefore bounded by the next object's start (and the last by the
    /// section's `local_fixups_offset`).
    pub fn objects(&self) -> Vec<PackfileObject> {
        let Some(data) = self.section("__data__") else {
            return Vec::new();
        };
        data.virtual_fixups
            .iter()
            .filter_map(|vf| {
                self.class_name_at(vf.class_name_offset)
                    .map(|name| PackfileObject {
                        class_name: name.to_string(),
                        section_offset: vf.data_offset,
                        absolute_offset: data.absolute_data_start as usize
                            + vf.data_offset as usize,
                    })
            })
            .collect()
    }
}

fn read_u32_le(data: &[u8], offset: usize) -> io::Result<u32> {
    data.get(offset..offset + 4)
        .map(|s| u32::from_le_bytes(s.try_into().unwrap()))
        .ok_or_else(|| io::Error::new(io::ErrorKind::UnexpectedEof, "havok packfile: truncated"))
}

fn read_cstr(data: &[u8], offset: usize, max_len: usize) -> io::Result<String> {
    let end = (offset + max_len).min(data.len());
    let slice = data.get(offset..end).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "havok packfile: truncated string",
        )
    })?;
    let nul = slice.iter().position(|&b| b == 0).unwrap_or(slice.len());
    Ok(String::from_utf8_lossy(&slice[..nul]).into_owned())
}

/// Fixed per-entry prefix in the `__classnames__` section, ahead of each
/// null-terminated name: verified as 5 bytes (a leading 4-byte value —
/// likely a class-signature hash, semantic unconfirmed — plus 1 further
/// byte) by offset arithmetic across the corpus (e.g. `"hkClass"`'s `h`
/// lands exactly 5 bytes after its record's start, and the next
/// record's prefix starts exactly `5 + name.len() + 1` bytes later).
const CLASSNAME_RECORD_PREFIX_LEN: usize = 5;

/// Walk the `__classnames__` section's content as a sequence of
/// `[5-byte prefix][null-terminated name]` records. Real corpus samples
/// carry a few trailing padding bytes after the last real record that
/// don't form a valid record (too short, or not printable ASCII) —
/// those are silently dropped rather than surfaced as a bogus name.
/// Returns `(offset_of_name, name)` per record. The offset is of the
/// *name text*, not the record start — that is what a [`VirtualFixup`]'s
/// `class_name_offset` points at, verified against the corpus.
fn parse_classname_records(content: &[u8]) -> Vec<(u32, String)> {
    let mut names = Vec::new();
    let mut pos = 0usize;
    while pos + CLASSNAME_RECORD_PREFIX_LEN < content.len() {
        let name_start = pos + CLASSNAME_RECORD_PREFIX_LEN;
        let Some(nul) = content[name_start..].iter().position(|&b| b == 0) else {
            break;
        };
        let name_bytes = &content[name_start..name_start + nul];
        if !name_bytes.is_empty() && name_bytes.iter().all(|&b| (0x20..0x7f).contains(&b)) {
            names.push((
                name_start as u32,
                String::from_utf8_lossy(name_bytes).into_owned(),
            ));
        } else {
            break;
        }
        pos = name_start + nul + 1;
    }
    names
}

/// Read a fixup table, stopping at the first `0xFFFFFFFF` sentinel (real
/// blobs pad a table's tail to its declared span) or at `end`.
fn read_local_fixups(data: &[u8], start: usize, end: usize) -> Vec<LocalFixup> {
    let mut out = Vec::new();
    let mut p = start;
    while p + LOCAL_FIXUP_SIZE <= end.min(data.len()) {
        let (src, dst) = (
            u32::from_le_bytes(data[p..p + 4].try_into().unwrap()),
            u32::from_le_bytes(data[p + 4..p + 8].try_into().unwrap()),
        );
        if src == u32::MAX {
            break;
        }
        out.push(LocalFixup {
            src_offset: src,
            dst_offset: dst,
        });
        p += LOCAL_FIXUP_SIZE;
    }
    out
}

fn read_global_fixups(data: &[u8], start: usize, end: usize) -> Vec<GlobalFixup> {
    let mut out = Vec::new();
    let mut p = start;
    while p + GLOBAL_FIXUP_SIZE <= end.min(data.len()) {
        let src = u32::from_le_bytes(data[p..p + 4].try_into().unwrap());
        if src == u32::MAX {
            break;
        }
        out.push(GlobalFixup {
            src_offset: src,
            dst_section: u32::from_le_bytes(data[p + 4..p + 8].try_into().unwrap()),
            dst_offset: u32::from_le_bytes(data[p + 8..p + 12].try_into().unwrap()),
        });
        p += GLOBAL_FIXUP_SIZE;
    }
    out
}

fn read_virtual_fixups(data: &[u8], start: usize, end: usize) -> Vec<VirtualFixup> {
    let mut out = Vec::new();
    let mut p = start;
    while p + GLOBAL_FIXUP_SIZE <= end.min(data.len()) {
        let off = u32::from_le_bytes(data[p..p + 4].try_into().unwrap());
        if off == u32::MAX {
            break;
        }
        out.push(VirtualFixup {
            data_offset: off,
            class_section: u32::from_le_bytes(data[p + 4..p + 8].try_into().unwrap()),
            class_name_offset: u32::from_le_bytes(data[p + 8..p + 12].try_into().unwrap()),
        });
        p += GLOBAL_FIXUP_SIZE;
    }
    out
}

/// Parse the outer container of a [`BhkSystemBinary`](super::BhkSystemBinary)
/// blob's raw `data`. Returns `Err` if the magic doesn't match or the
/// buffer is too short for the section table it declares — callers
/// should treat that as "not a recognized classic Havok packfile"
/// rather than a hard failure (other Havok/game versions may use a
/// different container).
pub fn parse_havok_packfile(data: &[u8]) -> io::Result<HavokPackfile> {
    if data.len() < SECTION_TABLE_START {
        return Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "havok packfile: too short for header",
        ));
    }
    let magic0 = read_u32_le(data, 0)?;
    let magic1 = read_u32_le(data, 4)?;
    if [magic0, magic1] != HAVOK_PACKFILE_MAGIC {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "havok packfile: magic mismatch",
        ));
    }
    let user_tag = read_u32_le(data, 8)?;
    let file_version = read_u32_le(data, 12)?;
    let layout_rules = [data[16], data[17], data[18], data[19]];
    let num_sections = read_u32_le(data, 20)?;
    let reserved_after_num_sections = read_u32_le(data, 24)?;
    let reserved_before_version_string = read_u32_le(data, 36)?;
    let contents_version = read_cstr(data, 40, 16)?;
    let reserved_after_version_string = read_u32_le(data, 60)?;

    let mut sections = Vec::with_capacity(num_sections as usize);
    for i in 0..num_sections as usize {
        let base = SECTION_TABLE_START + i * SECTION_HEADER_SIZE;
        if base + SECTION_HEADER_SIZE > data.len() {
            break;
        }
        let name = read_cstr(data, base, SECTION_NAME_FIELD_LEN)?;
        let f = base + 20; // past the 19-byte name field + 0xFF terminator
        let absolute_data_start = read_u32_le(data, f)?;
        let local_fixups_offset = read_u32_le(data, f + 4)?;
        let global_fixups_offset = read_u32_le(data, f + 8)?;
        let virtual_fixups_offset = read_u32_le(data, f + 12)?;
        let exports_offset = read_u32_le(data, f + 16)?;

        // The three fixup tables sit back-to-back between their declared
        // offsets. Each table is read only up to the number of whole
        // entries its declared byte span holds — real blobs pad the tail
        // of a table with `0xFFFFFFFF`, which `read_fixups` stops on.
        let base_abs = absolute_data_start as usize;
        let local_fixups = read_local_fixups(
            data,
            base_abs + local_fixups_offset as usize,
            base_abs + global_fixups_offset as usize,
        );
        let global_fixups = read_global_fixups(
            data,
            base_abs + global_fixups_offset as usize,
            base_abs + virtual_fixups_offset as usize,
        );
        let virtual_fixups = read_virtual_fixups(
            data,
            base_abs + virtual_fixups_offset as usize,
            base_abs + exports_offset as usize,
        );

        sections.push(PackfileSection {
            name,
            absolute_data_start,
            local_fixups_offset,
            global_fixups_offset,
            virtual_fixups_offset,
            exports_offset,
            imports_offset: read_u32_le(data, f + 20)?,
            end_offset: read_u32_le(data, f + 24)?,
            local_fixups,
            global_fixups,
            virtual_fixups,
        });
    }

    let classnames_section = sections.iter().find(|s| s.name == "__classnames__");
    let class_name_records = classnames_section
        .map(|s| parse_classname_records(data.get(s.content_range()).unwrap_or(&[])))
        .unwrap_or_default();
    let class_name_offsets = class_name_records.iter().map(|(o, _)| *o).collect();
    let class_names = class_name_records.into_iter().map(|(_, n)| n).collect();

    Ok(HavokPackfile {
        header: HavokPackfileHeader {
            user_tag,
            file_version,
            layout_rules,
            num_sections,
            reserved_after_num_sections,
            contents_version,
            reserved_before_version_string,
            reserved_after_version_string,
        },
        sections,
        class_names,
        class_name_offsets,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Hand-built synthetic packfile matching the exact layout verified
    /// against the real FO4 corpus: header + 3-section table
    /// (`__classnames__` non-empty, `__types__` empty, `__data__`
    /// carrying a small opaque payload). No real game bytes are
    /// embedded — this project doesn't ship copyrighted game data in
    /// its test fixtures.
    fn build_synthetic_packfile(class_names: &[&str], data_payload: &[u8]) -> Vec<u8> {
        build_synthetic_packfile_with_fixups(class_names, data_payload, &[], &[], &[])
    }

    /// As above, but `__data__` also carries the three fixup tables. The
    /// real corpus lays them out back-to-back after the object payload —
    /// local (8 B/entry), then global, then virtual (12 B/entry each) —
    /// which is what the offsets below reproduce.
    fn build_synthetic_packfile_with_fixups(
        class_names: &[&str],
        data_payload: &[u8],
        locals: &[LocalFixup],
        globals: &[GlobalFixup],
        virtuals: &[VirtualFixup],
    ) -> Vec<u8> {
        let mut classnames_blob = Vec::new();
        for (i, name) in class_names.iter().enumerate() {
            // Arbitrary per-entry prefix bytes (real semantic unconfirmed,
            // see `CLASSNAME_RECORD_PREFIX_LEN`) — varied per index so a
            // bug that accidentally treats them as part of the name is
            // caught rather than coincidentally matching.
            classnames_blob.extend_from_slice(&[i as u8, 0x11, 0x22, 0x33, 0x44]);
            classnames_blob.extend_from_slice(name.as_bytes());
            classnames_blob.push(0);
        }

        let mut buf = vec![0u8; SECTION_TABLE_START];
        buf[0..4].copy_from_slice(&HAVOK_PACKFILE_MAGIC[0].to_le_bytes());
        buf[4..8].copy_from_slice(&HAVOK_PACKFILE_MAGIC[1].to_le_bytes());
        buf[8..12].copy_from_slice(&0u32.to_le_bytes()); // user_tag
        buf[12..16].copy_from_slice(&11u32.to_le_bytes()); // file_version
        buf[16..20].copy_from_slice(&[0x08, 0x01, 0x00, 0x01]); // layout_rules
        buf[20..24].copy_from_slice(&3u32.to_le_bytes()); // num_sections
        buf[24..28].copy_from_slice(&2u32.to_le_bytes()); // reserved
        buf[36..40].copy_from_slice(&0x4bu32.to_le_bytes()); // reserved
        let version = b"hk_2014.1.0-r1\0";
        buf[40..40 + version.len()].copy_from_slice(version);
        buf[60..64].copy_from_slice(&0x15u32.to_le_bytes()); // reserved

        let push_section = |buf: &mut Vec<u8>,
                            name: &str,
                            data_start: u32,
                            local_fixups: u32,
                            global_fixups: u32,
                            virtual_fixups: u32,
                            exports: u32,
                            imports: u32,
                            end: u32| {
            let base = buf.len();
            buf.resize(base + SECTION_HEADER_SIZE, 0xFF);
            buf[base..base + name.len()].copy_from_slice(name.as_bytes());
            for b in buf[base + name.len()..base + SECTION_NAME_FIELD_LEN].iter_mut() {
                *b = 0;
            }
            buf[base + 19] = 0xFF;
            let f = base + 20;
            buf[f..f + 4].copy_from_slice(&data_start.to_le_bytes());
            buf[f + 4..f + 8].copy_from_slice(&local_fixups.to_le_bytes());
            buf[f + 8..f + 12].copy_from_slice(&global_fixups.to_le_bytes());
            buf[f + 12..f + 16].copy_from_slice(&virtual_fixups.to_le_bytes());
            buf[f + 16..f + 20].copy_from_slice(&exports.to_le_bytes());
            buf[f + 20..f + 24].copy_from_slice(&imports.to_le_bytes());
            buf[f + 24..f + 28].copy_from_slice(&end.to_le_bytes());
        };

        // __classnames__: data_start right after the 3-entry section
        // table (0x40 + 3*64 = 0x100), local_fixups_offset == the
        // class-name blob's length (i.e. no embedded fixup entries in
        // this synthetic fixture — zero-length tables are legal, as
        // seen for `__types__` in every real sample).
        let classnames_start = (SECTION_TABLE_START + 3 * SECTION_HEADER_SIZE) as u32;
        let classnames_len = classnames_blob.len() as u32;
        push_section(
            &mut buf,
            "__classnames__",
            classnames_start,
            classnames_len,
            classnames_len,
            classnames_len,
            classnames_len,
            classnames_len,
            classnames_len,
        );

        // __types__: empty, starts where classnames ends.
        let types_start = classnames_start + classnames_len;
        push_section(&mut buf, "__types__", types_start, 0, 0, 0, 0, 0, 0);

        // __data__: starts where types ends (also classnames end,
        // since types is empty), carries `data_payload` then the three
        // fixup tables back-to-back, as the real corpus does.
        let data_start = types_start;
        let payload_len = data_payload.len() as u32;
        let local_end = payload_len + (locals.len() * LOCAL_FIXUP_SIZE) as u32;
        let global_end = local_end + (globals.len() * GLOBAL_FIXUP_SIZE) as u32;
        let virtual_end = global_end + (virtuals.len() * GLOBAL_FIXUP_SIZE) as u32;
        push_section(
            &mut buf,
            "__data__",
            data_start,
            payload_len,
            local_end,
            global_end,
            virtual_end,
            virtual_end,
            virtual_end,
        );

        buf.extend_from_slice(&classnames_blob);
        buf.extend_from_slice(data_payload);
        for l in locals {
            buf.extend_from_slice(&l.src_offset.to_le_bytes());
            buf.extend_from_slice(&l.dst_offset.to_le_bytes());
        }
        for g in globals {
            buf.extend_from_slice(&g.src_offset.to_le_bytes());
            buf.extend_from_slice(&g.dst_section.to_le_bytes());
            buf.extend_from_slice(&g.dst_offset.to_le_bytes());
        }
        for v in virtuals {
            buf.extend_from_slice(&v.data_offset.to_le_bytes());
            buf.extend_from_slice(&v.class_section.to_le_bytes());
            buf.extend_from_slice(&v.class_name_offset.to_le_bytes());
        }
        buf
    }

    /// Offset of a class name's *text* within the synthetic
    /// `__classnames__` blob — mirrors the builder's own record layout so
    /// a virtual fixup in a test can point at a real name.
    fn synthetic_class_name_offset(class_names: &[&str], want: &str) -> u32 {
        let mut pos = 0u32;
        for name in class_names {
            let text = pos + CLASSNAME_RECORD_PREFIX_LEN as u32;
            if *name == want {
                return text;
            }
            pos = text + name.len() as u32 + 1;
        }
        panic!("{want} not in fixture");
    }

    #[test]
    fn decodes_the_three_fixup_tables() {
        let classes = ["hknpPhysicsSystemData", "hknpCompressedMeshShapeData"];
        let locals = [
            LocalFixup {
                src_offset: 16,
                dst_offset: 128,
            },
            LocalFixup {
                src_offset: 64,
                dst_offset: 208,
            },
        ];
        let globals = [GlobalFixup {
            src_offset: 208,
            dst_section: 2,
            dst_offset: 320,
        }];
        let virtuals = [
            VirtualFixup {
                data_offset: 0,
                class_section: 0,
                class_name_offset: synthetic_class_name_offset(&classes, "hknpPhysicsSystemData"),
            },
            VirtualFixup {
                data_offset: 320,
                class_section: 0,
                class_name_offset: synthetic_class_name_offset(
                    &classes,
                    "hknpCompressedMeshShapeData",
                ),
            },
        ];
        let blob = build_synthetic_packfile_with_fixups(
            &classes,
            &[0u8; 512],
            &locals,
            &globals,
            &virtuals,
        );
        let pf = parse_havok_packfile(&blob).expect("parse");
        let data = pf.section("__data__").expect("__data__");
        assert_eq!(data.local_fixups, locals);
        assert_eq!(data.global_fixups, globals);
        assert_eq!(data.virtual_fixups, virtuals);
    }

    /// The point of the virtual-fixup table: it turns the `__data__`
    /// section from opaque bytes into located, named objects.
    #[test]
    fn virtual_fixups_resolve_to_located_typed_objects() {
        let classes = ["hknpPhysicsSystemData", "hknpCompressedMeshShapeData"];
        let virtuals = [
            VirtualFixup {
                data_offset: 0,
                class_section: 0,
                class_name_offset: synthetic_class_name_offset(&classes, "hknpPhysicsSystemData"),
            },
            VirtualFixup {
                data_offset: 320,
                class_section: 0,
                class_name_offset: synthetic_class_name_offset(
                    &classes,
                    "hknpCompressedMeshShapeData",
                ),
            },
        ];
        let blob = build_synthetic_packfile_with_fixups(&classes, &[0u8; 512], &[], &[], &virtuals);
        let pf = parse_havok_packfile(&blob).expect("parse");
        let objects = pf.objects();
        assert_eq!(objects.len(), 2);
        assert_eq!(objects[0].class_name, "hknpPhysicsSystemData");
        assert_eq!(objects[0].section_offset, 0);
        assert_eq!(objects[1].class_name, "hknpCompressedMeshShapeData");
        assert_eq!(objects[1].section_offset, 320);

        let data_start = pf.section("__data__").unwrap().absolute_data_start as usize;
        assert_eq!(objects[1].absolute_offset, data_start + 320);
        // The absolute offset must actually address the blob, not run past it.
        assert!(objects[1].absolute_offset < blob.len());
    }

    /// A fixup table's declared span may exceed the entries it holds —
    /// real blobs pad the tail with `0xFFFFFFFF`. The reader must stop at
    /// the sentinel rather than emit a bogus entry.
    #[test]
    fn fixup_tables_stop_at_the_padding_sentinel() {
        let classes = ["hkClass"];
        let virtuals = [VirtualFixup {
            data_offset: 0,
            class_section: 0,
            class_name_offset: synthetic_class_name_offset(&classes, "hkClass"),
        }];
        let mut blob =
            build_synthetic_packfile_with_fixups(&classes, &[0u8; 64], &[], &[], &virtuals);
        // Widen __data__'s virtual-fixup span by one entry's worth of
        // padding, exactly as a real blob's tail looks.
        let data_base = SECTION_TABLE_START + 2 * SECTION_HEADER_SIZE + 20;
        let virtual_end =
            u32::from_le_bytes(blob[data_base + 16..data_base + 20].try_into().unwrap());
        let widened = virtual_end + GLOBAL_FIXUP_SIZE as u32;
        for off in [16usize, 20, 24] {
            blob[data_base + off..data_base + off + 4].copy_from_slice(&widened.to_le_bytes());
        }
        blob.extend_from_slice(&[0xFFu8; GLOBAL_FIXUP_SIZE]);

        let pf = parse_havok_packfile(&blob).expect("parse");
        assert_eq!(
            pf.section("__data__").unwrap().virtual_fixups.len(),
            1,
            "the 0xFFFFFFFF pad must not decode as a sixth object"
        );
        assert_eq!(pf.objects().len(), 1);
    }

    /// A virtual fixup naming an offset no class record starts at is
    /// dropped rather than surfaced as an object with a garbage name.
    #[test]
    fn an_unresolvable_class_offset_yields_no_object() {
        let classes = ["hkClass"];
        let virtuals = [VirtualFixup {
            data_offset: 0,
            class_section: 0,
            class_name_offset: 9999,
        }];
        let blob = build_synthetic_packfile_with_fixups(&classes, &[0u8; 64], &[], &[], &virtuals);
        let pf = parse_havok_packfile(&blob).expect("parse");
        assert_eq!(pf.section("__data__").unwrap().virtual_fixups.len(), 1);
        assert!(pf.objects().is_empty());
        assert!(pf.class_name_at(9999).is_none());
    }

    #[test]
    fn decodes_header_and_section_table() {
        let blob = build_synthetic_packfile(
            &["hkClass", "hknpCompressedMeshShapeData"],
            &[0xAA, 0xBB, 0xCC, 0xDD],
        );
        let pf = parse_havok_packfile(&blob).expect("parse");
        assert_eq!(pf.header.file_version, 11);
        assert_eq!(pf.header.contents_version, "hk_2014.1.0-r1");
        assert_eq!(pf.header.num_sections, 3);
        assert_eq!(pf.sections.len(), 3);
        assert_eq!(pf.sections[0].name, "__classnames__");
        assert_eq!(pf.sections[1].name, "__types__");
        assert_eq!(pf.sections[2].name, "__data__");
    }

    #[test]
    fn extracts_class_names() {
        let blob = build_synthetic_packfile(
            &[
                "hkClass",
                "hknpPhysicsSystemData",
                "hknpCompressedMeshShapeData",
            ],
            &[0x11, 0x22],
        );
        let pf = parse_havok_packfile(&blob).expect("parse");
        assert_eq!(
            pf.class_names,
            vec![
                "hkClass",
                "hknpPhysicsSystemData",
                "hknpCompressedMeshShapeData"
            ]
        );
        assert!(pf.has_class("hknpCompressedMeshShapeData"));
        assert!(!pf.has_class("hkpRigidBody"));
    }

    #[test]
    fn types_section_is_empty_and_data_section_spans_full_payload() {
        let payload = vec![0u8; 128];
        let blob = build_synthetic_packfile(&["hkClass"], &payload);
        let pf = parse_havok_packfile(&blob).expect("parse");
        let types = pf.section("__types__").unwrap();
        assert_eq!(types.content_range().len(), 0);

        let data = pf.section("__data__").unwrap();
        assert_eq!(data.absolute_end() as usize, blob.len());
        assert_eq!(&blob[data.content_range()], payload.as_slice());
    }

    #[test]
    fn rejects_bad_magic() {
        let mut blob = build_synthetic_packfile(&["hkClass"], &[]);
        blob[0] = 0x00;
        assert!(parse_havok_packfile(&blob).is_err());
    }

    #[test]
    fn rejects_truncated_header() {
        let blob = build_synthetic_packfile(&["hkClass"], &[]);
        assert!(parse_havok_packfile(&blob[..0x20]).is_err());
    }
}
