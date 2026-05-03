//! Walker functions extracted from ../mod.rs (stage B refactor).
//!
//! Functions: parse_modl_group, parse_ltex_group, parse_txst_group, parse_scol_group, parse_pkin_group, parse_movs_group, parse_mswp_group.

use super::helpers::read_zstring;
use super::*;
use crate::esm::reader::SubRecord;

/// Build a [`StaticObject`] from a record's already-decoded sub-records.
///
/// Pulled out of `parse_modl_group`'s inner loop in #527 so the records-
/// side walker can share the same MODL-extraction logic when handling
/// dual-target labels (WEAP/ARMO/AMMO/MISC/KEYM/ALCH/INGR/BOOK/NOTE/
/// CONT/NPC_/CREA/ACTI/TERM — every label that ships both a typed
/// record AND wants `cells.statics` populated for visual placement).
/// Pre-#527 the records walker re-decoded these groups end-to-end on a
/// second full pass; the fused walker calls `read_sub_records` once and
/// dispatches both consumers from the same `subs` slice.
///
/// Returns `None` for records that carry neither a model path, a LIGH
/// `DATA` chunk, nor an ADDN `DATA`/`DNAM` payload — those would
/// produce an empty `StaticObject` that the cell loader ignores anyway.
pub(crate) fn build_static_object_from_subs(
    form_id: u32,
    record_type: &[u8; 4],
    subs: &[SubRecord],
) -> Option<StaticObject> {
    let is_ligh = record_type == b"LIGH";
    let is_addn = record_type == b"ADDN";
    let mut editor_id = String::new();
    let mut model_path = String::new();
    let mut light_data = None;
    let mut addon_index: Option<i32> = None;
    let mut addon_dnam: Option<(u16, u16)> = None;
    let mut has_script = false;
    let mut xpwr_form_id: Option<u32> = None;

    for sub in subs {
        match &sub.sub_type {
            b"EDID" => editor_id = read_zstring(&sub.data),
            b"MODL" => model_path = read_zstring(&sub.data),
            b"VMAD" => has_script = true,
            b"DATA" if is_ligh && sub.data.len() >= 12 => {
                let radius = u32::from_le_bytes([
                    sub.data[4],
                    sub.data[5],
                    sub.data[6],
                    sub.data[7],
                ]) as f32;
                let r = sub.data[8] as f32 / 255.0;
                let g = sub.data[9] as f32 / 255.0;
                let b = sub.data[10] as f32 / 255.0;
                let flags = if sub.data.len() >= 16 {
                    u32::from_le_bytes([
                        sub.data[12],
                        sub.data[13],
                        sub.data[14],
                        sub.data[15],
                    ])
                } else {
                    0
                };
                light_data = Some(LightData {
                    radius,
                    color: [r, g, b],
                    flags,
                    xpwr_form_id: None,
                });
            }
            b"XPWR" if is_ligh && sub.data.len() >= 4 => {
                xpwr_form_id = Some(u32::from_le_bytes([
                    sub.data[0],
                    sub.data[1],
                    sub.data[2],
                    sub.data[3],
                ]));
            }
            b"DATA" if is_addn && sub.data.len() >= 4 => {
                addon_index = Some(i32::from_le_bytes([
                    sub.data[0],
                    sub.data[1],
                    sub.data[2],
                    sub.data[3],
                ]));
            }
            b"DNAM" if is_addn && sub.data.len() >= 4 => {
                let cap = u16::from_le_bytes([sub.data[0], sub.data[1]]);
                let flags = u16::from_le_bytes([sub.data[2], sub.data[3]]);
                addon_dnam = Some((cap, flags));
            }
            _ => {}
        }
    }

    if let (Some(ref mut ld), Some(form)) = (&mut light_data, xpwr_form_id) {
        ld.xpwr_form_id = Some(form);
    }

    let addon_data = if is_addn && (addon_index.is_some() || addon_dnam.is_some()) {
        let (master_particle_cap, flags) = addon_dnam.unwrap_or((0, 0));
        Some(AddonData {
            addon_index: addon_index.unwrap_or(0),
            master_particle_cap,
            flags,
        })
    } else {
        None
    };

    if !model_path.is_empty() || light_data.is_some() || addon_data.is_some() {
        Some(StaticObject {
            form_id,
            editor_id,
            model_path,
            // #renderlayer — capture the base record's four-CC so the
            // cell-loader can classify the spawned entity into a
            // RenderLayer (Architecture / Clutter / Actor) for the
            // depth-bias ladder. See `RecordType::render_layer`.
            record_type: crate::record::RecordType(*record_type),
            light_data,
            addon_data,
            has_script,
        })
    } else {
        None
    }
}

