//! Low-level binary reader for the TES4 record format.
//!
//! ESM/ESP files are sequences of records and groups. Each record has a
//! 4-char type code, data size, flags, and form ID. Records contain
//! sub-records (type + size + data). Groups contain other records/groups.
//!
//! **Per-game header layout.** Oblivion (TES4) uses a 20-byte record
//! header and 20-byte group header, ending after `vc_info`. Every later
//! game (Fallout 3, New Vegas, Skyrim, FO4, etc.) extends both to 24
//! bytes with a trailing version + unknown field. The first 16 bytes are
//! identical in either layout, so we only need to branch on the
//! additional skip at the end.

use anyhow::{ensure, Context, Result};
use flate2::read::ZlibDecoder;
use std::io::Read;

/// Record flag: data is zlib-compressed.
const FLAG_COMPRESSED: u32 = 0x00040000;

/// ESM format variant — determines record / group header size.
///
/// The two surviving layouts across the Bethesda lineage:
/// - [`Oblivion`](Self::Oblivion) — 20-byte headers (TES4, Oblivion.esm)
/// - [`Tes5Plus`](Self::Tes5Plus) — 24-byte headers (FO3 / FNV / Skyrim /
///   FO4 / FO76 / Starfield)
///
/// Morrowind's TES3 format is entirely different and not supported here;
/// it would need its own reader.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EsmVariant {
    /// Oblivion — 20-byte record and group headers.
    Oblivion,
    /// FO3 / FNV / Skyrim LE+SE / FO4 / FO76 / Starfield — 24-byte headers.
    Tes5Plus,
}

impl EsmVariant {
    /// Auto-detect the ESM variant from a file buffer.
    ///
    /// The heuristic looks at byte offset 20 in the file. Every Bethesda
    /// ESM begins with a `TES4` record, and the first sub-record inside
    /// its data area is always `HEDR`. In Oblivion, the record header is
    /// 20 bytes, so bytes 20-23 spell out `"HEDR"`. In every later game
    /// the header is 24 bytes, so bytes 20-23 are the version u16 +
    /// unknown u16 (small integers, never ASCII). Test the four ASCII
    /// bytes and you have a deterministic, one-shot detector.
    pub fn detect(data: &[u8]) -> Self {
        if data.len() >= 24 && &data[20..24] == b"HEDR" {
            Self::Oblivion
        } else {
            Self::Tes5Plus
        }
    }

    /// Record header size in bytes (`type + data_size + flags + form_id`
    /// plus trailing metadata).
    pub fn record_header_size(self) -> usize {
        match self {
            Self::Oblivion => 20,
            Self::Tes5Plus => 24,
        }
    }

    /// Group header size in bytes (`GRUP + size + label + group_type`
    /// plus trailing metadata).
    pub fn group_header_size(self) -> usize {
        match self {
            Self::Oblivion => 20,
            Self::Tes5Plus => 24,
        }
    }
}

/// Fine-grained game identity for sub-record layout dispatch.
///
/// [`EsmVariant`] only splits "Oblivion (20-byte headers)" from "everything
/// else (24-byte headers)" because that's what the low-level walker needs.
/// Per-record layouts diverge within the Tes5Plus family: FO3/FNV share one
/// schema for ARMO/WEAP/AMMO DATA, Skyrim uses a different one (no health
/// field, BOD2 instead of BMDT, DNAM as packed armor rating), and FO4 adds
/// its own variants again. Callers that parse body data need this finer
/// distinction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GameKind {
    /// Oblivion (TES4, HEDR 1.0).
    Oblivion,
    /// Fallout 3 (HEDR 0.85) and Fallout: New Vegas (HEDR 1.34). These two
    /// share their DATA/DNAM layouts everywhere the current parser cares
    /// about, so they collapse to one game kind.
    #[default]
    Fallout3NV,
    /// Skyrim LE + SE (HEDR 1.7). New ARMO/WEAP/AMMO sub-record schemas.
    Skyrim,
    /// Fallout 4 (HEDR 0.95). SCOL/PKIN/TXST and yet another item schema.
    Fallout4,
    /// Fallout 76 (HEDR 68.0 — unusually large).
    Fallout76,
    /// Starfield (HEDR 0.96).
    Starfield,
}

impl GameKind {
    /// Derive the game kind from the ESM variant plus the HEDR `Version`
    /// f32 (sub-record offset 0 of the TES4 record's HEDR). Callers that
    /// don't have a HEDR version should pass `0.0`, which falls back to
    /// [`GameKind::Fallout3NV`] (the most common Tes5Plus case — keeps
    /// existing synthetic test fixtures working).
    pub fn from_header(variant: EsmVariant, hedr_version: f32) -> Self {
        match variant {
            EsmVariant::Oblivion => Self::Oblivion,
            EsmVariant::Tes5Plus => {
                // HEDR versions sampled from real vanilla masters at
                // 2026-04-19 (all six FO3 GOTY, FNV, FO4, Skyrim SE,
                // Starfield):
                //   FO3 (GOTY) = 0.94    (bytes d7 a3 70 3f)
                //   FO4        = 1.0     (bytes 00 00 80 3f)
                //   Starfield  = 0.96    (bytes 8f c2 75 3f)
                //   FNV        = 1.34    (bytes 1f 85 ab 3f)
                //   Skyrim SE  = 1.71    (bytes 48 e1 da 3f)
                //   FO76       = 68.0
                // Exact float equality is unsafe — match on small bands
                // that leave clear gaps between the known values.
                //
                // Pre-fix the FO3 band (0.94..=0.955) routed every FO3
                // master to Fallout4 — and FO4's real 1.0 fell through
                // to Fallout3NV — so the FO3↔FO4 classification was
                // inverted. Latent because WEAP/ARMO/AMMO DATA arms in
                // items.rs bucket Fallout4 with Fallout3NV/Oblivion;
                // the first schema split (BGSM, dual-weapon SCOL, BOD2
                // typing) would have silently corrupted FO3 data.
                // See #439 / audit FO3-3-01.
                if hedr_version >= 60.0 {
                    Self::Fallout76
                } else if (1.6..=1.8).contains(&hedr_version) {
                    Self::Skyrim
                } else if (0.98..=1.04).contains(&hedr_version) {
                    Self::Fallout4
                } else if (0.955..=0.97).contains(&hedr_version) {
                    Self::Starfield
                } else if (0.93..=0.95).contains(&hedr_version) {
                    // FO3 GOTY (0.94). Pre-GOTY FO3 shipped 0.85 which
                    // falls through to the Fallout3NV tail branch — same
                    // parsing family, so the distinction is purely
                    // cosmetic.
                    Self::Fallout3NV
                } else {
                    // FNV (1.34), pre-GOTY FO3 (0.85), or unknown →
                    // treat as the legacy "Fallout" family.
                    Self::Fallout3NV
                }
            }
        }
    }

