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

    /// Resolve a pointer whose target lives in a *different* section.
    ///
    /// Unused by the two object types this crate decodes today: `hkaSkeleton`,
    /// `hkaSplineCompressedAnimation` and `hkaAnimationBinding` all reference
    /// same-section data, which [`Self::local_target`] handles. It is kept —
    /// and exercised by the tests below rather than left as silent
    /// scaffolding (#2267) — because the global fixup table is part of the
    /// packfile format this reader already parses and validates, so the
    /// accessor is the read half of data we decode either way. The first
    /// object type with a pointer that crosses a section boundary is its
    /// consumer.
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

#[cfg(test)]
mod tests {
    use super::*;

    const HEADER: usize = HEADER_SIZE;
    const SECTION_TABLE: usize = 2 * SECTION_HEADER_SIZE;

    /// Assembles a minimal but *structurally real* 64-bit little-endian
    /// Havok 2010 packfile: a `__classnames__` section holding the class
    /// name strings, and a `__data__` section carrying object bytes followed
    /// by the local / global / virtual fixup tables in the order the format
    /// mandates.
    #[derive(Default)]
    struct PackfileBuilder {
        classnames: Vec<u8>,
        data: Vec<u8>,
        local: Vec<(u32, u32)>,
        global: Vec<(u32, u32, u32)>,
        virtual_fixups: Vec<(u32, u32, u32)>,
    }

    impl PackfileBuilder {
        /// Append a NUL-terminated class name, returning its offset within
        /// the classnames section.
        fn class(&mut self, name: &str) -> u32 {
            let offset = self.classnames.len() as u32;
            self.classnames.extend_from_slice(name.as_bytes());
            self.classnames.push(0);
            offset
        }

        fn build(&self) -> Vec<u8> {
            self.build_with(|_| {})
        }

        /// `tweak` gets the finished buffer so a test can corrupt one field
        /// without hand-rolling the whole layout again.
        fn build_with(&self, tweak: impl FnOnce(&mut Vec<u8>)) -> Vec<u8> {
            let names_start = HEADER + SECTION_TABLE;
            let names_len = self.classnames.len();
            let data_start = names_start + names_len;

            let local_rel = self.data.len();
            let global_rel = local_rel + self.local.len() * 8;
            let virtual_rel = global_rel + self.global.len() * 12;
            let exports_rel = virtual_rel + self.virtual_fixups.len() * 12;

            let mut bytes = vec![0u8; names_start];
            bytes[0..4].copy_from_slice(&MAGIC_0.to_le_bytes());
            bytes[4..8].copy_from_slice(&MAGIC_1.to_le_bytes());
            bytes[0x10] = 8; // 64-bit pointers
            bytes[0x11] = 1; // little endian
            bytes[0x14..0x18].copy_from_slice(&2u32.to_le_bytes());

            let section = |bytes: &mut Vec<u8>,
                           index: usize,
                           tag: &str,
                           start: usize,
                           local: usize,
                           global: usize,
                           virt: usize,
                           exports: usize,
                           end: usize| {
                let base = HEADER + index * SECTION_HEADER_SIZE;
                bytes[base..base + tag.len()].copy_from_slice(tag.as_bytes());
                let put = |bytes: &mut Vec<u8>, field: usize, value: usize| {
                    bytes[base + field..base + field + 4]
                        .copy_from_slice(&(value as u32).to_le_bytes());
                };
                put(bytes, 20, start);
                put(bytes, 24, local);
                put(bytes, 28, global);
                put(bytes, 32, virt);
                put(bytes, 36, exports);
                put(bytes, 44, end);
            };
            // Section offsets past `start` are all section-relative.
            section(
                &mut bytes,
                0,
                "__classnames__",
                names_start,
                names_len,
                names_len,
                names_len,
                names_len,
                names_len,
            );
            section(
                &mut bytes,
                1,
                "__data__",
                data_start,
                local_rel,
                global_rel,
                virtual_rel,
                exports_rel,
                exports_rel,
            );

            bytes.extend_from_slice(&self.classnames);
            bytes.extend_from_slice(&self.data);
            for (source, target) in &self.local {
                bytes.extend_from_slice(&source.to_le_bytes());
                bytes.extend_from_slice(&target.to_le_bytes());
            }
            for (source, section, target) in &self.global {
                bytes.extend_from_slice(&source.to_le_bytes());
                bytes.extend_from_slice(&section.to_le_bytes());
                bytes.extend_from_slice(&target.to_le_bytes());
            }
            for (source, section, offset) in &self.virtual_fixups {
                bytes.extend_from_slice(&source.to_le_bytes());
                bytes.extend_from_slice(&section.to_le_bytes());
                bytes.extend_from_slice(&offset.to_le_bytes());
            }
            tweak(&mut bytes);
            bytes
        }
    }