/// Walk a top-level record group and extract any record with a MODL sub-record.
/// Works for STAT, MSTT, FURN, DOOR, ACTI, CONT, LIGH, MISC, etc.
pub(crate) fn parse_modl_group(
    reader: &mut EsmReader,
    end: usize,
    statics: &mut HashMap<u32, StaticObject>,
) -> Result<()> {
    while reader.position() < end && reader.remaining() > 0 {
        if reader.is_group() {
            let sub = reader.read_group_header()?;
            let sub_end = reader.group_content_end(&sub);
            parse_modl_group(reader, sub_end, statics)?;
            continue;
        }

        let header = reader.read_record_header()?;
        let subs = reader.read_sub_records(&header)?;
        if let Some(stat) =
            build_static_object_from_subs(header.form_id, &header.record_type, &subs)
        {
            statics.insert(header.form_id, stat);
        }
    }
    Ok(())
}

/// Parse LTEX (Landscape Texture) records.
///
/// FO3/FNV: LTEX has a TNAM sub-record pointing to a TXST form ID.
/// Oblivion: LTEX has an ICON sub-record with a direct texture path.
pub(crate) fn parse_ltex_group(
    reader: &mut EsmReader,
    end: usize,
    ltex_to_txst: &mut HashMap<u32, u32>,
    direct_paths: &mut HashMap<u32, String>,
) -> Result<()> {
    while reader.position() < end && reader.remaining() > 0 {
        if reader.is_group() {
            let sub = reader.read_group_header()?;
            let sub_end = reader.group_content_end(&sub);
            parse_ltex_group(reader, sub_end, ltex_to_txst, direct_paths)?;
            continue;
        }

        let header = reader.read_record_header()?;
        if &header.record_type == b"LTEX" {
            let subs = reader.read_sub_records(&header)?;
            for sub in &subs {
                match sub.sub_type.as_slice() {
                    // FO3/FNV/Skyrim: TNAM → TXST form ID.
                    b"TNAM" if sub.data.len() >= 4 => {
                        let txst_id = u32::from_le_bytes([
                            sub.data[0],
                            sub.data[1],
                            sub.data[2],
                            sub.data[3],
                        ]);
                        ltex_to_txst.insert(header.form_id, txst_id);
                    }
                    // Oblivion: ICON → direct texture path.
                    b"ICON" => {
                        let path = read_zstring(&sub.data);
                        if !path.is_empty() {
                            direct_paths.insert(header.form_id, path);
                        }
                    }
                    _ => {}
                }
            }
        } else {
            reader.skip_record(&header);
        }
    }
    Ok(())
}

/// Parse TXST (Texture Set) records. Extracts all 8 texture slots
/// (TX00..TX07) into a [`TextureSet`] entry, plus the legacy
/// `txst_textures: form_id → diffuse_path` map kept for the LTEX
/// resolver downstream. Pre-#357 only TX00 was retained — REFR
/// XTNM/XPRD overrides referencing a TXST silently dropped 7 of 8
/// channels (visible on Skyrim re-skinned statics as "wrong material
/// on a re-textured prop"). See audit S6-11.
pub(crate) fn parse_txst_group(
    reader: &mut EsmReader,
    end: usize,
    txst_textures: &mut HashMap<u32, String>,
    texture_sets: &mut HashMap<u32, TextureSet>,
) -> Result<()> {
    while reader.position() < end && reader.remaining() > 0 {
        if reader.is_group() {
            let sub = reader.read_group_header()?;
            let sub_end = reader.group_content_end(&sub);
            parse_txst_group(reader, sub_end, txst_textures, texture_sets)?;
            continue;
        }

        let header = reader.read_record_header()?;
        if &header.record_type == b"TXST" {
            let subs = reader.read_sub_records(&header)?;
            let mut set = TextureSet::default();
            for sub in &subs {
                // Helper: extract a non-empty zstring path for one slot.
                let extract = |bytes: &[u8]| -> Option<String> {
                    let s = read_zstring(bytes);
                    if s.is_empty() {
                        None
                    } else {
                        Some(s)
                    }
                };
                match sub.sub_type.as_slice() {
                    b"TX00" => set.diffuse = extract(&sub.data),
                    b"TX01" => set.normal = extract(&sub.data),
                    b"TX02" => set.glow = extract(&sub.data),
                    b"TX03" => set.height = extract(&sub.data),
                    b"TX04" => set.env = extract(&sub.data),
                    b"TX05" => set.env_mask = extract(&sub.data),
                    b"TX06" => set.inner = extract(&sub.data),
                    b"TX07" => set.specular = extract(&sub.data),
                    // FO4+ BGSM material path. 37 % of vanilla
                    // `Fallout4.esm` TXST records (140 / 382) are
                    // MNAM-only with no TX00 at all; pre-#406 they were
                    // silently dropped because the outer `if set !=
                    // default()` guard would fail and `txst_textures`
                    // never got a diffuse fallback either. See #406.
                    b"MNAM" => set.material_path = extract(&sub.data),
                    _ => {}
                }
            }
            // Backward-compat LTEX resolver: legacy diffuse-only map.
            if let Some(diffuse) = set.diffuse.as_ref() {
                txst_textures.insert(header.form_id, diffuse.clone());
            }
            // Skip the all-empty case (a TXST with no readable slots
            // is uninteresting and would just bloat the map).
            if set != TextureSet::default() {
                texture_sets.insert(header.form_id, set);
            }
        } else {
            reader.skip_record(&header);
        }
    }
    Ok(())
}

