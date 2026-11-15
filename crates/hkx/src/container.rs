//! The classic Havok binary packfile container (#6954): the `0x40`-byte header, the section
//! table and each section's three fixup tables, validated once. This crate's object reader
//! (`packfile.rs`) and `byroredux-nif`'s `HavokPackfile` (Fallout 4 `bhkSystemBinary` blobs) both
//! read through it, so a hardening fix to the container lands in both.

use crate::{HkxError, Result};

pub(crate) const MAGIC_0: u32 = 0x57e0_e057;
pub(crate) const MAGIC_1: u32 = 0x10c0_c010;
pub(crate) const HEADER_SIZE: usize = 0x40;
pub(crate) const SECTION_HEADER_SIZE: usize = 0x30;
/// Havok 2014 (file version 11) pads each section header to 0x40 bytes.
pub(crate) const SECTION_HEADER_SIZE_V11: usize = 0x40;
/// The table stride of a local fixup (two `u32`) and of a global or virtual fixup (three).
const LOCAL_FIXUP_SIZE: usize = 8;
const GLOBAL_FIXUP_SIZE: usize = 12;

/// The `hkPackfileHeader` fields the container reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackfileHeader {
    pub user_tag: u32,
    /// `m_fileVersion`: 8 (Havok 2010, Skyrim) or 11 (Havok 2014, Fallout 4).
    pub file_version: u32,
    /// `m_layoutRules`: bytes in a pointer, little-endian, reuse-padding, empty-base-class.
    pub layout_rules: [u8; 4],
    pub section_count: u32,
    /// `m_contentsVersion`, e.g. `hk_2014.1.0-r1`.
    pub contents_version: String,
}

/// One section-table entry, every offset absolute and validated in order
/// (`start <= local_fixups <= global_fixups <= virtual_fixups <= exports <= end <= file length`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Section {
    pub tag: String,
    pub start: usize,
    pub local_fixups: usize,
    pub global_fixups: usize,
    pub virtual_fixups: usize,
    pub exports: usize,
    pub imports: usize,
    pub end: usize,
}

/// A validated packfile: its header and section table over the file's bytes.
pub struct Container<'a> {
    pub bytes: &'a [u8],
    pub header: PackfileHeader,
    pub sections: Vec<Section>,
}

