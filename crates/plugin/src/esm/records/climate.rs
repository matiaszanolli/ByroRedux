//! CLMT (Climate) record parser.
//!
//! Climate records define the weather probability table for a worldspace.
//! Each worldspace references one climate via CNAM; the climate lists
//! the possible weathers with relative chances.

use super::common::{read_zstring, remap_fid};
use crate::esm::reader::{FormIdRemap, GameKind, SubRecord};
use crate::esm::sub_reader::SubReader;

/// A weather entry in the climate's weather list.
#[derive(Debug, Clone)]
pub struct ClimateWeather {
    /// Form ID of the WTHR record.
    pub weather_form_id: u32,
    /// Relative chance (higher = more likely). UESP + in-tree WLST parser
    /// comment both describe this as **i32**. Pre-#476 it was typed u32;
    /// negative-chance entries (used by mods as sentinels or subtractive
    /// weights) wrapped to huge positive and silently won `max_by_key`
    /// during default-weather selection.
    pub chance: i32,
}

/// Parsed CLMT record.
#[derive(Debug, Clone, Default)]
pub struct ClimateRecord {
    pub form_id: u32,
    pub editor_id: String,
    /// Ordered weather list with chances. First entry is typically the default.
    pub weathers: Vec<ClimateWeather>,
    /// Sun texture path (from FNAM).
    pub sun_texture: Option<String>,
    /// Sunrise/sunset timing data from TNAM (6 bytes: sunrise begin/end,
    /// sunset begin/end, volatility, moon phases — times in 10-minute units).
    pub sunrise_begin: u8,
    pub sunrise_end: u8,
    pub sunset_begin: u8,
    pub sunset_end: u8,
}

