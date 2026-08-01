use crate::{HkxError, Result};

const MAGIC_0: u32 = 0x57e0_e057;
const MAGIC_1: u32 = 0x10c0_c010;
const HEADER_SIZE: usize = 0x40;
const SECTION_HEADER_SIZE: usize = 0x30;

#[derive(Debug, Clone)]
struct Section {
    tag: String,
    start: usize,
    local_fixups: usize,
    global_fixups: usize,
    virtual_fixups: usize,
    exports: usize,
    end: usize,
}

/// Resolved view of a Havok 2010 binary packfile.
pub(crate) struct Packfile<'a> {
    bytes: &'a [u8],
    sections: Vec<Section>,
    data_section: usize,
    local_fixups: Vec<(usize, usize)>,
    global_fixups: Vec<(usize, usize, usize)>,
    objects: Vec<(usize, String)>,
}

impl<'a> Packfile<'a> {
    pub(crate) fn parse(bytes: &'a [u8]) -> Result<Self> {
        if read_u32(bytes, 0, "HKX magic")? != MAGIC_0
            || read_u32(bytes, 4, "HKX magic")? != MAGIC_1
        {
            return Err(HkxError::InvalidMagic);
        }
        let pointer_size = *bytes.get(0x10).ok_or(HkxError::Truncated("layout rules"))?;
        let little_endian = *bytes.get(0x11).ok_or(HkxError::Truncated("layout rules"))?;
        if pointer_size != 8 || little_endian != 1 {
            return Err(HkxError::UnsupportedLayout(
                "expected a 64-bit little-endian packfile",
            ));
        }

        let section_count = read_u32(bytes, 0x14, "section count")? as usize;
        if section_count == 0 || section_count > 64 {
            return Err(HkxError::InvalidData("implausible section count"));
        }
        let section_table_bytes = section_count
            .checked_mul(SECTION_HEADER_SIZE)
            .ok_or(HkxError::InvalidData("section table size overflow"))?;
        let section_table_end = HEADER_SIZE
            .checked_add(section_table_bytes)
            .ok_or(HkxError::InvalidData("section table overflow"))?;
        if section_table_end > bytes.len() {
            return Err(HkxError::Truncated("section table"));
        }

        let mut sections = Vec::with_capacity(section_count);
        for index in 0..section_count {
            let base = HEADER_SIZE + index * SECTION_HEADER_SIZE;
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

        let data_section = sections
            .iter()
            .position(|section| section.tag == "__data__")
            .ok_or(HkxError::MissingSection("__data__"))?;
        let data = &sections[data_section];

        let mut local_fixups = Vec::new();
        let mut cursor = data.local_fixups;
        while cursor + 8 <= data.global_fixups {
            let source = read_u32(bytes, cursor, "local fixup source")?;
            let target = read_u32(bytes, cursor + 4, "local fixup target")?;
            if source == u32::MAX {
                break;
            }
            local_fixups.push((source as usize, target as usize));
            cursor += 8;
        }
        local_fixups.sort_unstable_by_key(|entry| entry.0);

        let mut global_fixups = Vec::new();
        cursor = data.global_fixups;
        while cursor + 12 <= data.virtual_fixups {
            let source = read_u32(bytes, cursor, "global fixup source")?;
            let target_section = read_u32(bytes, cursor + 4, "global fixup section")?;
            let target = read_u32(bytes, cursor + 8, "global fixup target")?;
            if source == u32::MAX {
                break;
            }
            if target_section as usize >= sections.len() {
                return Err(HkxError::InvalidData("global fixup section out of range"));
            }
            global_fixups.push((source as usize, target_section as usize, target as usize));
            cursor += 12;
        }
        global_fixups.sort_unstable_by_key(|entry| entry.0);

        let mut objects = Vec::new();
        cursor = data.virtual_fixups;
        while cursor + 12 <= data.exports {
            let source = read_u32(bytes, cursor, "virtual fixup source")?;
            let class_section = read_u32(bytes, cursor + 4, "virtual fixup section")?;
            let class_offset = read_u32(bytes, cursor + 8, "virtual fixup class")?;
            if source == u32::MAX {
                break;
            }
            let class_section = sections
                .get(class_section as usize)
                .ok_or(HkxError::InvalidData("class-name section out of range"))?;
            // Havok virtual fixups address the first byte of the
            // NUL-terminated class name inside the classname section.
            let name_start = class_section
                .start
                .checked_add(class_offset as usize)
                .ok_or(HkxError::InvalidData("class-name offset overflow"))?;
            let name = read_cstr(bytes, name_start, "class name")?.to_owned();
            objects.push((source as usize, name));
            cursor += 12;
        }

        Ok(Self {
            bytes,
            sections,
            data_section,
            local_fixups,
            global_fixups,
            objects,
        })
    }

    pub(crate) fn object(&self, class_name: &'static str) -> Result<usize> {
        self.objects
            .iter()
            .find_map(|(offset, class)| (class == class_name).then_some(*offset))
            .ok_or(HkxError::MissingClass(class_name))
    }

    pub(crate) fn local_target(&self, source: usize) -> Option<usize> {
        self.local_fixups
            .binary_search_by_key(&source, |entry| entry.0)
            .ok()
            .map(|index| self.local_fixups[index].1)
    }

    #[allow(dead_code)]
    pub(crate) fn global_target(&self, source: usize) -> Option<(usize, usize)> {
        self.global_fixups
            .binary_search_by_key(&source, |entry| entry.0)
            .ok()
            .map(|index| {
                let (_, section, target) = self.global_fixups[index];
                (section, target)
            })
    }

    pub(crate) fn data_slice(
        &self,
        relative_offset: usize,
        len: usize,
        label: &'static str,
    ) -> Result<&'a [u8]> {
        let section = &self.sections[self.data_section];
        let start = section
            .start
            .checked_add(relative_offset)
            .ok_or(HkxError::InvalidData("data offset overflow"))?;
        let end = start
            .checked_add(len)
            .ok_or(HkxError::InvalidData("data length overflow"))?;
        if end > section.local_fixups || end > self.bytes.len() {
            return Err(HkxError::Truncated(label));
        }
        Ok(&self.bytes[start..end])
    }