impl<'a> Container<'a> {
    pub fn parse(bytes: &'a [u8]) -> Result<Self> {
        if read_u32(bytes, 0, "HKX magic")? != MAGIC_0
            || read_u32(bytes, 4, "HKX magic")? != MAGIC_1
        {
            return Err(HkxError::InvalidMagic);
        }
        let pointer_size = *bytes.get(0x10).ok_or(HkxError::Truncated("layout rules"))?;
        let little_endian = *bytes.get(0x11).ok_or(HkxError::Truncated("layout rules"))?;
        // Skyrim ships both: the 2011 release's packfiles are 32-bit, Special
        // Edition re-exported the same `hk_2010.2.0-r1` content as 64-bit.
        // The header, section table and fixup tables are fixed-width either
        // way; only object field offsets depend on this byte, and
        // `animation.rs` derives those from it.
        if !matches!(pointer_size, 4 | 8) || little_endian != 1 {
            return Err(HkxError::UnsupportedLayout(
                "expected a 32- or 64-bit little-endian packfile",
            ));
        }
        // #4332 — pointer width and endianness are not the whole layout.
        // `animation.rs`'s `Layout` also assumes Havok 2010 class field order
        // and MSVC struct packing, and neither byte above confirms either: an
        // `hk_2014` file, or one written with tail-padding reuse (which
        // shifts every derived member of `hkReferencedObject`), would pass
        // and decode with wrong offsets. The header states all three
        // (`hkPackfileHeader`: `m_fileVersion` @0x0C, `m_layoutRules[2]` =
        // `m_reusePaddingOptimization` @0x12, `m_contentsVersion` @0x28).
        // Every sampled vanilla Skyrim LE and SE packfile is version 8,
        // `hk_2010.2.0-r1`, reuse-padding 0.
        let version = read_u32(bytes, 0x0C, "file version")?;
        if version != 8 && version != 11 {
            return Err(HkxError::UnsupportedLayout(
                "expected a version-8 (Havok 2010) or version-11 (Havok 2014) packfile",
            ));
        }
        let layout_rules: [u8; 4] = bytes
            .get(0x10..0x14)
            .ok_or(HkxError::Truncated("layout rules"))?
            .try_into()
            .unwrap();
        if layout_rules[2] != 0 {
            return Err(HkxError::UnsupportedLayout(
                "expected MSVC struct layout without tail-padding reuse",
            ));
        }
        let contents_version = bytes
            .get(0x28..0x38)
            .ok_or(HkxError::Truncated("contents version"))?;
        let expected: &[u8] = if version == 8 { b"hk_2010" } else { b"hk_2014" };
        if !contents_version.starts_with(expected) {
            return Err(HkxError::UnsupportedLayout(
                "packfile contents do not match its file version",
            ));
        }
        let contents_end = contents_version.iter().position(|byte| *byte == 0).unwrap_or(16);
        let contents_version = String::from_utf8_lossy(&contents_version[..contents_end]).into_owned();
        let section_header_size = if version == 8 { SECTION_HEADER_SIZE } else { SECTION_HEADER_SIZE_V11 };

        let section_count = read_u32(bytes, 0x14, "section count")?;
        // #4624 — bounded before anything is sized from it.
        if section_count == 0 || section_count > 64 {
            return Err(HkxError::InvalidData("implausible section count"));
        }
        let section_count_usize = section_count as usize;
        // Havok 2014 follows the 0x40-byte header with a predicate area
        // (`m_predicateArraySizePlusPadding`, u16 @0x3E) before the section table;
        // the animation clips of Fallout 4 carry one (0x10 bytes), its behavior
        // graphs none.
        let table_start = if version == 11 {
            HEADER_SIZE
                + usize::from(u16::from_le_bytes(
                    bytes
                        .get(0x3E..0x40)
                        .ok_or(HkxError::Truncated("predicate area size"))?
                        .try_into()
                        .unwrap(),
                ))
        } else {
            HEADER_SIZE
        };
        let section_table_bytes = section_count_usize
            .checked_mul(section_header_size)
            .ok_or(HkxError::InvalidData("section table size overflow"))?;
        let section_table_end = table_start
            .checked_add(section_table_bytes)
            .ok_or(HkxError::InvalidData("section table overflow"))?;
        if section_table_end > bytes.len() {
            return Err(HkxError::Truncated("section table"));
        }

        let mut sections = Vec::with_capacity(section_count_usize);
        for index in 0..section_count_usize {
            let base = table_start + index * section_header_size;
            let tag_bytes = &bytes[base..base + 19];
            let tag_end = tag_bytes.iter().position(|byte| *byte == 0).unwrap_or(19);
            let tag = String::from_utf8_lossy(&tag_bytes[..tag_end]).into_owned();
            let start = read_u32(bytes, base + 20, "section data start")? as usize;
            let relative = |field, label| -> Result<usize> {
                start
                    .checked_add(read_u32(bytes, base + field, label)? as usize)
                    .ok_or(HkxError::InvalidData("section offset overflow"))
            };
            let section = Section {
                tag,
                start,
                local_fixups: relative(24, "local fixups")?,
                global_fixups: relative(28, "global fixups")?,
                virtual_fixups: relative(32, "virtual fixups")?,
                exports: relative(36, "exports")?,
                imports: relative(40, "imports")?,
                end: relative(44, "section end")?,
            };
            if section.start > section.local_fixups {
                return Err(HkxError::InvalidData("local fixups precede section data"));
            }
            if section.local_fixups > section.global_fixups {
                return Err(HkxError::InvalidData("global fixups precede local fixups"));
            }
            if section.global_fixups > section.virtual_fixups {
                return Err(HkxError::InvalidData(
                    "virtual fixups precede global fixups",
                ));
            }
            if section.virtual_fixups > section.exports {
                return Err(HkxError::InvalidData("exports precede virtual fixups"));
            }
            if section.exports > section.end {
                return Err(HkxError::InvalidData("section end precedes exports"));
            }
            if section.end > bytes.len() {
                return Err(HkxError::InvalidData("section end exceeds file"));
            }
            sections.push(section);
        }

        Ok(Self {
            bytes,
            header: PackfileHeader {
                user_tag: read_u32(bytes, 0x08, "user tag")?,
                file_version: version,
                layout_rules,
                section_count,
                contents_version,
            },
            sections,
        })
    }