/// Parse SCOL (Static Collection) records. Each record is captured
/// both in the legacy `statics` map (so REFRs targeting the SCOL
/// still resolve its cached combined mesh via MODL) and in the new
/// `scols` map which carries the full ONAM/DATA child-placement
/// data the cell loader needs to expand mod-added SCOLs whose
/// cached `CM*.NIF` isn't shipped. Pre-#405 SCOLs were routed
/// through `parse_modl_group` and the placement arrays were
/// discarded. See audit FO4-D4-C2.
pub(crate) fn parse_scol_group(
    reader: &mut EsmReader,
    end: usize,
    statics: &mut HashMap<u32, StaticObject>,
    scols: &mut HashMap<u32, crate::esm::records::ScolRecord>,
) -> Result<()> {
    while reader.position() < end && reader.remaining() > 0 {
        if reader.is_group() {
            let sub = reader.read_group_header()?;
            let sub_end = reader.group_content_end(&sub);
            parse_scol_group(reader, sub_end, statics, scols)?;
            continue;
        }

        let header = reader.read_record_header()?;
        if &header.record_type == b"SCOL" {
            let subs = reader.read_sub_records(&header)?;
            let record = crate::esm::records::parse_scol(header.form_id, &subs);
            // Preserve the MODL-backed StaticObject entry so REFR
            // resolution against the SCOL form ID keeps finding the
            // cached combined mesh. Mirror `parse_modl_group`'s
            // (empty light_data / empty addon_data / has_script)
            // defaults — SCOL carries none of those.
            if !record.model_path.is_empty() || !record.editor_id.is_empty() {
                statics.insert(
                    header.form_id,
                    StaticObject {
                        form_id: header.form_id,
                        editor_id: record.editor_id.clone(),
                        model_path: record.model_path.clone(),
                        record_type: crate::record::RecordType::SCOL,
                        light_data: None,
                        addon_data: None,
                        // `parse_scol` doesn't currently capture VMAD
                        // presence — vanilla FO4 has no script-bearing
                        // SCOLs; revisit if mods add them.
                        has_script: false,
                    },
                );
            }
            scols.insert(header.form_id, record);
        } else {
            reader.skip_record(&header);
        }
    }
    Ok(())
}

/// Parse PKIN (Pack-In) records. Each record is captured in the
/// `packins` map with its CNAM-driven content-reference list, and
/// also gets a nominal `StaticObject` entry with an empty
/// `model_path` so REFR resolution still finds the base form at cell
/// load time — the cell loader uses "statics[base].model_path empty
/// AND base in packins" as the signal to expand into synthetic
/// placements.
///
/// Pre-#589 PKIN records were routed through the MODL-only parser
/// (which only pulls EDID when MODL is absent) and the CNAM content
/// list was silently dropped. Vanilla Fallout4.esm ships 872 PKIN
/// records — every FO4 workshop-content bundle REFR rendered as
/// nothing. See audit FO4-DIM4-03.
pub(crate) fn parse_pkin_group(
    reader: &mut EsmReader,
    end: usize,
    statics: &mut HashMap<u32, StaticObject>,
    packins: &mut HashMap<u32, crate::esm::records::PkinRecord>,
) -> Result<()> {
    while reader.position() < end && reader.remaining() > 0 {
        if reader.is_group() {
            let sub = reader.read_group_header()?;
            let sub_end = reader.group_content_end(&sub);
            parse_pkin_group(reader, sub_end, statics, packins)?;
            continue;
        }

        let header = reader.read_record_header()?;
        if &header.record_type == b"PKIN" {
            let subs = reader.read_sub_records(&header)?;
            let record = crate::esm::records::parse_pkin(header.form_id, &subs);
            // Register a nominal StaticObject so REFR base-form lookup
            // succeeds. Empty `model_path` + `contents.len() > 0` is
            // the cell loader's expansion trigger (see
            // `expand_pkin_placements`). Keeping the `editor_id`
            // populated lets debug logging surface the PKIN name when
            // a spawn fails to find the base.
            if !record.editor_id.is_empty() {
                statics.insert(
                    header.form_id,
                    StaticObject {
                        form_id: header.form_id,
                        editor_id: record.editor_id.clone(),
                        model_path: String::new(),
                        record_type: crate::record::RecordType::PKIN,
                        light_data: None,
                        addon_data: None,
                        has_script: false,
                    },
                );
            }
            packins.insert(header.form_id, record);
        } else {
            reader.skip_record(&header);
        }
    }
    Ok(())
}