    /// `Packfile` carries a borrowed slice and derives neither `Debug` nor
    /// `PartialEq`, so rejection tests assert on the error alone.
    fn parse_err(bytes: &[u8]) -> HkxError {
        Packfile::parse(bytes).err().expect("must be rejected")
    }

    /// One object at data offset 0x20, a same-section pointer at 0x30, and a
    /// cross-section pointer at 0x40 — the three things every decode in
    /// `animation.rs` is built out of.
    fn sample() -> PackfileBuilder {
        let mut builder = PackfileBuilder {
            data: vec![0u8; 0x60],
            ..Default::default()
        };
        let class = builder.class("hkaSkeleton");
        builder.local.push((0x30, 0x50));
        builder.global.push((0x40, 0, 4));
        builder.virtual_fixups.push((0x20, 0, class));
        builder
    }

    #[test]
    fn parses_sections_fixups_and_objects() {
        let bytes = sample().build();
        let pack = Packfile::parse(&bytes).unwrap();
        assert_eq!(pack.object("hkaSkeleton").unwrap(), 0x20);
        assert_eq!(pack.local_target(0x30), Some(0x50));
    }

    /// #2267 — `global_target` had zero call sites and zero coverage. Cross-
    /// section fixups carry a *section index* alongside the offset, which is
    /// exactly what distinguishes them from local ones.
    #[test]
    fn global_target_resolves_the_section_and_offset() {
        let bytes = sample().build();
        let pack = Packfile::parse(&bytes).unwrap();
        assert_eq!(pack.global_target(0x40), Some((0, 4)));
        assert_eq!(
            pack.global_target(0x30),
            None,
            "a local fixup source must not resolve as a global one"
        );
        assert_eq!(pack.global_target(0x1234), None);
    }

    #[test]
    fn unmapped_local_sources_do_not_resolve() {
        let bytes = sample().build();
        let pack = Packfile::parse(&bytes).unwrap();
        assert_eq!(pack.local_target(0x40), None);
        assert_eq!(pack.local_target(0x1234), None);
    }

    /// Havok terminates a fixup table with an all-ones source; entries after
    /// it are padding, not data.
    #[test]
    fn fixup_walk_stops_at_the_terminator() {
        let mut builder = sample();
        builder.local.clear();
        builder.local.push((0x30, 0x50));
        builder.local.push((u32::MAX, u32::MAX));
        builder.local.push((0x38, 0x58));
        let bytes = builder.build();
        let pack = Packfile::parse(&bytes).unwrap();
        assert_eq!(pack.local_target(0x30), Some(0x50));
        assert_eq!(
            pack.local_target(0x38),
            None,
            "entries past the terminator are padding"
        );
    }

    #[test]
    fn missing_class_is_named_in_the_error() {
        let bytes = sample().build();
        let pack = Packfile::parse(&bytes).unwrap();
        assert_eq!(
            pack.object("hkaSplineCompressedAnimation"),
            Err(HkxError::MissingClass("hkaSplineCompressedAnimation"))
        );
    }

    #[test]
    fn rejects_a_non_packfile() {
        assert_eq!(parse_err(&[]), (HkxError::Truncated("HKX magic")));
        let mut bytes = sample().build();
        bytes[0] ^= 0xFF;
        assert_eq!(parse_err(&bytes), (HkxError::InvalidMagic));
    }