    pub(crate) fn u32(&self, relative_offset: usize, label: &'static str) -> Result<u32> {
        let data = self.data_slice(relative_offset, 4, label)?;
        Ok(u32::from_le_bytes(data.try_into().unwrap()))
    }

    pub(crate) fn f32(&self, relative_offset: usize, label: &'static str) -> Result<f32> {
        Ok(f32::from_bits(self.u32(relative_offset, label)?))
    }

    pub(crate) fn cstr(&self, relative_offset: usize, label: &'static str) -> Result<&'a str> {
        let section = &self.sections[self.data_section];
        let start = section
            .start
            .checked_add(relative_offset)
            .ok_or(HkxError::InvalidData("string offset overflow"))?;
        read_cstr(&self.bytes[..section.local_fixups], start, label)
    }
}

fn read_u32(bytes: &[u8], offset: usize, label: &'static str) -> Result<u32> {
    let end = offset
        .checked_add(4)
        .ok_or(HkxError::InvalidData("integer offset overflow"))?;
    let raw = bytes.get(offset..end).ok_or(HkxError::Truncated(label))?;
    Ok(u32::from_le_bytes(raw.try_into().unwrap()))
}

fn read_cstr<'a>(bytes: &'a [u8], offset: usize, label: &'static str) -> Result<&'a str> {
    let rest = bytes.get(offset..).ok_or(HkxError::Truncated(label))?;
    let len = rest
        .iter()
        .position(|byte| *byte == 0)
        .ok_or(HkxError::Truncated(label))?;
    std::str::from_utf8(&rest[..len]).map_err(|_| HkxError::InvalidData("non-UTF-8 string"))
}
