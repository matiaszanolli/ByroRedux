//! Walker functions extracted from ../mod.rs (stage B refactor).
//!
//! Functions: parse_modl_group, parse_ltex_group, parse_txst_group, parse_scol_group, parse_pkin_group, parse_movs_group, parse_mswp_group.

use super::helpers::{read_mesh_path, read_zstring};
use super::*;
use crate::esm::reader::{GameKind, SubRecord};

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
/// `DATA` (Skyrim/FO4) or `DAT2` (Starfield, #1567) light chunk, nor an
/// ADDN `DATA`/`DNAM` payload — those would produce an empty
/// `StaticObject` that the cell loader ignores anyway.
///
/// Only the top-level `MODL` is read (#1576 / SF-D4-03). Some Starfield
/// STAT/ACTI/ARMO forms with no `MODL` DO carry a model reference inside
/// a `BFCB`/`BFCE`-tagged `TESModel_Component` in their generic
/// `BaseFormComponents` array instead — confirmed against the
/// authoritative `wbDefinitionsSF1.pas` (TES5Edit dev-4.1.6):
/// `wbBaseFormComponents`'s component-type enum lists `TESModel_Component`
/// by name. But — same blocker class as #1567's LIGH `DAT2` decode before
/// its schema landed — that same reference only names the tag; it has no
/// field-level breakdown of what's inside the component's BFCB payload
/// (xEdit's own per-component decoders are reflection-derived, dumped
/// from a live running Starfield process, not present in any static
/// schema in this repo's reference set). Decoding it without that layout
/// would be exactly the offset-guessing the no-guessing policy forbids.
/// Affects ~140 REFRs / ~0.5% of one cell, mostly very-low-FormID
/// marker/template STAT forms. Do not add a speculative decoder here —
/// wait for a reflection dump or a validated reverse-engineering pass.
pub(crate) fn build_static_object_from_subs(
    form_id: u32,
    record_type: &[u8; 4],
    visible_when_distant: bool,
    subs: &[SubRecord],
    remap: &Option<crate::esm::reader::FormIdRemap>,
) -> Option<StaticObject> {
    let is_ligh = record_type == b"LIGH";
    let is_addn = record_type == b"ADDN";
    // Skyrim+ ARMO records overload MODL with a fixed-width FormID pointing
    // at an ARMA record. The typed item parser resolves that reference via
    // `EsmIndex::armor_addons`; the cell-side StaticObject has no gender/race
    // context and must therefore not reinterpret it as a mesh path (#3056).
    let is_armor = record_type == b"ARMO";
    let mut editor_id = String::new();
    let mut model_path = String::new();
    let mut light_data = None;
    let mut addon_index: Option<i32> = None;
    let mut addon_dnam: Option<(u16, u16)> = None;
    let mut has_script = false;
    let mut script_instance = None;
    let mut xpwr_form_id: Option<u32> = None;

    for sub in subs {
        match &sub.sub_type {
            b"EDID" => editor_id = read_zstring(&sub.data),
            b"MODL" if is_armor && sub.data.len() == 4 => {
                // Consumed by `parse_armo` as an ARMA FormID. ARMO records
                // are inventory definitions, not world-placement meshes, so
                // there is no StaticObject path to populate here.
            }
            b"MODL" => match read_mesh_path(&sub.data) {
                Ok(p) => model_path = p,
                // #1620 — a MODL holding control bytes is a non-string value
                // (FormID-shaped u32) mis-read as a path. Warn (the old path
                // was silent) and leave `model_path` empty so the REFR is
                // treated as model-less rather than caching a garbage key.
                Err(bad) => log::warn!(
                    "#1620 — {} {:08X}: corrupt MODL mesh path (control bytes), \
                     treating as model-less: {:?}",
                    std::str::from_utf8(record_type).unwrap_or("????"),
                    form_id,
                    bad,
                ),
            },
            // #2663 (SCR-D7-NEW11-02) — decode the full VMAD payload,
            // mirroring `CommonNamedFields::from_subs`'s `VMAD` arm. This
            // was presence-only (`has_script = true`, payload dropped)
            // until this fix; the sibling gap to #2189, which taught
            // `CommonItemFields` the same decode for the item family.
            b"VMAD" => {
                has_script = true;
                script_instance = Some(
                    super::super::records::script_instance::ScriptInstanceData::parse_with_remap(
                        &sub.data, remap,
                    ),
                );
            }
            b"DATA" if is_ligh && sub.data.len() >= 12 => {
                let radius =
                    u32::from_le_bytes([sub.data[4], sub.data[5], sub.data[6], sub.data[7]]) as f32;
                let r = sub.data[8] as f32 / 255.0;
                let g = sub.data[9] as f32 / 255.0;
                let b = sub.data[10] as f32 / 255.0;
                let flags = if sub.data.len() >= 16 {
                    u32::from_le_bytes([sub.data[12], sub.data[13], sub.data[14], sub.data[15]])
                } else {
                    0
                };
                // Layout per UESP / xEdit `wbDefinitionsSkyrim`, identical
                // in the falloff/FOV prefix to FO3/FNV/FO4, but the two
                // families DIVERGE from byte 24 on and end at different
                // total lengths — this is NOT one struct truncated to
                // different points, it is two different structs that
                // happen to share a 24-byte prefix:
                //   Skyrim+ (48 bytes total):
                //     16-19 falloff exponent (see `LightData.falloff_exponent`)
                //     20-23 FOV (spot light)
                //     24-27 near clip
                //     28-31 flicker period
                //     32-35 intensity amplitude
                //     36-39 movement amplitude
                //     40-43 value, 44-47 weight
                //   Oblivion/FO3/FNV (32 bytes total):
                //     16-19 falloff exponent, 20-23 FOV (same prefix)
                //     24-27 value, 28-31 weight  ← NOT flicker period
                // #2478 / REN-D22-03 — a 32-byte pre-Skyrim record passes
                // `len >= 32` just as validly as a full Skyrim record does,
                // so gating flicker-period/amplitude reads on `len >= 32`
                // (their Skyrim-layout byte offset) read the pre-Skyrim
                // record's Value/Weight fields as flicker data. Bethesda's
                // own tooling always emits the FULL fixed-size struct for
                // whichever layout a game uses (no partial-length Skyrim
                // records exist in the wild), so gate on the complete
                // Skyrim-layout length (48) instead of the byte offset
                // alone — anything shorter is the pre-Skyrim struct, which
                // authors no flicker parameters at all.
                let read_f32 = |off: usize| -> f32 {
                    f32::from_le_bytes([
                        sub.data[off],
                        sub.data[off + 1],
                        sub.data[off + 2],
                        sub.data[off + 3],
                    ])
                };
                let falloff_exponent = if sub.data.len() >= 20 {
                    read_f32(16)
                } else {
                    0.0
                };
                // #2439 / NIFAL-D2-01 — FOV sits in the shared 24-byte
                // prefix (bytes 20-23), present at the same offset in
                // BOTH the pre-Skyrim 32-byte and Skyrim+ 48-byte
                // layouts (unlike period/amplitude below, which are
                // Skyrim+-only). Gate purely on length, not
                // `is_skyrim_plus_layout`.
                let fov_degrees = if sub.data.len() >= 24 {
                    read_f32(20)
                } else {
                    0.0
                };
                let is_skyrim_plus_layout = sub.data.len() >= 48;
                let period_secs = if is_skyrim_plus_layout {
                    read_f32(28)
                } else {
                    0.0
                };
                let intensity_amplitude = if is_skyrim_plus_layout {
                    read_f32(32)
                } else {
                    0.0
                };
                let movement_amplitude = if is_skyrim_plus_layout {
                    read_f32(36)
                } else {
                    0.0
                };
                light_data = Some(LightData {
                    radius,
                    color: [r, g, b],
                    flags,
                    falloff_exponent,
                    period_secs,
                    intensity_amplitude,
                    movement_amplitude,
                    fov_degrees,
                    xpwr_form_id: None,
                });
            }
            // Starfield LIGH light data (#1567). Starfield LIGH records
            // carry no `DATA` and no top-level `MODL`; the light
            // parameters live in a `DAT2` (76-byte) subrecord with a
            // layout distinct from the Skyrim/FO4 `DATA` arm above.
            // Without this arm every Starfield LIGH returned `None`, was
            // never inserted into `cells.statics`, and the 656 Cydonia
            // REFRs pointing at 62 LIGH forms missed at the static lookup
            // and were silently skipped — the cell rendered markedly
            // under-lit (only NIF-embedded lights + XCLL ambient survived).
            //
            // Byte layout verified against xEdit `wbDefinitionsSF1.pas`
            // (`wbRecord(LIGH … wbStruct(DAT2, 'Data', [...]))`), NOT
            // guessed — the offsets differ from Skyrim's `DATA`:
            //   { 4} Float      Radius   (Skyrim `DATA` stores this as u32)
            //   { 8} ByteColors Color    (RGBA; RGB at 8/9/10)
            //   {12} UInt16     Flags    (Skyrim `DATA` stores u32)
            //   {16} Float      Falloff Exponent
            //   {20} Float      FOV (#2439 / NIFAL-D2-01 — same relative
            //                   position as the Skyrim/FO4 DATA arm above)
            //   {28} Float      Flicker Period
            //   {32} Float      Intensity Amplitude
            //   {36} Float      Movement Amplitude
            // (Near-clip / PBR temperature+lumens / adaptive-light tail at
            // 24..76, excluding 28-39 above, are parsed by xEdit but not yet
            // consumed by our `LightData`.) Gating on the `DAT2` signature is
            // itself the Starfield discriminator — Skyrim/FO4 LIGH use
            // `DATA`, so the `DATA` arm above stays the FO4/Skyrim path
            // untouched.
            b"DAT2" if is_ligh && sub.data.len() >= 11 => {
                let read_f32 = |off: usize| -> f32 {
                    f32::from_le_bytes([
                        sub.data[off],
                        sub.data[off + 1],
                        sub.data[off + 2],
                        sub.data[off + 3],
                    ])
                };
                let radius = read_f32(4);
                let r = sub.data[8] as f32 / 255.0;
                let g = sub.data[9] as f32 / 255.0;
                let b = sub.data[10] as f32 / 255.0;
                let flags = if sub.data.len() >= 14 {
                    u16::from_le_bytes([sub.data[12], sub.data[13]]) as u32
                } else {
                    0
                };
                let falloff_exponent = if sub.data.len() >= 20 {
                    read_f32(16)
                } else {
                    0.0
                };
                // #2439 / NIFAL-D2-01 — same offset as the DATA arm above.
                let fov_degrees = if sub.data.len() >= 24 {
                    read_f32(20)
                } else {
                    0.0
                };
                let period_secs = if sub.data.len() >= 32 {
                    read_f32(28)
                } else {
                    0.0
                };
                let intensity_amplitude = if sub.data.len() >= 36 {
                    read_f32(32)
                } else {
                    0.0
                };
                let movement_amplitude = if sub.data.len() >= 40 {
                    read_f32(36)
                } else {
                    0.0
                };
                light_data = Some(LightData {
                    radius,
                    color: [r, g, b],
                    flags,
                    falloff_exponent,
                    period_secs,
                    intensity_amplitude,
                    movement_amplitude,
                    fov_degrees,
                    xpwr_form_id: None,
                });
            }
            b"XPWR" if is_ligh && sub.data.len() >= 4 => {
                // #3314 — a REFR reference like any other. No ECS consumer
                // reads it yet (settlement power circuits), but it is stored
                // for one, so it is remapped at decode time rather than
                // leaving a raw value for a future consumer to trip over.
                let raw = u32::from_le_bytes([sub.data[0], sub.data[1], sub.data[2], sub.data[3]]);
                xpwr_form_id = Some(remap.as_ref().map_or(raw, |r| r.remap(raw)));
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
            script_instance,
            visible_when_distant,
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
    parse_modl_group_inner(reader, end, statics, 0)
}

fn parse_modl_group_inner(
    reader: &mut EsmReader,
    end: usize,
    statics: &mut HashMap<u32, StaticObject>,
    depth: u32,
) -> Result<()> {
    let remap = reader.get_form_id_remap();
    while reader.position() < end && reader.remaining() > 0 {
        if reader.is_group() {
            let sub = reader.read_group_header()?;
            let Some(sub_end) = reader.bounded_group_content_end(&sub, depth, "parse_modl_group")
            else {
                continue;
            };
            parse_modl_group_inner(reader, sub_end, statics, depth + 1)?;
            continue;
        }

        let header = reader.read_record_header()?;
        let subs = reader.read_sub_records(&header)?;
        if let Some(stat) = build_static_object_from_subs(
            header.form_id,
            &header.record_type,
            header.is_visible_when_distant(),
            &subs,
            &remap,
        ) {
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
    parse_ltex_group_inner(reader, end, ltex_to_txst, direct_paths, 0)
}

fn parse_ltex_group_inner(
    reader: &mut EsmReader,
    end: usize,
    ltex_to_txst: &mut HashMap<u32, u32>,
    direct_paths: &mut HashMap<u32, String>,
    depth: u32,
) -> Result<()> {
    while reader.position() < end && reader.remaining() > 0 {
        if reader.is_group() {
            let sub = reader.read_group_header()?;
            let Some(sub_end) = reader.bounded_group_content_end(&sub, depth, "parse_ltex_group")
            else {
                continue;
            };
            parse_ltex_group_inner(reader, sub_end, ltex_to_txst, direct_paths, depth + 1)?;
            continue;
        }

        let header = reader.read_record_header()?;
        if &header.record_type == b"LTEX" {
            let subs = reader.read_sub_records(&header)?;
            for sub in &subs {
                match sub.sub_type.as_slice() {
                    // FO3/FNV/Skyrim: TNAM → TXST form ID.
                    b"TNAM" if sub.data.len() >= 4 => {
                        // #3314 — the map KEY (`header.form_id`) is already
                        // remapped by `read_record_header`; the VALUE is a
                        // TXST reference and must be remapped too, or the
                        // `txst_textures` lookup misses on every DLC splat.
                        let txst_id = reader.remap_form_id(u32::from_le_bytes([
                            sub.data[0],
                            sub.data[1],
                            sub.data[2],
                            sub.data[3],
                        ]));
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

/// Parse TXST (Texture Set) records. Resolves TX00..TX07 into named
/// [`TextureSet`] roles, plus the legacy
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
    game: GameKind,
) -> Result<()> {
    parse_txst_group_inner(reader, end, txst_textures, texture_sets, game, 0)
}

fn parse_txst_group_inner(
    reader: &mut EsmReader,
    end: usize,
    txst_textures: &mut HashMap<u32, String>,
    texture_sets: &mut HashMap<u32, TextureSet>,
    game: GameKind,
    depth: u32,
) -> Result<()> {
    while reader.position() < end && reader.remaining() > 0 {
        if reader.is_group() {
            let sub = reader.read_group_header()?;
            let Some(sub_end) = reader.bounded_group_content_end(&sub, depth, "parse_txst_group")
            else {
                continue;
            };
            parse_txst_group_inner(
                reader,
                sub_end,
                txst_textures,
                texture_sets,
                game,
                depth + 1,
            )?;
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
                    // TXST is *not* stored in BSShaderTextureSet order.
                    // FO3/FNV + Skyrim: TX02 is environment mask /
                    // subsurface tint. FO4/FO76 renamed that lane to
                    // wrinkles. Starfield's xEdit definition has not seen
                    // TX02 in shipped content; retain the FO4-era meaning
                    // if mod content authors one rather than fabricating an
                    // environment mask.
                    b"TX02" => {
                        let path = extract(&sub.data);
                        if matches!(
                            game,
                            GameKind::Fallout4 | GameKind::Fallout76 | GameKind::Starfield
                        ) {
                            set.wrinkle = path;
                        } else {
                            set.env_mask = path;
                        }
                    }
                    b"TX03" => set.glow = extract(&sub.data),
                    b"TX04" => set.height = extract(&sub.data),
                    b"TX05" => set.env = extract(&sub.data),
                    b"TX06" => set.inner = extract(&sub.data),
                    b"TX07" => set.specular = extract(&sub.data),
                    // FO4+ BGSM material path. 37 % of vanilla
                    // `Fallout4.esm` TXST records (140 / 382) are
                    // MNAM-only with no TX00 at all; pre-#406 they were
                    // silently dropped because the outer `if set !=
                    // default()` guard would fail and `txst_textures`
                    // never got a diffuse fallback either. See #406.
                    b"MNAM" => set.material_path = extract(&sub.data),
                    // TXST flags (`DNAM`). FO4 = 2 bytes, Skyrim = 1
                    // byte; capture as u16 with the Skyrim path landing
                    // in the low byte. 100 % of vanilla Fallout4.esm
                    // TXSTs ship a DNAM. See #814.
                    b"DNAM" if !sub.data.is_empty() => {
                        set.flags = if sub.data.len() >= 2 {
                            u16::from_le_bytes([sub.data[0], sub.data[1]])
                        } else {
                            sub.data[0] as u16
                        };
                    }
                    // TXST decal-data (`DODT`). Fixed 36-byte layout
                    // per UESP / xEdit `wbDefinitionsFO4`. 207 / 382
                    // vanilla Fallout4.esm TXSTs (every decal-bearing
                    // record) ship a DODT — pre-#813 silently dropped.
                    b"DODT" if sub.data.len() >= 36 => {
                        let d = &sub.data;
                        let f32_at = |o: usize| -> f32 {
                            f32::from_le_bytes([d[o], d[o + 1], d[o + 2], d[o + 3]])
                        };
                        set.decal_data = Some(crate::esm::cell::DecalData {
                            min_width: f32_at(0),
                            max_width: f32_at(4),
                            min_height: f32_at(8),
                            max_height: f32_at(12),
                            depth: f32_at(16),
                            shininess: f32_at(20),
                            parallax_scale: f32_at(24),
                            parallax_passes: d[28],
                            flags: d[29],
                            // d[30..32] is unused per xEdit.
                            color: [d[32], d[33], d[34], d[35]],
                        });
                    }
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
    parse_scol_group_inner(reader, end, statics, scols, 0)
}

fn parse_scol_group_inner(
    reader: &mut EsmReader,
    end: usize,
    statics: &mut HashMap<u32, StaticObject>,
    scols: &mut HashMap<u32, crate::esm::records::ScolRecord>,
    depth: u32,
) -> Result<()> {
    while reader.position() < end && reader.remaining() > 0 {
        if reader.is_group() {
            let sub = reader.read_group_header()?;
            let Some(sub_end) = reader.bounded_group_content_end(&sub, depth, "parse_scol_group")
            else {
                continue;
            };
            parse_scol_group_inner(reader, sub_end, statics, scols, depth + 1)?;
            continue;
        }

        let header = reader.read_record_header()?;
        if &header.record_type == b"SCOL" {
            let subs = reader.read_sub_records(&header)?;
            // #3400 — same `reader.get_form_id_remap()` its sibling
            // `parse_modl_group` already resolves; SCOL's ONAM/FLTR
            // children are keyed in global space like every other index
            // map. Read per record so a nested GRUP recursion can't
            // outlive the reader's current plugin.
            let remap = reader.get_form_id_remap();
            let record = crate::esm::records::parse_scol(header.form_id, &subs, &remap);
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
                        // `parse_scol` now scans for VMAD presence per
                        // #1178 / FO4-D4-001. Vanilla FO4 has no
                        // script-bearing SCOLs; mod content can attach
                        // (animated decals, conditional visibility, mod
                        // physics). Propagate so Papyrus event dispatch
                        // doesn't skip scripted SCOL placements.
                        has_script: record.has_script,
                        // #2663 — `parse_scol` doesn't decode VMAD past
                        // presence (out of this issue's scope; no vanilla
                        // SCOL carries one). Not the same gap as STAT/etc.
                        script_instance: None,
                        visible_when_distant: header.is_visible_when_distant(),
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
    parse_pkin_group_inner(reader, end, statics, packins, 0)
}

fn parse_pkin_group_inner(
    reader: &mut EsmReader,
    end: usize,
    statics: &mut HashMap<u32, StaticObject>,
    packins: &mut HashMap<u32, crate::esm::records::PkinRecord>,
    depth: u32,
) -> Result<()> {
    while reader.position() < end && reader.remaining() > 0 {
        if reader.is_group() {
            let sub = reader.read_group_header()?;
            let Some(sub_end) = reader.bounded_group_content_end(&sub, depth, "parse_pkin_group")
            else {
                continue;
            };
            parse_pkin_group_inner(reader, sub_end, statics, packins, depth + 1)?;
            continue;
        }

        let header = reader.read_record_header()?;
        if &header.record_type == b"PKIN" {
            let subs = reader.read_sub_records(&header)?;
            // #3400 — see `parse_scol_group`.
            let remap = reader.get_form_id_remap();
            let record = crate::esm::records::parse_pkin(header.form_id, &subs, &remap);
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
                        script_instance: None,
                        // Nominal expansion-trigger entry (empty model_path);
                        // the flag rides the real PKIN header for completeness,
                        // though the synthetic child placements are what render.
                        visible_when_distant: header.is_visible_when_distant(),
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
    parse_movs_group_inner(reader, end, statics, movables, 0)
}

fn parse_movs_group_inner(
    reader: &mut EsmReader,
    end: usize,
    statics: &mut HashMap<u32, StaticObject>,
    movables: &mut HashMap<u32, crate::esm::records::MovableStaticRecord>,
    depth: u32,
) -> Result<()> {
    while reader.position() < end && reader.remaining() > 0 {
        if reader.is_group() {
            let sub = reader.read_group_header()?;
            let Some(sub_end) = reader.bounded_group_content_end(&sub, depth, "parse_movs_group")
            else {
                continue;
            };
            parse_movs_group_inner(reader, sub_end, statics, movables, depth + 1)?;
            continue;
        }

        let header = reader.read_record_header()?;
        if &header.record_type == b"MOVS" {
            let subs = reader.read_sub_records(&header)?;
            // #3401 — see `parse_scol_group`.
            let remap = reader.get_form_id_remap();
            let record = crate::esm::records::parse_movs(header.form_id, &subs, &remap);
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
                        // #2663 — `parse_movs` doesn't decode VMAD past
                        // presence (out of this issue's scope; vanilla
                        // Fallout4.esm ships zero MOVS records).
                        script_instance: None,
                        visible_when_distant: header.is_visible_when_distant(),
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
    parse_mswp_group_inner(reader, end, material_swaps, 0)
}

fn parse_mswp_group_inner(
    reader: &mut EsmReader,
    end: usize,
    material_swaps: &mut HashMap<u32, crate::esm::records::MaterialSwapRecord>,
    depth: u32,
) -> Result<()> {
    while reader.position() < end && reader.remaining() > 0 {
        if reader.is_group() {
            let sub = reader.read_group_header()?;
            let Some(sub_end) = reader.bounded_group_content_end(&sub, depth, "parse_mswp_group")
            else {
                continue;
            };
            parse_mswp_group_inner(reader, sub_end, material_swaps, depth + 1)?;
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

#[cfg(test)]
mod ligh_dat2_tests {
    use super::build_static_object_from_subs;
    use crate::esm::reader::SubRecord;

    fn sub(sig: &[u8; 4], data: Vec<u8>) -> SubRecord {
        SubRecord {
            sub_type: *sig,
            data,
        }
    }

    /// Build a 76-byte Starfield LIGH `DAT2` payload per the verified
    /// xEdit `wbDefinitionsSF1.pas` layout, with caller-supplied
    /// radius / color / flicker so the test pins the exact field offsets.
    fn dat2_bytes(radius: f32, rgb: [u8; 3], flags: u16) -> Vec<u8> {
        let mut d = vec![0u8; 76];
        // { 0} Time (i32) — left zero.
        d[4..8].copy_from_slice(&radius.to_le_bytes()); // { 4} Float Radius
        d[8] = rgb[0]; // { 8} ByteColors Color: R
        d[9] = rgb[1]; //          G
        d[10] = rgb[2]; //         B
        d[11] = 255; //  A (unused for color)
        d[12..14].copy_from_slice(&flags.to_le_bytes()); // {12} U16 Flags
                                                         // {14} Unused(2)
        d[16..20].copy_from_slice(&2.0f32.to_le_bytes()); // {16} Falloff Exponent
        d[20..24].copy_from_slice(&90.0f32.to_le_bytes()); // {20} FOV
        d[28..32].copy_from_slice(&1.5f32.to_le_bytes()); // {28} Flicker Period
        d[32..36].copy_from_slice(&0.25f32.to_le_bytes()); // {32} Intensity Amplitude
        d[36..40].copy_from_slice(&0.5f32.to_le_bytes()); // {36} Movement Amplitude
        d
    }

    /// #1567 regression: a Starfield LIGH carrying a `DAT2` light chunk but
    /// NO `MODL` and NO `DATA` must decode to a `StaticObject` with
    /// `light_data` (color + radius from the verified offsets) — pre-fix it
    /// returned `None` and the form was never indexed, dropping 656 Cydonia
    /// lights.
    #[test]
    fn starfield_ligh_dat2_decodes_to_light_data() {
        // Cydonia LIGH skeleton from the audit dump (000027BB), trimmed to
        // the fields that matter: EDID + the top-level DAT2. No MODL/DATA.
        let subs = vec![
            sub(b"EDID", b"TestSconce\0".to_vec()),
            sub(b"DAT2", dat2_bytes(512.0, [200, 150, 100], 0x0010)),
        ];

        let obj = build_static_object_from_subs(0x000027BB, b"LIGH", false, &subs, &None)
            .expect("Starfield LIGH with DAT2 must produce a StaticObject");

        let ld = obj.light_data.expect("DAT2 must yield light_data");
        assert_eq!(ld.radius, 512.0, "radius is a Float at DAT2 offset 4");
        assert!((ld.color[0] - 200.0 / 255.0).abs() < 1e-6, "R at offset 8");
        assert!((ld.color[1] - 150.0 / 255.0).abs() < 1e-6, "G at offset 9");
        assert!((ld.color[2] - 100.0 / 255.0).abs() < 1e-6, "B at offset 10");
        assert_eq!(ld.flags, 0x0010, "Flags is a U16 at offset 12");
        assert_eq!(ld.falloff_exponent, 2.0, "Falloff Exponent at offset 16");
        assert_eq!(
            ld.fov_degrees, 90.0,
            "FOV at DAT2 offset 20 (#2439 / NIFAL-D2-01)"
        );
        assert_eq!(ld.period_secs, 1.5, "Flicker Period at offset 28");
        assert_eq!(
            ld.intensity_amplitude, 0.25,
            "Intensity Amplitude at offset 32"
        );
        assert_eq!(
            ld.movement_amplitude, 0.5,
            "Movement Amplitude at offset 36"
        );
        assert!(obj.model_path.is_empty(), "Starfield LIGH carries no MODL");
    }

    /// The Skyrim/FO4 `DATA`-layout LIGH path is untouched: a record with a
    /// `DATA` chunk (radius as u32, not the Starfield float) still decodes,
    /// proving the new arm is additive and gated on the `DAT2` signature.
    #[test]
    fn skyrim_ligh_data_path_still_decodes() {
        // Skyrim DATA: { 4} u32 Radius=300, { 8..11} RGB, {12} u32 flags.
        let mut data = vec![0u8; 32];
        data[4..8].copy_from_slice(&300u32.to_le_bytes());
        data[8] = 255;
        data[9] = 200;
        data[10] = 128;
        let subs = vec![sub(b"DATA", data)];
        let obj = build_static_object_from_subs(0x1, b"LIGH", false, &subs, &None)
            .expect("Skyrim LIGH DATA must still produce a StaticObject");
        let ld = obj.light_data.expect("DATA must yield light_data");
        assert_eq!(ld.radius, 300.0, "Skyrim radius is a u32 cast to f32");
    }

    /// #2478 / REN-D22-03 regression: a pre-Skyrim (Oblivion/FO3/FNV)
    /// 32-byte `DATA` layout has Value at bytes 24-27 and Weight at
    /// 28-31 — NOT near-clip/flicker-period. Pre-fix, `len >= 32` alone
    /// gated the flicker-period read at offset 28, so a light with a
    /// nonzero Weight (a completely ordinary, expected value — every
    /// real LIGH has one) decoded that Weight as `period_secs`. Pin
    /// that a 32-byte record decodes NO flicker parameters at all,
    /// regardless of what garbage-if-misread values sit at those bytes.
    #[test]
    fn pre_skyrim_32_byte_data_never_reads_value_weight_as_flicker() {
        let mut data = vec![0u8; 32];
        data[4..8].copy_from_slice(&300u32.to_le_bytes()); // radius
        data[8] = 255;
        data[9] = 200;
        data[10] = 128;
        data[16..20].copy_from_slice(&2.0f32.to_le_bytes()); // falloff exponent
                                                             // Bytes 24-27: Value (u32) — an ordinary nonzero game-gold value.
        data[24..28].copy_from_slice(&50u32.to_le_bytes());
        // Bytes 28-31: Weight (f32) — an ordinary nonzero encumbrance
        // value. Pre-fix this was misread as `period_secs`.
        data[28..32].copy_from_slice(&1.5f32.to_le_bytes());
        let subs = vec![sub(b"DATA", data)];
        let obj = build_static_object_from_subs(0x3, b"LIGH", false, &subs, &None)
            .expect("pre-Skyrim LIGH DATA must still produce a StaticObject");
        let ld = obj.light_data.expect("DATA must yield light_data");
        assert_eq!(ld.falloff_exponent, 2.0, "falloff exponent is still read");
        assert_eq!(
            ld.period_secs, 0.0,
            "32-byte pre-Skyrim DATA authors no flicker period — must not \
             read the record's Weight field as one"
        );
        assert_eq!(
            ld.intensity_amplitude, 0.0,
            "32-byte pre-Skyrim DATA authors no intensity amplitude"
        );
        assert_eq!(
            ld.movement_amplitude, 0.0,
            "32-byte pre-Skyrim DATA authors no movement amplitude"
        );
    }

    /// Sibling of the above: the full 48-byte Skyrim+ `DATA` layout must
    /// still decode its flicker period / intensity / movement amplitude
    /// correctly — the #2478 fix narrows the gate, it must not zero out
    /// the legitimate Skyrim+ path.
    #[test]
    fn skyrim_plus_48_byte_data_still_decodes_flicker_parameters() {
        let mut data = vec![0u8; 48];
        data[4..8].copy_from_slice(&300u32.to_le_bytes()); // radius
        data[16..20].copy_from_slice(&1.0f32.to_le_bytes()); // falloff exponent
        data[24..28].copy_from_slice(&512.0f32.to_le_bytes()); // near clip
        data[28..32].copy_from_slice(&2.0f32.to_le_bytes()); // flicker period
        data[32..36].copy_from_slice(&0.25f32.to_le_bytes()); // intensity amplitude
        data[36..40].copy_from_slice(&0.1f32.to_le_bytes()); // movement amplitude
        let subs = vec![sub(b"DATA", data)];
        let obj = build_static_object_from_subs(0x4, b"LIGH", false, &subs, &None)
            .expect("Skyrim+ LIGH DATA must produce a StaticObject");
        let ld = obj.light_data.expect("DATA must yield light_data");
        assert_eq!(ld.period_secs, 2.0, "flicker period at offset 28");
        assert_eq!(
            ld.intensity_amplitude, 0.25,
            "intensity amplitude at offset 32"
        );
        assert_eq!(
            ld.movement_amplitude, 0.1,
            "movement amplitude at offset 36"
        );
    }

    /// #2439 / NIFAL-D2-01 regression: FOV (bytes 20-23) sits in the
    /// prefix SHARED by the pre-Skyrim 32-byte and Skyrim+ 48-byte `DATA`
    /// layouts (unlike period/amplitude, which are Skyrim+-only) — must
    /// decode at both lengths, not just the 48-byte one.
    #[test]
    fn fov_decodes_from_data_at_both_pre_skyrim_and_skyrim_plus_lengths() {
        let mut pre_skyrim = vec![0u8; 32];
        pre_skyrim[4..8].copy_from_slice(&300u32.to_le_bytes());
        pre_skyrim[20..24].copy_from_slice(&35.0f32.to_le_bytes());
        let obj =
            build_static_object_from_subs(0x5, b"LIGH", false, &[sub(b"DATA", pre_skyrim)], &None)
                .expect("pre-Skyrim LIGH DATA must produce a StaticObject");
        assert_eq!(
            obj.light_data.expect("must yield light_data").fov_degrees,
            35.0,
            "FOV at DATA offset 20, 32-byte pre-Skyrim layout"
        );

        let mut skyrim_plus = vec![0u8; 48];
        skyrim_plus[4..8].copy_from_slice(&300u32.to_le_bytes());
        skyrim_plus[20..24].copy_from_slice(&35.0f32.to_le_bytes());
        let obj =
            build_static_object_from_subs(0x6, b"LIGH", false, &[sub(b"DATA", skyrim_plus)], &None)
                .expect("Skyrim+ LIGH DATA must produce a StaticObject");
        assert_eq!(
            obj.light_data.expect("must yield light_data").fov_degrees,
            35.0,
            "FOV at DATA offset 20, 48-byte Skyrim+ layout"
        );
    }

    /// A non-LIGH record carrying a `DAT2` (e.g. FO3/FNV AMMO weapon data)
    /// must NOT be misread as light data — the arm is `is_ligh`-gated.
    #[test]
    fn non_ligh_dat2_is_not_treated_as_light() {
        let subs = vec![sub(b"DAT2", dat2_bytes(512.0, [200, 150, 100], 0))];
        // AMMO with only a DAT2 and no MODL → no light_data, no model → None.
        let obj = build_static_object_from_subs(0x2, b"AMMO", false, &subs, &None);
        assert!(
            obj.is_none() || obj.unwrap().light_data.is_none(),
            "DAT2 on a non-LIGH record must not synthesize light_data"
        );
    }
}

#[cfg(test)]
mod starfield_armo_modl_tests {
    use super::build_static_object_from_subs;
    use crate::esm::reader::SubRecord;

    #[test]
    fn fixed_width_modl_is_not_reported_as_corrupt_mesh_path() {
        let subs = vec![
            SubRecord {
                sub_type: *b"EDID",
                data: b"StarfieldArmor\0".to_vec(),
            },
            SubRecord {
                sub_type: *b"MODL",
                data: 0x0012_3456u32.to_le_bytes().to_vec(),
            },
        ];

        // ARMO's FormID is consumed by `parse_armo` and resolved through the
        // ARMA table. The cell-side extractor has no placement mesh for an
        // inventory definition, so it must simply decline the StaticObject
        // without logging the generic #1620 corrupt-path warning.
        assert!(build_static_object_from_subs(0x42, b"ARMO", false, &subs, &None).is_none());
    }
}