    // ── Semantic predicates ────────────────────────────────────────
    //
    // These are the canonical version-feature gates consumed by NPC
    // spawn code (M41.0) and any future face / animation / tint
    // pipeline. The convention mirrors `NifVariant`'s
    // `has_shader_alpha_refs()` / `has_material_crc()` style: each
    // predicate names a *capability*, not a game; new games extend
    // the match arms in one place instead of bumping a dozen
    // `match game { ... }` blocks scattered through the parsers.

    /// True when the NPC face shape is a runtime-evaluated recipe
    /// (FGGS / FGGA / FGTS slider arrays + `.egm` / `.egt` / `.tri`
    /// sidecar deltas applied at spawn time on top of a race-shared
    /// base head NIF). Oblivion / FO3 / FNV use this model.
    ///
    /// **Mutually exclusive** with [`uses_prebaked_facegen`].
    pub fn has_runtime_facegen_recipe(self) -> bool {
        matches!(self, Self::Oblivion | Self::Fallout3NV)
    }

    /// True when the NPC face shape is a per-NPC pre-baked NIF
    /// shipped under `meshes\actors\character\facegendata\facegeom\
    /// <plugin>\<formid:08x>.nif` with a matching face-tint DDS at
    /// `textures\actors\character\facegendata\facetint\...`.
    /// Skyrim / FO4 / FO76 / Starfield use this model.
    ///
    /// **Mutually exclusive** with [`has_runtime_facegen_recipe`].
    pub fn uses_prebaked_facegen(self) -> bool {
        matches!(
            self,
            Self::Skyrim | Self::Fallout4 | Self::Fallout76 | Self::Starfield,
        )
    }

    /// True when the engine ships `.kf` keyframe animation clips that
    /// the existing [`crates::nif::anim::import_kf`] importer can
    /// decode directly. FNV vanilla ships ~962 idle clips under
    /// `meshes\characters\_male\idleanims\`. Skyrim+ vanilla ships
    /// **zero** `.kf` files (Havok `.hkx` only).
    pub fn has_kf_animations(self) -> bool {
        matches!(self, Self::Oblivion | Self::Fallout3NV)
    }

    /// True when the engine animates actors via Havok Behavior Format
    /// (`.hkx` files + behaviour graphs). Skyrim onwards. M41.0 ships
    /// these games at bind pose; M41.x adds a minimal `.hkx` decoder.
    pub fn has_havok_animations(self) -> bool {
        matches!(
            self,
            Self::Skyrim | Self::Fallout4 | Self::Fallout76 | Self::Starfield,
        )
    }
}

/// Binary reader for ESM/ESP files.
pub struct EsmReader<'a> {
    data: &'a [u8],
    pos: usize,
    variant: EsmVariant,
    /// Per-plugin FormID mod-index remap. `None` = identity (single
    /// plugin with no masters). `Some` = remap record FormID top bytes
    /// according to this plugin's position in the global load order.
    /// See [`FormIdRemap`] for the mapping rules.
    form_id_remap: Option<FormIdRemap>,
}

/// FormID mod-index remap rule for a single plugin in a load order.
///
/// A plugin-local FormID has its top byte pointing into the plugin's
/// MASTERS array (or to itself if the top byte equals the master
/// count). At load time every FormID needs to be rewritten to point
/// into the GLOBAL load order so references and map keys stay
/// collision-free across plugins. See `FormIdPair` in
/// `crates/plugin/src/legacy/mod.rs` for the dual-index form.
///
/// Single-plugin load (the default) uses `plugin_index = 0` and an
/// empty `master_indices`, which makes the remap a no-op (a file's
/// own records have top byte 0, and `0 == master_indices.len()` →
/// self-reference → stays 0). Pre-#445 the reader had no remap at
/// all; every record's raw u32 landed directly in `EsmIndex` maps
/// and multi-plugin loads silently collided on the shared
/// base-game-index 0x01 (Anchorage / BrokenSteel / PointLookout /
/// Pitt / Zeta all use 0x01 for their own new forms).
#[derive(Debug, Clone)]
pub struct FormIdRemap {
    /// This plugin's index in the global load order (0-based).
    pub plugin_index: u8,
    /// For each entry in this plugin's MASTERS list, the global
    /// load-order index of that master. `master_indices[N]` is
    /// where a FormID with mod-index `N` actually lives.
    pub master_indices: Vec<u8>,
}

impl FormIdRemap {
    /// Apply this remap to a raw plugin-local FormID.
    ///
    /// `raw & 0xFFFFFF` (the bottom 24 bits) is preserved unchanged —
    /// it's the plugin's internal unique-id. The top byte is replaced
    /// with the global load-order index of whichever plugin owns this
    /// form.
    pub fn remap(&self, raw: u32) -> u32 {
        let mod_index = (raw >> 24) as u8;
        let local = raw & 0x00FF_FFFF;
        let global_index = if mod_index as usize == self.master_indices.len() {
            // Self-reference — top byte equals master count per the
            // Bethesda ESM spec.
            self.plugin_index
        } else if (mod_index as usize) < self.master_indices.len() {
            self.master_indices[mod_index as usize]
        } else {
            // Out-of-range mod index — either a malformed file or an
            // in-memory injected form. Log once per plugin and pass
            // through unchanged so the caller can still see the raw
            // value for diagnosis. Single-plugin loads never hit this.
            log::warn!(
                "FormID {raw:08x} has mod_index {mod_index} but plugin has {} masters",
                self.master_indices.len()
            );
            mod_index
        };
        ((global_index as u32) << 24) | local
    }
}

/// Parsed record header (CELL, REFR, STAT, TES4, etc.).
#[derive(Debug, Clone)]
pub struct RecordHeader {
    pub record_type: [u8; 4],
    pub data_size: u32,
    pub flags: u32,
    pub form_id: u32,
}

/// Parsed group header (GRUP).
#[derive(Debug, Clone)]
pub struct GroupHeader {
    pub label: [u8; 4],
    pub group_type: u32,
    /// Total size including this header.
    pub total_size: u32,
}