    /// Width of a pointer in the file's object data: 4 or 8.
    pub fn pointer_size(&self) -> usize {
        usize::from(self.header.layout_rules[0])
    }

    /// A header word the container does not interpret, read raw (`None` past the header).
    pub fn raw_header_u32(&self, offset: usize) -> Option<u32> {
        (offset + 4 <= HEADER_SIZE).then(|| read_u32(self.bytes, offset, "header word").ok()).flatten()
    }

    /// `section`'s local fixups, `(source, target)` relative to its start, in table order up to
    /// the `0xFFFFFFFF` padding sentinel.
    pub fn local_fixups(&self, section: &Section) -> Result<Vec<(u32, u32)>> {
        let mut fixups = Vec::new();
        let mut cursor = section.local_fixups;
        while cursor + LOCAL_FIXUP_SIZE <= section.global_fixups {
            let source = read_u32(self.bytes, cursor, "local fixup source")?;
            let target = read_u32(self.bytes, cursor + 4, "local fixup target")?;
            if source == u32::MAX {
                break;
            }
            fixups.push((source, target));
            cursor += LOCAL_FIXUP_SIZE;
        }
        Ok(fixups)
    }

    /// `section`'s global fixups, `(source, target section index, target)`, in table order up to
    /// the sentinel. A target section past the table is an error.
    pub fn global_fixups(&self, section: &Section) -> Result<Vec<(u32, u32, u32)>> {
        let mut fixups = Vec::new();
        let mut cursor = section.global_fixups;
        while cursor + GLOBAL_FIXUP_SIZE <= section.virtual_fixups {
            let source = read_u32(self.bytes, cursor, "global fixup source")?;
            let target_section = read_u32(self.bytes, cursor + 4, "global fixup section")?;
            let target = read_u32(self.bytes, cursor + 8, "global fixup target")?;
            if source == u32::MAX {
                break;
            }
            if target_section as usize >= self.sections.len() {
                return Err(HkxError::InvalidData("global fixup section out of range"));
            }
            fixups.push((source, target_section, target));
            cursor += GLOBAL_FIXUP_SIZE;
        }
        Ok(fixups)
    }

    /// `section`'s virtual fixups, `(object offset, class-name section index, class-name offset)`,
    /// in table order up to the sentinel. A class-name section past the table is an error; the
    /// name itself is resolved by the reader.
    pub fn virtual_fixups(&self, section: &Section) -> Result<Vec<(u32, u32, u32)>> {
        let mut fixups = Vec::new();
        let mut cursor = section.virtual_fixups;
        while cursor + GLOBAL_FIXUP_SIZE <= section.exports {
            let source = read_u32(self.bytes, cursor, "virtual fixup source")?;
            let class_section = read_u32(self.bytes, cursor + 4, "virtual fixup section")?;
            let class_offset = read_u32(self.bytes, cursor + 8, "virtual fixup class")?;
            if source == u32::MAX {
                break;
            }
            if class_section as usize >= self.sections.len() {
                return Err(HkxError::InvalidData("class-name section out of range"));
            }
            fixups.push((source, class_section, class_offset));
            cursor += GLOBAL_FIXUP_SIZE;
        }
        Ok(fixups)
    }
}

pub(crate) fn read_u32(bytes: &[u8], offset: usize, label: &'static str) -> Result<u32> {
    let end = offset
        .checked_add(4)
        .ok_or(HkxError::InvalidData("integer offset overflow"))?;
    let raw = bytes.get(offset..end).ok_or(HkxError::Truncated(label))?;
    Ok(u32::from_le_bytes(raw.try_into().unwrap()))
}