    /// The crate deliberately supports one layout — Skyrim SE's. A 32-bit or
    /// big-endian packfile must be refused, not misread.
    #[test]
    fn rejects_layouts_other_than_64_bit_little_endian() {
        for (offset, value) in [(0x10, 4u8), (0x11, 0u8)] {
            let bytes = sample().build_with(|bytes| bytes[offset] = value);
            assert_eq!(
                parse_err(&bytes),
                (HkxError::UnsupportedLayout("expected a 64-bit little-endian packfile"))
            );
        }
    }

    #[test]
    fn rejects_an_implausible_section_count() {
        for count in [0u32, 65] {
            let bytes = sample()
                .build_with(|bytes| bytes[0x14..0x18].copy_from_slice(&count.to_le_bytes()));
            assert_eq!(
                parse_err(&bytes),
                (HkxError::InvalidData("implausible section count"))
            );
        }
    }

    #[test]
    fn rejects_a_truncated_section_table() {
        let bytes = sample().build();
        assert_eq!(
            parse_err(&bytes[..HEADER + SECTION_HEADER_SIZE]),
            (HkxError::Truncated("section table"))
        );
    }

    /// The fixup tables are stored in a fixed order; a section header that
    /// claims otherwise would make the walkers read one table as another.
    #[test]
    fn rejects_out_of_order_fixup_tables() {
        // Swap the data section's local and global fixup offsets.
        let bytes = sample().build_with(|bytes| {
            let base = HEADER + SECTION_HEADER_SIZE;
            let local: [u8; 4] = bytes[base + 24..base + 28].try_into().unwrap();
            let global: [u8; 4] = bytes[base + 28..base + 32].try_into().unwrap();
            bytes[base + 24..base + 28].copy_from_slice(&global);
            bytes[base + 28..base + 32].copy_from_slice(&local);
        });
        assert_eq!(
            parse_err(&bytes),
            (HkxError::InvalidData("global fixups precede local fixups"))
        );
    }

    #[test]
    fn rejects_a_missing_data_section() {
        let bytes = sample().build_with(|bytes| {
            let base = HEADER + SECTION_HEADER_SIZE;
            bytes[base..base + 8].copy_from_slice(b"__misc__");
        });
        assert_eq!(parse_err(&bytes), (HkxError::MissingSection("__data__")));
    }

    #[test]
    fn rejects_a_global_fixup_naming_a_section_that_does_not_exist() {
        let mut builder = sample();
        builder.global.clear();
        builder.global.push((0x40, 7, 4));
        let bytes = builder.build();
        assert_eq!(
            parse_err(&bytes),
            (HkxError::InvalidData("global fixup section out of range"))
        );
    }

    /// `data_slice` is the choke point every typed accessor goes through: it
    /// must not hand out bytes past the object data, where the fixup tables
    /// live.
    #[test]
    fn data_reads_stop_at_the_end_of_the_object_region() {
        let mut builder = sample();
        builder.data[0x10..0x14].copy_from_slice(&0xDEAD_BEEFu32.to_le_bytes());
        builder.data[0x14..0x18].copy_from_slice(&1.5f32.to_bits().to_le_bytes());
        builder.data[0x18..0x1D].copy_from_slice(b"bone\0");
        let bytes = builder.build();
        let pack = Packfile::parse(&bytes).unwrap();

        assert_eq!(pack.u32(0x10, "probe").unwrap(), 0xDEAD_BEEF);
        assert_eq!(pack.f32(0x14, "probe").unwrap(), 1.5);
        assert_eq!(pack.cstr(0x18, "probe").unwrap(), "bone");
        assert_eq!(pack.data_slice(0x10, 4, "probe").unwrap().len(), 4);

        // 0x60 is the first byte of the local fixup table.
        assert_eq!(
            pack.u32(0x60, "probe"),
            Err(HkxError::Truncated("probe")),
            "a read must not walk off the object data into the fixup tables"
        );
        assert_eq!(
            pack.data_slice(usize::MAX, 4, "probe"),
            Err(HkxError::InvalidData("data offset overflow"))
        );
    }
}