/// Parse MOVS (Movable Static) records. Visually identical to STAT —
/// MOVS distinguishes itself by being driven by Havok at runtime — so
/// every record gets its standard `StaticObject` registration via the
/// MODL pointer (REFR base-form resolution stays unchanged) AND its
/// typed `MovableStaticRecord` shape lands on `EsmCellIndex::movables`
/// for downstream physics / sound / destruction wiring. Pre-#588 MOVS
/// was lumped into the MODL-only catch-all alongside STAT/FURN/etc.
/// which preserved visual placement but silently dropped the
/// distinguishing `LNAM`/`ZNAM`/`DEST`/`VMAD` sub-records.
///
/// Vanilla Fallout4.esm itself ships zero MOVS records — the impact is
/// felt on DLC / mod content that authors breakable furniture,
/// deployable workshop objects, and physics-puzzle props. See audit
/// `FO4-DIM4-02` / #588.
pub(crate) fn parse_movs_group(
    reader: &mut EsmReader,
    end: usize,
    statics: &mut HashMap<u32, StaticObject>,
    movables: &mut HashMap<u32, crate::esm::records::MovableStaticRecord>,
) -> Result<()> {
    while reader.position() < end && reader.remaining() > 0 {
        if reader.is_group() {
            let sub = reader.read_group_header()?;
            let sub_end = reader.group_content_end(&sub);
            parse_movs_group(reader, sub_end, statics, movables)?;
            continue;
        }

        let header = reader.read_record_header()?;
        if &header.record_type == b"MOVS" {
            let subs = reader.read_sub_records(&header)?;
            let record = crate::esm::records::parse_movs(header.form_id, &subs);
            // Preserve the MODL-backed StaticObject entry so REFR
            // resolution against the MOVS form ID keeps finding the
            // visual mesh. Mirror `parse_modl_group`'s defaults
            // (empty light/addon data; `has_script` flips on `VMAD`
            // presence). Skip records with neither EDID nor MODL —
            // those are header-only stubs that wouldn't render anyway.
            if !record.model_path.is_empty() || !record.editor_id.is_empty() {
                statics.insert(
                    header.form_id,
                    StaticObject {
                        form_id: header.form_id,
                        editor_id: record.editor_id.clone(),
                        model_path: record.model_path.clone(),
                        record_type: crate::record::RecordType::MOVS,
                        light_data: None,
                        addon_data: None,
                        has_script: record.has_script,
                    },
                );
            }
            movables.insert(header.form_id, record);
        } else {
            reader.skip_record(&header);
        }
    }
    Ok(())
}

/// Walk an MSWP group and parse every `MSWP` record into the
/// `material_swaps` map. Sub-groups (rare in vanilla but common in
/// mods that nest under MSWP) recurse like every other group walker
/// in this file. Pre-#590 the entire group was `skip_group`'d so all
/// ~2,500 vanilla Fallout4.esm material-swap tables were silently
/// discarded — every Raider armour, station-wagon rust variant, and
/// vault-decay overlay rendered identically across REFRs.
///
/// Stores nothing on `statics` — MSWP isn't a placeable base form,
/// only a substitution table consumed at REFR-spawn time when the
/// REFR carries `XMSP`. See audit FO4-DIM6-05.
pub(crate) fn parse_mswp_group(
    reader: &mut EsmReader,
    end: usize,
    material_swaps: &mut HashMap<u32, crate::esm::records::MaterialSwapRecord>,
) -> Result<()> {
    while reader.position() < end && reader.remaining() > 0 {
        if reader.is_group() {
            let sub = reader.read_group_header()?;
            let sub_end = reader.group_content_end(&sub);
            parse_mswp_group(reader, sub_end, material_swaps)?;
            continue;
        }

        let header = reader.read_record_header()?;
        if &header.record_type == b"MSWP" {
            let subs = reader.read_sub_records(&header)?;
            let record = crate::esm::records::parse_mswp(header.form_id, &subs);
            material_swaps.insert(header.form_id, record);
        } else {
            reader.skip_record(&header);
        }
    }
    Ok(())
}