/// A sub-record within a record.
#[derive(Debug, Clone)]
pub struct SubRecord {
    pub sub_type: [u8; 4],
    pub data: Vec<u8>,
}

/// File header data from the TES4 record.
#[derive(Debug)]
pub struct FileHeader {
    pub master_files: Vec<String>,
    pub record_count: u32,
    /// HEDR `Version` f32 (sub-record offset 0). 0.0 when absent (synthetic
    /// test fixtures often omit HEDR). Feed into [`GameKind::from_header`].
    pub hedr_version: f32,
    /// TES4 record flag bit `0x80` (Localized). When set, Skyrim+
    /// plugins encode FULL / DESC / RNAM / etc. as a 4-byte u32
    /// reference into companion `Strings/*.{STRINGS,DLSTRINGS,ILSTRINGS}`
    /// files rather than inline z-strings. Downstream record parsers
    /// consult the thread-local `CURRENT_PLUGIN_LOCALIZED` flag
    /// (`records/common.rs`) when decoding those sub-records so a
    /// 4-byte payload becomes a `"<lstring 0xNNNNNNNN>"` placeholder
    /// instead of 3-character UTF-8 garbage. See audit S6-03 / #348.
    pub localized: bool,
}

impl<'a> EsmReader<'a> {
    /// Create a reader, auto-detecting the game variant from the file
    /// header. Oblivion gets 20-byte record/group headers; everything
    /// else gets 24. See [`EsmVariant::detect`].
    pub fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            pos: 0,
            variant: EsmVariant::detect(data),
            form_id_remap: None,
        }
    }

    /// Create a reader with an explicit variant — used by the unit
    /// tests which build synthetic 24-byte records regardless of game.
    pub fn with_variant(data: &'a [u8], variant: EsmVariant) -> Self {
        Self {
            data,
            pos: 0,
            variant,
            form_id_remap: None,
        }
    }

    /// Install a FormID mod-index remap — call once, before walking
    /// records, when this reader is loading a plugin in a multi-plugin
    /// load order. See [`FormIdRemap`] for the rules. See #445.
    pub fn set_form_id_remap(&mut self, remap: FormIdRemap) {
        self.form_id_remap = Some(remap);
    }

    /// Apply the installed FormID remap (if any) to a raw plugin-local
    /// u32. Callsite-agnostic: use this anywhere a sub-record field
    /// carries a cross-record FormID reference (REFR.NAME, XOWN, XLOC,
    /// etc.) so cross-plugin references stay valid. Identity when no
    /// remap is set. See #445.
    pub fn remap_form_id(&self, raw: u32) -> u32 {
        self.form_id_remap
            .as_ref()
            .map_or(raw, |remap| remap.remap(raw))
    }

    pub fn variant(&self) -> EsmVariant {
        self.variant
    }

    pub fn position(&self) -> usize {
        self.pos
    }

    pub fn remaining(&self) -> usize {
        self.data.len().saturating_sub(self.pos)
    }

    pub fn skip(&mut self, n: usize) {
        self.pos += n;
    }

    /// Peek at the next 4 bytes without advancing.
    pub fn peek_type(&self) -> Option<[u8; 4]> {
        if self.pos + 4 <= self.data.len() {
            Some([
                self.data[self.pos],
                self.data[self.pos + 1],
                self.data[self.pos + 2],
                self.data[self.pos + 3],
            ])
        } else {
            None
        }
    }

    /// Check if the next record is a GRUP.
    pub fn is_group(&self) -> bool {
        self.peek_type() == Some(*b"GRUP")
    }

    /// Read a record header (20 bytes on Oblivion, 24 on FO3+).
    pub fn read_record_header(&mut self) -> Result<RecordHeader> {
        let header_size = self.variant.record_header_size();
        ensure!(self.remaining() >= header_size, "Truncated record header");
        let record_type = self.read_bytes_4();
        let data_size = self.read_u32();
        let flags = self.read_u32();
        let form_id_raw = self.read_u32();
        // Trailing metadata: Oblivion = 4 bytes (vc_info); FO3+ = 8 bytes
        // (vc_info + unknown + version + unknown). We don't consume any
        // of it today, just skip past.
        self.skip(header_size - 16);
        // #445 — remap the record's own FormID through the installed
        // load-order so multi-plugin maps stay collision-free. No-op
        // when no remap is set (single-plugin load).
        let form_id = self.remap_form_id(form_id_raw);
        Ok(RecordHeader {
            record_type,
            data_size,
            flags,
            form_id,
        })
    }

    /// Read a group header (20 bytes on Oblivion, 24 on FO3+). Caller
    /// must verify `peek_type() == "GRUP"` first.
    pub fn read_group_header(&mut self) -> Result<GroupHeader> {
        let header_size = self.variant.group_header_size();
        ensure!(self.remaining() >= header_size, "Truncated group header");
        let typ = self.read_bytes_4();
        ensure!(
            &typ == b"GRUP",
            "Expected GRUP, got {:?}",
            std::str::from_utf8(&typ)
        );
        let total_size = self.read_u32();
        let label = self.read_bytes_4();
        let group_type = self.read_u32();
        // Trailing metadata: Oblivion = 4 bytes (stamp); FO3+ = 8 bytes
        // (stamp + unknown + version + unknown).
        self.skip(header_size - 16);
        Ok(GroupHeader {
            label,
            group_type,
            total_size,
        })
    }

    /// Read the sub-records within a record's data section.
    ///
    /// If the record is compressed (FLAG_COMPRESSED), decompresses first.
    pub fn read_sub_records(&mut self, header: &RecordHeader) -> Result<Vec<SubRecord>> {
        let data_start = self.pos;
        let raw_data = if header.flags & FLAG_COMPRESSED != 0 {
            // First 4 bytes = uncompressed size, rest is zlib.
            ensure!(header.data_size >= 4, "Compressed record too small");
            let decompressed_size = self.read_u32() as usize;
            let compressed_len = header.data_size as usize - 4;
            ensure!(
                self.remaining() >= compressed_len,
                "Truncated compressed data"
            );
            let compressed = &self.data[self.pos..self.pos + compressed_len];
            self.pos += compressed_len;

            let mut decoder = ZlibDecoder::new(compressed);
            let mut decompressed = Vec::with_capacity(decompressed_size);
            decoder
                .read_to_end(&mut decompressed)
                .context("Failed to decompress ESM record")?;
            decompressed
        } else {
            let size = header.data_size as usize;
            ensure!(self.remaining() >= size, "Truncated record data");
            let slice = self.data[self.pos..self.pos + size].to_vec();
            self.pos += size;
            slice
        };

        // Parse sub-records from the (possibly decompressed) data.
        let mut sub_pos = 0;
        let mut subs = Vec::new();
        while sub_pos + 6 <= raw_data.len() {
            let sub_type = [
                raw_data[sub_pos],
                raw_data[sub_pos + 1],
                raw_data[sub_pos + 2],
                raw_data[sub_pos + 3],
            ];
            let sub_size =
                u16::from_le_bytes([raw_data[sub_pos + 4], raw_data[sub_pos + 5]]) as usize;
            sub_pos += 6;

            if sub_pos + sub_size > raw_data.len() {
                // Tolerate truncated final sub-record.
                break;
            }
            let data = raw_data[sub_pos..sub_pos + sub_size].to_vec();
            sub_pos += sub_size;
            subs.push(SubRecord { sub_type, data });
        }

        // Ensure we consumed exactly data_size from the outer stream.
        let consumed = self.pos - data_start;
        if consumed != header.data_size as usize {
            // Adjust position if we over/under-read (shouldn't happen, but defensive).
            self.pos = data_start + header.data_size as usize;
        }

        Ok(subs)
    }

    /// Skip a record's data section without parsing.
    pub fn skip_record(&mut self, header: &RecordHeader) {
        self.pos += header.data_size as usize;
    }

    /// Skip a group's remaining content. `total_size` in the group
    /// header includes the (20- or 24-byte) header that the caller has
    /// already read, so subtract the variant's header size to get the
    /// remaining content length.
    pub fn skip_group(&mut self, header: &GroupHeader) {
        self.pos += self.group_content_len(header);
    }

    /// Remaining content length for a group the caller has just read.
    /// Equivalent to `total_size - group_header_size`, variant-aware.
    pub fn group_content_len(&self, header: &GroupHeader) -> usize {
        (header.total_size as usize).saturating_sub(self.variant.group_header_size())
    }

    /// Absolute byte offset of the end of a group's content. The caller
    /// must have already consumed the group header — `position()` is
    /// expected to sit at the first byte of the content. Replaces the
    /// error-prone `reader.position() + total_size - 24` pattern, which
    /// bakes in a Tes5Plus header size and breaks on Oblivion's 20-byte
    /// group header (#391).
    pub fn group_content_end(&self, header: &GroupHeader) -> usize {
        self.pos + self.group_content_len(header)
    }

    /// Parse the TES4 file header record.
    pub fn read_file_header(&mut self) -> Result<FileHeader> {
        let header = self.read_record_header()?;
        ensure!(
            &header.record_type == b"TES4",
            "ESM file must start with TES4 record, got {:?}",
            std::str::from_utf8(&header.record_type),
        );

        // Bit 0x80 in the TES4 record flags is the "Localized" flag
        // Skyrim+ sets on plugins whose FULL/DESC/etc. sub-records
        // carry 4-byte lstring-table u32 indices instead of inline
        // z-strings. Capture once so downstream record parsers can
        // route string decoding through the lstring helper. See
        // audit S6-03 / #348.
        let localized = header.flags & 0x80 != 0;

        let subs = self.read_sub_records(&header)?;
        let mut masters = Vec::new();
        let mut record_count = 0;
        let mut hedr_version = 0.0f32;

        for sub in &subs {
            match &sub.sub_type {
                b"HEDR" if sub.data.len() >= 12 => {
                    hedr_version =
                        f32::from_le_bytes([sub.data[0], sub.data[1], sub.data[2], sub.data[3]]);
                    record_count =
                        u32::from_le_bytes([sub.data[4], sub.data[5], sub.data[6], sub.data[7]]);
                }
                b"MAST" => {
                    // Null-terminated string.
                    let name = sub.data.split(|&b| b == 0).next().unwrap_or(&sub.data);
                    masters.push(String::from_utf8_lossy(name).to_string());
                }
                _ => {}
            }
        }

        Ok(FileHeader {
            master_files: masters,
            record_count,
            hedr_version,
            localized,
        })
    }

    // ── Primitives ──────────────────────────────────────────────────

    fn read_u32(&mut self) -> u32 {
        let v = u32::from_le_bytes([
            self.data[self.pos],
            self.data[self.pos + 1],
            self.data[self.pos + 2],
            self.data[self.pos + 3],
        ]);
        self.pos += 4;
        v
    }

    fn read_bytes_4(&mut self) -> [u8; 4] {
        let v = [
            self.data[self.pos],
            self.data[self.pos + 1],
            self.data[self.pos + 2],
            self.data[self.pos + 3],
        ];
        self.pos += 4;
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a synthetic Tes5Plus (24-byte header) record.
    fn build_record(typ: &[u8; 4], form_id: u32, sub_records: &[(&[u8; 4], &[u8])]) -> Vec<u8> {
        build_record_for(EsmVariant::Tes5Plus, typ, form_id, sub_records)
    }

    /// Build a synthetic record with explicit variant — Oblivion's
    /// 20-byte header has 4 bytes of vc_info padding where the Tes5Plus
    /// layout has 8.
    fn build_record_for(
        variant: EsmVariant,
        typ: &[u8; 4],
        form_id: u32,
        sub_records: &[(&[u8; 4], &[u8])],
    ) -> Vec<u8> {
        // Build sub-record data first.
        let mut sub_data = Vec::new();
        for (sub_type, data) in sub_records {
            sub_data.extend_from_slice(*sub_type);
            sub_data.extend_from_slice(&(data.len() as u16).to_le_bytes());
            sub_data.extend_from_slice(data);
        }

        let mut buf = Vec::new();
        buf.extend_from_slice(typ);
        buf.extend_from_slice(&(sub_data.len() as u32).to_le_bytes()); // data_size
        buf.extend_from_slice(&0u32.to_le_bytes()); // flags
        buf.extend_from_slice(&form_id.to_le_bytes());
        // Trailing metadata: 4 bytes for Oblivion, 8 for Tes5Plus.
        buf.resize(buf.len() + (variant.record_header_size() - 16), 0);
        buf.extend_from_slice(&sub_data);
        buf
    }

    /// Build a synthetic Tes5Plus (24-byte header) group.
    fn build_group(label: &[u8; 4], group_type: u32, content: &[u8]) -> Vec<u8> {
        build_group_for(EsmVariant::Tes5Plus, label, group_type, content)
    }

    fn build_group_for(
        variant: EsmVariant,
        label: &[u8; 4],
        group_type: u32,
        content: &[u8],
    ) -> Vec<u8> {
        let total_size = variant.group_header_size() + content.len();
        let mut buf = Vec::new();
        buf.extend_from_slice(b"GRUP");
        buf.extend_from_slice(&(total_size as u32).to_le_bytes());
        buf.extend_from_slice(label);
        buf.extend_from_slice(&group_type.to_le_bytes());
        buf.resize(buf.len() + (variant.group_header_size() - 16), 0);
        buf.extend_from_slice(content);
        buf
    }

    /// Regression: #439 / FO3-3-01. Pin the HEDR → GameKind mapping
    /// against real vanilla master values sampled from disk on
    /// 2026-04-19. Pre-fix FO3's 0.94 routed to Fallout4 and FO4's 1.0
    /// fell through to Fallout3NV, inverting the FO3↔FO4
    /// classification.
    /// Regression: #445 — single-plugin loads (no masters, self-index 0)
    /// must be an exact identity remap so the existing CLI behaviour
    /// and every existing consumer keep seeing the same u32 FormIDs.
    #[test]
    fn form_id_remap_single_plugin_is_identity() {
        let remap = FormIdRemap {
            plugin_index: 0,
            master_indices: Vec::new(),
        };
        // Self-references (top byte = 0 = master_count) pass through.
        assert_eq!(remap.remap(0x0001_2345), 0x0001_2345);
        assert_eq!(remap.remap(0x00CA_FEBA), 0x00CA_FEBA);
    }

    /// Two-plugin load: Anchorage.esm depends on Fallout3.esm.
    /// Anchorage's own new forms land in the global slot for Anchorage
    /// (plugin_index=1), and its references to Fallout3 statics pass
    /// through unchanged (mod_index 0 → master_indices[0] = 0).
    #[test]
    fn form_id_remap_dlc_on_base_routes_self_and_master_correctly() {
        // Anchorage.esm loaded at plugin_index=1 with Fallout3.esm as master 0.
        let remap = FormIdRemap {
            plugin_index: 1,
            master_indices: vec![0],
        };
        // Reference to Fallout3 (mod_index=0, master_indices[0]=0) → unchanged.
        assert_eq!(remap.remap(0x0001_2345), 0x0001_2345);
        // Self-reference (mod_index=1 == master count) → plugin_index=1.
        assert_eq!(remap.remap(0x0101_2345), 0x0101_2345);
    }

    /// Three-plugin collision scenario from the issue: Anchorage and
    /// BrokenSteel both ship form 0x01_012345 in-file. Remapping by
    /// load-order prevents the collision in shared HashMaps.
    #[test]
    fn form_id_remap_two_dlcs_resolve_collision() {
        // Anchorage.esm loaded at plugin_index=1, masters=[0 (Fallout3)].
        let anchorage = FormIdRemap {
            plugin_index: 1,
            master_indices: vec![0],
        };
        // BrokenSteel.esm loaded at plugin_index=2, masters=[0 (Fallout3)].
        let broken_steel = FormIdRemap {
            plugin_index: 2,
            master_indices: vec![0],
        };

        // Both files ship their own 0x01_012345 — the audit's canonical
        // example. Under remap they land in distinct global slots.
        let anchorage_form = anchorage.remap(0x0101_2345);
        let broken_steel_form = broken_steel.remap(0x0101_2345);
        assert_eq!(anchorage_form, 0x0101_2345);
        assert_eq!(broken_steel_form, 0x0201_2345);
        assert_ne!(
            anchorage_form, broken_steel_form,
            "DLC self-refs must not collide after remap — this is the #445 regression"
        );

        // Both files' references to Fallout3 base forms remain unchanged.
        assert_eq!(anchorage.remap(0x00CA_FEBA), 0x00CA_FEBA);
        assert_eq!(broken_steel.remap(0x00CA_FEBA), 0x00CA_FEBA);
    }

    /// A plugin loaded at plugin_index=3 that only declares Fallout3
    /// as master (master_indices=[0]) — self-refs use mod_index=1
    /// in-file but must remap to the plugin's actual load-order slot 3.
    /// Catches the case where the file's MAST count is smaller than
    /// the load-order index.
    #[test]
    fn form_id_remap_rewrites_self_ref_to_load_order_index() {
        let remap = FormIdRemap {
            plugin_index: 3,
            master_indices: vec![0],
        };
        // mod_index 1 == num_masters → self → plugin_index=3.
        assert_eq!(remap.remap(0x0112_3456), 0x0312_3456);
    }

    /// Record header read returns the remapped FormID on any installed
    /// remap — the integration point where the fix actually lands.
    #[test]
    fn read_record_header_applies_installed_remap() {
        let data = build_record(b"STAT", 0x0112_3456, &[]);
        let mut reader = EsmReader::with_variant(&data, EsmVariant::Tes5Plus);
        reader.set_form_id_remap(FormIdRemap {
            plugin_index: 2,
            master_indices: vec![0],
        });
        let header = reader.read_record_header().unwrap();
        assert_eq!(
            header.form_id, 0x0212_3456,
            "read_record_header must route the top byte through set_form_id_remap (#445)"
        );
    }

    /// Bottom 24 bits of every FormID must be preserved verbatim — only
    /// the mod-index byte changes.
    #[test]
    fn form_id_remap_preserves_local_24_bits() {
        let remap = FormIdRemap {
            plugin_index: 5,
            master_indices: vec![0, 1],
        };
        let local = 0x00AB_CDEF;
        assert_eq!(remap.remap(local) & 0x00FF_FFFF, local);
        assert_eq!(remap.remap(0x01CD_EF12) & 0x00FF_FFFF, 0x00CD_EF12);
        assert_eq!(remap.remap(0x02CD_EF12) & 0x00FF_FFFF, 0x00CD_EF12);
    }

    #[test]
    fn game_kind_from_header_maps_real_master_hedr_values() {
        // FO3 GOTY — bytes d7 a3 70 3f → f32 0.94.
        assert_eq!(
            GameKind::from_header(EsmVariant::Tes5Plus, 0.94),
            GameKind::Fallout3NV,
            "FO3 GOTY (HEDR=0.94) must classify as Fallout3NV",
        );
        // FNV — bytes 1f 85 ab 3f → f32 ≈ 1.34.
        assert_eq!(
            GameKind::from_header(EsmVariant::Tes5Plus, 1.34),
            GameKind::Fallout3NV,
            "FNV (HEDR=1.34) must classify as Fallout3NV",
        );
        // FO4 — bytes 00 00 80 3f → f32 1.0.
        assert_eq!(
            GameKind::from_header(EsmVariant::Tes5Plus, 1.0),
            GameKind::Fallout4,
            "FO4 (HEDR=1.0) must classify as Fallout4",
        );
        // Skyrim SE — bytes 48 e1 da 3f → f32 ≈ 1.71.
        assert_eq!(
            GameKind::from_header(EsmVariant::Tes5Plus, 1.71),
            GameKind::Skyrim,
            "Skyrim SE (HEDR=1.71) must classify as Skyrim",
        );
        // Starfield — bytes 8f c2 75 3f → f32 ≈ 0.96.
        assert_eq!(
            GameKind::from_header(EsmVariant::Tes5Plus, 0.96),
            GameKind::Starfield,
            "Starfield (HEDR=0.96) must classify as Starfield",
        );
        // FO76 — HEDR=68.0 per UESP.
        assert_eq!(
            GameKind::from_header(EsmVariant::Tes5Plus, 68.0),
            GameKind::Fallout76,
            "FO76 (HEDR=68.0) must classify as Fallout76",
        );
        // Oblivion — variant-dispatched regardless of HEDR.
        assert_eq!(
            GameKind::from_header(EsmVariant::Oblivion, 1.0),
            GameKind::Oblivion,
        );
    }

    #[test]
    fn read_record_header_basic() {
        let data = build_record(b"STAT", 0x12345, &[]);
        let mut reader = EsmReader::with_variant(&data, EsmVariant::Tes5Plus);
        let header = reader.read_record_header().unwrap();
        assert_eq!(&header.record_type, b"STAT");
        assert_eq!(header.form_id, 0x12345);
        assert_eq!(header.data_size, 0);
    }

    #[test]
    fn read_sub_records() {
        let data = build_record(
            b"STAT",
            0x100,
            &[(b"EDID", b"TestStatic\0"), (b"MODL", b"meshes\\test.nif\0")],
        );
        let mut reader = EsmReader::with_variant(&data, EsmVariant::Tes5Plus);
        let header = reader.read_record_header().unwrap();
        let subs = reader.read_sub_records(&header).unwrap();

        assert_eq!(subs.len(), 2);
        assert_eq!(&subs[0].sub_type, b"EDID");
        assert_eq!(&subs[0].data, b"TestStatic\0");
        assert_eq!(&subs[1].sub_type, b"MODL");
        assert_eq!(&subs[1].data, b"meshes\\test.nif\0");
    }

    #[test]
    fn read_group_header_basic() {
        let group = build_group(b"CELL", 0, &[]);
        let mut reader = EsmReader::with_variant(&group, EsmVariant::Tes5Plus);
        let header = reader.read_group_header().unwrap();
        assert_eq!(&header.label, b"CELL");
        assert_eq!(header.group_type, 0);
        assert_eq!(header.total_size, 24);
    }

    #[test]
    fn is_group_detects_grup() {
        let group = build_group(b"CELL", 0, &[]);
        let reader = EsmReader::with_variant(&group, EsmVariant::Tes5Plus);
        assert!(reader.is_group());

        let record = build_record(b"STAT", 0, &[]);
        let reader = EsmReader::with_variant(&record, EsmVariant::Tes5Plus);
        assert!(!reader.is_group());
    }

    #[test]
    fn skip_record_advances_position() {
        let data = build_record(b"STAT", 0, &[(b"EDID", b"Test\0")]);
        let mut reader = EsmReader::with_variant(&data, EsmVariant::Tes5Plus);
        let header = reader.read_record_header().unwrap();
        reader.skip_record(&header);
        assert_eq!(reader.remaining(), 0);
    }

    #[test]
    fn file_header_parses_tes4() {
        let tes4 = build_record(
            b"TES4",
            0,
            &[
                // HEDR: version(f32) + record_count(u32) + next_object_id(u32)
                (b"HEDR", &{
                    let mut d = Vec::new();
                    d.extend_from_slice(&1.0f32.to_le_bytes()); // version
                    d.extend_from_slice(&42u32.to_le_bytes()); // record count
                    d.extend_from_slice(&0u32.to_le_bytes()); // next object id
                    d
                }),
                (b"MAST", b"FalloutNV.esm\0"),
                (b"DATA", &0u64.to_le_bytes()),
            ],
        );
        let mut reader = EsmReader::with_variant(&tes4, EsmVariant::Tes5Plus);
        let fh = reader.read_file_header().unwrap();
        assert_eq!(fh.record_count, 42);
        assert_eq!(fh.master_files, vec!["FalloutNV.esm"]);
    }

    // ── Oblivion (20-byte header) tests ────────────────────────────────

    #[test]
    fn variant_detect_oblivion() {
        // Build a real Oblivion TES4 record: 20-byte header + HEDR subrecord.
        let tes4 = build_record_for(
            EsmVariant::Oblivion,
            b"TES4",
            0,
            &[(b"HEDR", &{
                let mut d = Vec::new();
                d.extend_from_slice(&1.0f32.to_le_bytes()); // Oblivion version = 1.0
                d.extend_from_slice(&0u32.to_le_bytes()); // record count
                d.extend_from_slice(&0u32.to_le_bytes()); // next object id
                d
            })],
        );
        // At offset 20 we should see "HEDR" — the sub-record type.
        assert_eq!(&tes4[20..24], b"HEDR");
        assert_eq!(EsmVariant::detect(&tes4), EsmVariant::Oblivion);
    }

    #[test]
    fn variant_detect_tes5_plus() {
        // FNV-style TES4 — HEDR lands at offset 24.
        let tes4 = build_record_for(
            EsmVariant::Tes5Plus,
            b"TES4",
            0,
            &[(b"HEDR", b"placeholder\0")],
        );
        assert_eq!(&tes4[24..28], b"HEDR");
        assert_eq!(EsmVariant::detect(&tes4), EsmVariant::Tes5Plus);
    }

    #[test]
    fn read_oblivion_record_header_has_20_byte_layout() {
        let data = build_record_for(EsmVariant::Oblivion, b"STAT", 0xAB, &[]);
        assert_eq!(data.len(), 20); // no sub-records → 20 header bytes total
        let mut reader = EsmReader::with_variant(&data, EsmVariant::Oblivion);
        let header = reader.read_record_header().unwrap();
        assert_eq!(&header.record_type, b"STAT");
        assert_eq!(header.form_id, 0xAB);
        assert_eq!(header.data_size, 0);
        assert_eq!(reader.position(), 20);
    }

    #[test]
    fn read_oblivion_group_header_has_20_byte_layout() {
        let group = build_group_for(EsmVariant::Oblivion, b"CELL", 0, &[]);
        assert_eq!(group.len(), 20);
        let mut reader = EsmReader::with_variant(&group, EsmVariant::Oblivion);
        let header = reader.read_group_header().unwrap();
        assert_eq!(&header.label, b"CELL");
        assert_eq!(header.total_size, 20);
        assert_eq!(reader.position(), 20);
    }

    #[test]
    fn read_oblivion_sub_records() {
        let data = build_record_for(
            EsmVariant::Oblivion,
            b"STAT",
            0x100,
            &[
                (b"EDID", b"TestOblivion\0"),
                (b"MODL", b"meshes\\stat.nif\0"),
            ],
        );
        let mut reader = EsmReader::with_variant(&data, EsmVariant::Oblivion);
        let header = reader.read_record_header().unwrap();
        let subs = reader.read_sub_records(&header).unwrap();
        assert_eq!(subs.len(), 2);
        assert_eq!(&subs[0].sub_type, b"EDID");
        assert_eq!(subs[0].data, b"TestOblivion\0");
        assert_eq!(&subs[1].sub_type, b"MODL");
    }

    #[test]
    fn oblivion_skip_group_uses_20_byte_header() {
        // Group containing one STAT record. skip_group should land exactly
        // at end-of-buffer — off-by-4 bugs show up here.
        let inner = build_record_for(EsmVariant::Oblivion, b"STAT", 1, &[]);
        let group = build_group_for(EsmVariant::Oblivion, b"STAT", 0, &inner);
        let mut reader = EsmReader::with_variant(&group, EsmVariant::Oblivion);
        let header = reader.read_group_header().unwrap();
        reader.skip_group(&header);
        assert_eq!(reader.remaining(), 0);
    }

    #[test]
    fn group_content_end_is_variant_aware() {
        // Regression: #391 — the walker used to hardcode `-24` when
        // deriving a group's content end, which over-read by 4 bytes on
        // Oblivion (20-byte group header). The helper on the reader
        // must produce different absolute positions for the two
        // variants given identical `total_size` payload.
        let inner_tes5 = build_record_for(EsmVariant::Tes5Plus, b"STAT", 1, &[]);
        let group_tes5 = build_group_for(EsmVariant::Tes5Plus, b"STAT", 0, &inner_tes5);
        let mut r5 = EsmReader::with_variant(&group_tes5, EsmVariant::Tes5Plus);
        let gh5 = r5.read_group_header().unwrap();
        assert_eq!(
            r5.group_content_end(&gh5),
            group_tes5.len(),
            "Tes5Plus: end of content must equal buffer length"
        );
        assert_eq!(r5.group_content_len(&gh5), group_tes5.len() - 24);

        let inner_obl = build_record_for(EsmVariant::Oblivion, b"STAT", 1, &[]);
        let group_obl = build_group_for(EsmVariant::Oblivion, b"STAT", 0, &inner_obl);
        let mut ro = EsmReader::with_variant(&group_obl, EsmVariant::Oblivion);
        let gho = ro.read_group_header().unwrap();
        assert_eq!(
            ro.group_content_end(&gho),
            group_obl.len(),
            "Oblivion: end of content must equal buffer length (20-byte header)"
        );
        assert_eq!(ro.group_content_len(&gho), group_obl.len() - 20);

        // Same payload, different header sizes — the variant decides.
        let fake_header = GroupHeader {
            label: *b"STAT",
            group_type: 0,
            total_size: 100,
        };
        let r5 = EsmReader::with_variant(&[], EsmVariant::Tes5Plus);
        let ro = EsmReader::with_variant(&[], EsmVariant::Oblivion);
        assert_eq!(r5.group_content_len(&fake_header), 76); // 100 - 24
        assert_eq!(ro.group_content_len(&fake_header), 80); // 100 - 20
    }

    #[test]
    fn oblivion_file_header_parses() {
        let tes4 = build_record_for(
            EsmVariant::Oblivion,
            b"TES4",
            0,
            &[
                (b"HEDR", &{
                    let mut d = Vec::new();
                    d.extend_from_slice(&1.0f32.to_le_bytes()); // Oblivion v1.0
                    d.extend_from_slice(&123u32.to_le_bytes()); // record count
                    d.extend_from_slice(&0u32.to_le_bytes());
                    d
                }),
                (b"MAST", b"Oblivion.esm\0"),
            ],
        );
        let mut reader = EsmReader::new(&tes4); // auto-detect
        assert_eq!(reader.variant(), EsmVariant::Oblivion);
        let fh = reader.read_file_header().unwrap();
        assert_eq!(fh.record_count, 123);
        assert_eq!(fh.master_files, vec!["Oblivion.esm"]);
    }

    /// M41.0 Phase 1a — face/animation predicates are exhaustive
    /// across the 6 supported `GameKind` variants and partition them
    /// cleanly: every game uses **either** a runtime FaceGen recipe
    /// **or** a pre-baked NIF (no double-counting); same for `.kf` vs
    /// `.hkx`. A new game variant must extend both `match` arms in
    /// one place to satisfy this test.
    #[test]
    fn game_kind_face_animation_predicates_partition_cleanly() {
        let all = [
            GameKind::Oblivion,
            GameKind::Fallout3NV,
            GameKind::Skyrim,
            GameKind::Fallout4,
            GameKind::Fallout76,
            GameKind::Starfield,
        ];
        for g in all {
            assert!(
                g.has_runtime_facegen_recipe() ^ g.uses_prebaked_facegen(),
                "{g:?}: must satisfy exactly one of runtime / prebaked FaceGen",
            );
            assert!(
                g.has_kf_animations() ^ g.has_havok_animations(),
                "{g:?}: must satisfy exactly one of kf / hkx animation",
            );
        }

        // Spot-check the membership of each set so a future
        // misclassification fails loudly here, not at spawn time.
        assert!(GameKind::Fallout3NV.has_runtime_facegen_recipe());
        assert!(GameKind::Oblivion.has_runtime_facegen_recipe());
        assert!(GameKind::Skyrim.uses_prebaked_facegen());
        assert!(GameKind::Fallout4.uses_prebaked_facegen());
        assert!(GameKind::Starfield.uses_prebaked_facegen());

        assert!(GameKind::Fallout3NV.has_kf_animations());
        assert!(GameKind::Skyrim.has_havok_animations());
        assert!(GameKind::Starfield.has_havok_animations());
    }

    // ── Compressed record tests (#990 / SK-D6-NEW-02) ──────────────────
    //
    // `read_sub_records` had zero unit test coverage for the FLAG_COMPRESSED
    // branch. These tests guard against: backend swaps (flate2 → miniz_oxide),
    // decompressed-size prefix off-by-one, the data_size < 4 panic path, and
    // silent byte-order changes in read_u32.

    /// Build a raw sub-record payload byte-vector (same layout `read_sub_records`
    /// parses) without going through a full record header.
    fn build_sub_record_payload(sub_records: &[(&[u8; 4], &[u8])]) -> Vec<u8> {
        let mut payload = Vec::new();
        for (sub_type, data) in sub_records {
            payload.extend_from_slice(*sub_type);
            payload.extend_from_slice(&(data.len() as u16).to_le_bytes());
            payload.extend_from_slice(data);
        }
        payload
    }

    /// Build a compressed record: zlib-encode the sub-record payload, prepend
    /// the 4-byte decompressed-size header, and wrap in a Tes5Plus record with
    /// FLAG_COMPRESSED set.
    fn build_compressed_record(
        typ: &[u8; 4],
        form_id: u32,
        sub_records: &[(&[u8; 4], &[u8])],
    ) -> Vec<u8> {
        use flate2::write::ZlibEncoder;
        use flate2::Compression;
        use std::io::Write;

        let payload = build_sub_record_payload(sub_records);
        let decompressed_size = payload.len() as u32;

        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(&payload).expect("zlib encode");
        let compressed = encoder.finish().expect("zlib finish");

        // data_size = 4 (decompressed-size prefix) + compressed_len.
        let data_size = (4 + compressed.len()) as u32;

        let mut buf = Vec::new();
        buf.extend_from_slice(typ); // record type
        buf.extend_from_slice(&data_size.to_le_bytes()); // data_size
        buf.extend_from_slice(&FLAG_COMPRESSED.to_le_bytes()); // flags
        buf.extend_from_slice(&form_id.to_le_bytes()); // form_id
        // Tes5Plus trailing 8 bytes (vc_info + revision + version + unknown).
        buf.extend_from_slice(&[0u8; 8]);
        buf.extend_from_slice(&decompressed_size.to_le_bytes()); // 4-byte prefix
        buf.extend_from_slice(&compressed);
        buf
    }

    /// Happy path: a compressed STAT record with two sub-records round-trips
    /// correctly through `read_sub_records`.
    #[test]
    fn compressed_record_round_trips_sub_records() {
        let data = build_compressed_record(
            b"STAT",
            0x200,
            &[(b"EDID", b"TreeLOD\0"), (b"MODL", b"meshes\\tree_lod.nif\0")],
        );
        let mut reader = EsmReader::with_variant(&data, EsmVariant::Tes5Plus);
        let header = reader.read_record_header().unwrap();

        // FLAG_COMPRESSED must be visible on the header.
        assert_ne!(
            header.flags & FLAG_COMPRESSED,
            0,
            "FLAG_COMPRESSED must be set on the header"
        );

        let subs = reader.read_sub_records(&header).unwrap();

        assert_eq!(subs.len(), 2);
        assert_eq!(&subs[0].sub_type, b"EDID");
        assert_eq!(subs[0].data, b"TreeLOD\0");
        assert_eq!(&subs[1].sub_type, b"MODL");
        assert_eq!(subs[1].data, b"meshes\\tree_lod.nif\0");
    }

    /// Decompressed size embedded in the 4-byte prefix must match the actual
    /// decompressed content length. This test verifies that the capacity hint
    /// (`Vec::with_capacity(decompressed_size)`) is correct — a mismatch here
    /// would panic or silently truncate on strict allocators.
    #[test]
    fn compressed_record_prefix_matches_payload_length() {
        use flate2::write::ZlibEncoder;
        use flate2::Compression;
        use std::io::Write;

        let payload = build_sub_record_payload(&[(b"FULL", b"Hello world\0")]);
        let expected_len = payload.len();
        let decompressed_size = payload.len() as u32;

        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(&payload).unwrap();
        let compressed = encoder.finish().unwrap();

        let data_size = (4 + compressed.len()) as u32;
        let mut buf = Vec::new();
        buf.extend_from_slice(b"MISC");
        buf.extend_from_slice(&data_size.to_le_bytes());
        buf.extend_from_slice(&FLAG_COMPRESSED.to_le_bytes());
        buf.extend_from_slice(&1u32.to_le_bytes()); // form_id
        buf.extend_from_slice(&[0u8; 8]);
        buf.extend_from_slice(&decompressed_size.to_le_bytes());
        buf.extend_from_slice(&compressed);

        let mut reader = EsmReader::with_variant(&buf, EsmVariant::Tes5Plus);
        let header = reader.read_record_header().unwrap();
        let subs = reader.read_sub_records(&header).unwrap();

        assert_eq!(subs.len(), 1);
        assert_eq!(subs[0].data.len() + 6, expected_len, // +6 for sub-type + length prefix
            "decompressed payload length must match the 4-byte prefix");
    }

    /// Error path: data_size < 4 must be rejected with an error, not a panic.
    #[test]
    fn compressed_record_too_small_returns_error() {
        // Build a record with FLAG_COMPRESSED but data_size = 3 (too small for prefix).
        let mut buf = Vec::new();
        buf.extend_from_slice(b"WEAP");
        buf.extend_from_slice(&3u32.to_le_bytes()); // data_size = 3 — too small
        buf.extend_from_slice(&FLAG_COMPRESSED.to_le_bytes());
        buf.extend_from_slice(&0u32.to_le_bytes()); // form_id
        buf.extend_from_slice(&[0u8; 8]);
        buf.extend_from_slice(&[0u8; 3]); // 3 bytes of body

        let mut reader = EsmReader::with_variant(&buf, EsmVariant::Tes5Plus);
        let header = reader.read_record_header().unwrap();
        let result = reader.read_sub_records(&header);
        assert!(
            result.is_err(),
            "data_size < 4 must return Err, got Ok"
        );
    }
}