/// Parse a CLMT record from its sub-records.
///
/// `game` selects the WLST entry layout: Oblivion ships 8-byte
/// `(form_id: u32, chance: i32)` entries; every later game ships
/// 12-byte `(form_id: u32, chance: i32, global: u32)` entries. Pre-#540
/// the parser autodetected via `data.len() % 12 == 0`, which mis-fired
/// on any Oblivion CLMT whose 8-byte entry count was a multiple of 3
/// — the buffer would parse as `N × 2/3` 12-byte entries with the
/// next entry's form_id leaking into the first entry's `global` slot
/// (and the trailing entry's data straddling the buffer boundary).
/// Vanilla Oblivion ships none, but DLC + OBSE weather mods plausibly
/// cross into 6-entry territory. See M33-08 / #540.
pub fn parse_clmt(
    form_id: u32,
    subs: &[SubRecord],
    game: GameKind,
    remap: &Option<FormIdRemap>,
) -> ClimateRecord {
    let mut record = ClimateRecord {
        form_id,
        ..ClimateRecord::default()
    };

    for sub in subs {
        match &sub.sub_type {
            b"EDID" => record.editor_id = read_zstring(&sub.data),

            // WLST: weather list. Per-game entry size dispatched off
            // `game` (M33-08 / #540) — see the function-level doc for
            // the autodetect-collision rationale.
            //
            //   Oblivion → 8 bytes:  (form_id: u32, chance: i32)
            //   else     → 12 bytes: (form_id: u32, chance: i32, global: u32)
            b"WLST" => {
                let entry_size = match game {
                    GameKind::Oblivion => 8,
                    GameKind::Fallout3NV
                    | GameKind::Skyrim
                    | GameKind::Fallout4
                    | GameKind::Fallout76
                    | GameKind::Starfield => 12,
                };
                let count = sub.data.len() / entry_size;
                let trailing_pad = entry_size - 8;
                let mut r = SubReader::new(&sub.data);
                for _ in 0..count {
                    if let (Ok(fid), Ok(chance_bits)) = (r.u32(), r.u32()) {
                        if fid != 0 {
                            record.weathers.push(ClimateWeather {
                                // #4066 — WLST weather ids are cross-record
                                // references and must land in the same global
                                // space `EsmIndex.weathers` is keyed in. Every
                                // shipped DLC master authors these as
                                // self-references with a non-zero mod index,
                                // so leaving them raw resolved 100 % of them
                                // to no weather.
                                weather_form_id: remap_fid(fid, remap),
                                // Reinterpret the 4-byte little-endian slot as
                                // signed — UESP WLST schema says i32. See #476.
                                chance: chance_bits as i32,
                            });
                        }
                    }
                    r.skip_or_eof(trailing_pad);
                }
            }

            // FNAM: sun texture path.
            b"FNAM" => record.sun_texture = Some(read_zstring(&sub.data)),

            // TNAM: timing data (6 bytes).
            b"TNAM" if sub.data.len() >= 4 => {
                record.sunrise_begin = sub.data[0];
                record.sunrise_end = sub.data[1];
                record.sunset_begin = sub.data[2];
                record.sunset_end = sub.data[3];
            }

            _ => {}
        }
    }

    record
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::esm::reader::SubRecord;

    fn make_sub(sub_type: &[u8; 4], data: Vec<u8>) -> SubRecord {
        SubRecord {
            sub_type: *sub_type,
            data,
        }
    }

    /// #4066 — `WLST` weather ids are cross-record references into
    /// `EsmIndex.weathers`, which is keyed in **global** space (every
    /// record's own FormID is remapped in `read_record_header`). Leaving
    /// them raw meant a DLC-authored climate resolved to no weather at
    /// all: the census over every shipped DLC master that authors a
    /// `CLMT` found 100 % of `WLST` references are self-references with a
    /// non-zero mod index (FO3 Anchorage/BrokenSteel/PointLookout/ThePitt,
    /// FNV DeadMoney/HonestHearts/OldWorldBlues/LonesomeRoad, Skyrim
    /// Dawnguard/Dragonborn, FO4 DLCCoast/DLCNukaWorld).
    #[test]
    fn parse_clmt_wlst_remaps_to_global_form_id_space() {
        // A one-master DLC sitting at global slot 2 — the shape every
        // plugin in the census has (`Dawnguard.esm` has two masters and
        // behaves identically, one slot further along).
        let remap = crate::esm::reader::FormIdRemap::regular(2, vec![0]);

        // mod_index 1 == this plugin's own position in its own MASTERS
        // list, i.e. a self-reference: a WTHR the DLC itself adds.
        let self_ref: u32 = 0x0100_0ABC;
        // mod_index 0 == the master's slot: a base-game WTHR.
        let master_ref: u32 = 0x0000_0DEF;

        let mut wlst = Vec::new();
        for fid in [self_ref, master_ref] {
            wlst.extend_from_slice(&fid.to_le_bytes());
            wlst.extend_from_slice(&50i32.to_le_bytes());
            wlst.extend_from_slice(&0u32.to_le_bytes());
        }

        let c = parse_clmt(
            0x0200_0001,
            &[make_sub(b"WLST", wlst)],
            GameKind::Fallout3NV,
            &Some(remap),
        );

        assert_eq!(
            c.weathers[0].weather_form_id, 0x0200_0ABC,
            "a self-referencing WLST entry must land on the plugin's own \
             global slot, not keep its plugin-local mod index"
        );
        assert_eq!(
            c.weathers[1].weather_form_id, master_ref,
            "a master-slot reference (mod_index 0) already sits at slot 0"
        );
    }

    /// The no-load-order path (a lone plugin parsed with `remap: &None`)
    /// must leave ids byte-identical — `remap_fid` is a no-op there, and
    /// every other test in this module relies on it.
    #[test]
    fn parse_clmt_wlst_without_a_remap_is_identity() {
        let mut wlst = Vec::new();
        wlst.extend_from_slice(&0x0100_0ABCu32.to_le_bytes());
        wlst.extend_from_slice(&50i32.to_le_bytes());
        wlst.extend_from_slice(&0u32.to_le_bytes());
        let c = parse_clmt(
            0x0200_0001,
            &[make_sub(b"WLST", wlst)],
            GameKind::Fallout3NV,
            &None,
        );
        assert_eq!(c.weathers[0].weather_form_id, 0x0100_0ABC);
    }

    #[test]
    fn parse_clmt_basic() {
        // Build a WLST with 2 weather entries (12-byte format: fid + chance + global).
        let mut wlst_data = Vec::new();
        wlst_data.extend_from_slice(&0x1000u32.to_le_bytes()); // weather 1 form ID
        wlst_data.extend_from_slice(&60i32.to_le_bytes()); // 60% chance
        wlst_data.extend_from_slice(&0u32.to_le_bytes()); // global form ID (unused)
        wlst_data.extend_from_slice(&0x2000u32.to_le_bytes()); // weather 2 form ID
        wlst_data.extend_from_slice(&40i32.to_le_bytes()); // 40% chance
        wlst_data.extend_from_slice(&0u32.to_le_bytes()); // global form ID (unused)

        let subs = vec![
            make_sub(b"EDID", b"TestClimate\0".to_vec()),
            make_sub(b"WLST", wlst_data),
            make_sub(b"FNAM", b"sky\\sun_01.dds\0".to_vec()),
            make_sub(b"TNAM", vec![6, 8, 18, 20, 0, 0]),
        ];

        let c = parse_clmt(0xABCD, &subs, GameKind::Fallout3NV, &None);
        assert_eq!(c.form_id, 0xABCD);
        assert_eq!(c.editor_id, "TestClimate");
        assert_eq!(c.weathers.len(), 2);
        assert_eq!(c.weathers[0].weather_form_id, 0x1000);
        assert_eq!(c.weathers[0].chance, 60);
        assert_eq!(c.weathers[1].weather_form_id, 0x2000);
        assert_eq!(c.weathers[1].chance, 40);
        assert_eq!(c.sun_texture.as_deref(), Some("sky\\sun_01.dds"));
        assert_eq!(c.sunrise_begin, 6);
        assert_eq!(c.sunset_end, 20);
    }

    /// Regression: #476 — negative-chance WLST entries must decode as
    /// signed (not wrap to huge u32). Mods use -1 as a sentinel /
    /// subtractive weight; pre-#476 the u32 reinterpretation made -1
    /// win `max_by_key` against legitimate positive chances.
    #[test]
    fn parse_clmt_wlst_decodes_negative_chance() {
        let mut wlst_data = Vec::new();
        wlst_data.extend_from_slice(&0x1000u32.to_le_bytes()); // weather 1
        wlst_data.extend_from_slice(&(-1i32).to_le_bytes()); // negative sentinel
        wlst_data.extend_from_slice(&0u32.to_le_bytes()); // global unused
        wlst_data.extend_from_slice(&0x2000u32.to_le_bytes()); // weather 2
        wlst_data.extend_from_slice(&75i32.to_le_bytes()); // 75% chance
        wlst_data.extend_from_slice(&0u32.to_le_bytes());

        let subs = vec![make_sub(b"WLST", wlst_data)];
        let c = parse_clmt(0xBEEF, &subs, GameKind::Fallout3NV, &None);
        assert_eq!(c.weathers.len(), 2);
        assert_eq!(c.weathers[0].chance, -1);
        assert_eq!(c.weathers[1].chance, 75);
        // Consumers that `max_by_key` over chance must filter < 0 first;
        // see cell_loader.rs default-weather selection (#476 consumer fix).
    }

    /// Regression: M33-08 / #540 — Oblivion WLST is **always** 8-byte
    /// entries. The pre-fix `data.len() % 12 == 0` autodetect mis-fired
    /// on any Oblivion CLMT whose entry count was a multiple of 3
    /// (3 × 8 = 24, 6 × 8 = 48, 9 × 8 = 72 …): the buffer parsed as
    /// `N × 2/3` 12-byte entries with the second entry's form_id
    /// leaking into the first entry's `global` slot. The 3-entry case
    /// below is the smallest collision and the most likely modding
    /// shape (vanilla Oblivion ships none, but adding a few weather
    /// variants is a common DLC/OBSE pattern).
    #[test]
    fn parse_clmt_oblivion_three_entry_wlst_decodes_as_three() {
        let mut wlst_data = Vec::new();
        // 3 × 8-byte entries — 24 bytes total, divisible by 12.
        wlst_data.extend_from_slice(&0xAAAA_AAAAu32.to_le_bytes()); // weather 1
        wlst_data.extend_from_slice(&50i32.to_le_bytes());
        wlst_data.extend_from_slice(&0xBBBB_BBBBu32.to_le_bytes()); // weather 2
        wlst_data.extend_from_slice(&30i32.to_le_bytes());
        wlst_data.extend_from_slice(&0xCCCC_CCCCu32.to_le_bytes()); // weather 3
        wlst_data.extend_from_slice(&20i32.to_le_bytes());
        assert_eq!(wlst_data.len(), 24);
        assert_eq!(
            wlst_data.len() % 12,
            0,
            "the autodetect-collision case requires len % 12 == 0"
        );

        let subs = vec![make_sub(b"WLST", wlst_data)];
        let c = parse_clmt(0xCAFE, &subs, GameKind::Oblivion, &None);
        assert_eq!(
            c.weathers.len(),
            3,
            "Oblivion 3 × 8-byte WLST must decode as 3 entries (not 2 \
             with garbage). Pre-#540 this returned 2 entries with \
             [0]=AAAA chance=50 then [1]=garbage(0xBBBB)+30."
        );
        assert_eq!(c.weathers[0].weather_form_id, 0xAAAA_AAAA);
        assert_eq!(c.weathers[0].chance, 50);
        assert_eq!(c.weathers[1].weather_form_id, 0xBBBB_BBBB);
        assert_eq!(c.weathers[1].chance, 30);
        assert_eq!(c.weathers[2].weather_form_id, 0xCCCC_CCCC);
        assert_eq!(c.weathers[2].chance, 20);
    }

    /// Regression: M33-08 / #540 — the same 3 × 12-byte buffer parsed
    /// against Fallout3NV must decode as 2 entries (the buffer's
    /// 24 bytes / 12 = 2 entries with one trailing-12-byte
    /// truncation), pinning that the per-game dispatch doesn't
    /// regress the FO3+ path. `extract_records` paid no attention to
    /// the autodetect; the dispatch is purely on the entry-size axis.
    #[test]
    fn parse_clmt_fo3nv_24_byte_wlst_decodes_as_two_12_byte_entries() {
        let mut wlst_data = Vec::new();
        // 2 × 12-byte entries = 24 bytes — same buffer length as the
        // Oblivion 3-entry case above.
        wlst_data.extend_from_slice(&0xAAAA_AAAAu32.to_le_bytes());
        wlst_data.extend_from_slice(&60i32.to_le_bytes());
        wlst_data.extend_from_slice(&0u32.to_le_bytes()); // global
        wlst_data.extend_from_slice(&0xBBBB_BBBBu32.to_le_bytes());
        wlst_data.extend_from_slice(&40i32.to_le_bytes());
        wlst_data.extend_from_slice(&0u32.to_le_bytes());
        assert_eq!(wlst_data.len(), 24);

        let subs = vec![make_sub(b"WLST", wlst_data)];
        let c = parse_clmt(0xCAFF, &subs, GameKind::Fallout3NV, &None);
        assert_eq!(c.weathers.len(), 2);
        assert_eq!(c.weathers[0].weather_form_id, 0xAAAA_AAAA);
        assert_eq!(c.weathers[1].weather_form_id, 0xBBBB_BBBB);
    }
}
