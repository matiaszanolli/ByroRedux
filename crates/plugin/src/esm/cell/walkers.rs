//! Walker functions extracted from ../mod.rs (stage B refactor).
//!
//! Functions: parse_cell_group, parse_refr_group, parse_land_record.

use super::helpers::{read_form_id, read_form_id_array, read_zstring};
use super::*;
use crate::esm::records::common::read_lstring_or_zstring;
use crate::esm::sub_reader::SubReader;

/// Walk the CELL group hierarchy to find interior cells and their placed references.
pub(crate) fn parse_cell_group(
    reader: &mut EsmReader,
    end: usize,
    cells: &mut HashMap<String, CellData>,
) -> Result<()> {
    // Track the last parsed interior cell so we can attach children groups to it.
    let mut current_cell: Option<(u32, String)> = None;

    while reader.position() < end && reader.remaining() > 0 {
        if reader.is_group() {
            let sub_group = reader.read_group_header()?;
            let sub_end = reader.group_content_end(&sub_group);

            match sub_group.group_type {
                // Interior cell block (2) and sub-block (3): recurse.
                2 | 3 => {
                    parse_cell_group(reader, sub_end, cells)?;
                }
                // Cell children groups (6=temporary, 8=persistent, 9=visible distant).
                6 | 8 | 9 => {
                    if let Some((_, ref editor_id)) = current_cell {
                        let key = editor_id.to_ascii_lowercase();
                        let mut refs = Vec::new();
                        let mut _land = None; // Interior cells don't have LAND records
                        parse_refr_group(reader, sub_end, &mut refs, &mut _land)?;
                        if let Some(cell) = cells.get_mut(&key) {
                            cell.references.extend(refs);
                        }
                    } else {
                        reader.skip_group(&sub_group);
                    }
                }
                _ => {
                    reader.skip_group(&sub_group);
                }
            }
        } else {
            let header = reader.read_record_header()?;
            if &header.record_type == b"CELL" {
                let subs = reader.read_sub_records(&header)?;
                let mut editor_id = String::new();
                // #624 / SK-D6-NEW-02 — display name from FULL. Routes
                // through the lstring helper so localized Skyrim plugins
                // get a `<lstring 0x…>` placeholder instead of garbage
                // 3-byte cstrings.
                let mut display_name: Option<String> = None;
                let mut is_interior = false;
                let mut lighting = None;
                let mut water_height: Option<f32> = None;
                let mut image_space_form: Option<u32> = None;
                let mut water_type_form: Option<u32> = None;
                let mut acoustic_space_form: Option<u32> = None;
                let mut music_type_form: Option<u32> = None;
                // #693 / O3-N-05 — pre-Skyrim XCMT (1-byte enum) and
                // Skyrim XCCM (4-byte CLMT FormID). Both fell to the
                // catch-all `_` arm pre-fix.
                let mut music_type_enum: Option<u8> = None;
                let mut climate_override: Option<u32> = None;
                let mut location_form: Option<u32> = None;
                let mut regions: Vec<u32> = Vec::new();
                // SK-D6-02 / #566 — LTMP lighting-template FormID. Skyrim+
                // cells that omit XCLL fall back to this LGTM reference;
                // pre-#566 the link was dropped on the catch-all `_` arm
                // and the cell rendered with the engine default ambient.
                let mut lighting_template_form: Option<u32> = None;
                // #692 — XOWN / XRNK / XGLB ownership tuple. All three
                // sub-records optional; cell ends up with `Some` only
                // when at least XOWN is present. XRNK and XGLB without
                // XOWN are nonsensical and dropped (the consumer would
                // have nothing to gate against).
                let mut ownership_owner: Option<u32> = None;
                let mut ownership_rank: Option<i32> = None;
                let mut ownership_global: Option<u32> = None;

                for sub in &subs {
                    match &sub.sub_type {
                        b"EDID" => editor_id = read_zstring(&sub.data),
                        // #624 / SK-D6-NEW-02 — Skyrim cells DO ship FULL
                        // (e.g. WhiterunBanneredMare's FULL = "The
                        // Bannered Mare"). Pre-fix this fell to the
                        // catch-all `_` arm and the display name was
                        // dropped. The lstring helper auto-routes the
                        // 4-byte STRINGS-table case for localized
                        // plugins.
                        b"FULL" => display_name = Some(read_lstring_or_zstring(&sub.data)),
                        b"DATA" if sub.data.len() >= 1 => is_interior = sub.data[0] & 1 != 0,
                        b"XCLW" => {
                            // XCLW: f32 water plane height in world units
                            // (Z-up). Same layout across Oblivion / FO3 / FNV
                            // / Skyrim — the cell's water surface sits at
                            // this Z (interior) or Z-in-worldspace (exterior).
                            // See #397 / #356.
                            water_height = SubReader::new(&sub.data).f32().ok();
                        }
                        // Skyrim extended CELL sub-records (#356). Each is
                        // a 4-byte FormID; the walker previously dropped
                        // them on the `_` arm so the renderer / audio /
                        // quest system had no per-cell context.
                        b"XCIM" => image_space_form = read_form_id(&sub.data),
                        b"XCWT" => water_type_form = read_form_id(&sub.data),
                        // LTMP — lighting-template FormID (SK-D6-02 / #566).
                        // Same shape as the other 4-byte FormID slots; the
                        // cell loader walks `EsmIndex.lighting_templates`
                        // when XCLL is absent so vanilla Skyrim interior
                        // cells (Solitude inn cluster, Dragonsreach
                        // throne room, Markarth cells) render with the
                        // template ambient instead of the engine default.
                        b"LTMP" => lighting_template_form = read_form_id(&sub.data),
                        b"XCAS" => acoustic_space_form = read_form_id(&sub.data),
                        b"XCMO" => music_type_form = read_form_id(&sub.data),
                        // #693 / O3-N-05 — XCMT pre-Skyrim music enum
                        // (Oblivion / FO3 / FNV). 1-byte payload.
                        b"XCMT" if !sub.data.is_empty() => {
                            music_type_enum = Some(sub.data[0]);
                        }
                        // #693 / O3-N-05 — XCCM Skyrim climate override
                        // (per-cell CLMT FormID, exterior cells only,
                        // but a few interior mods have been seen with
                        // it for "outside through window" effects).
                        b"XCCM" => climate_override = read_form_id(&sub.data),
                        b"XLCN" => location_form = read_form_id(&sub.data),
                        // XCLR is a packed FormID array — region tags
                        // referenced by REGN records. Variable length;
                        // empty list is normal.
                        b"XCLR" => regions = read_form_id_array(&sub.data),
                        // #692 — XOWN owner, XRNK faction-rank gate,
                        // XGLB global-variable FormID. Same shape on
                        // CELL + REFR. Cross-game (Oblivion / FO3 /
                        // FNV / Skyrim+).
                        b"XOWN" => {
                            ownership_owner = read_form_id(&sub.data);
                        }
                        b"XRNK" => {
                            ownership_rank = SubReader::new(&sub.data).i32().ok();
                        }
                        b"XGLB" => {
                            ownership_global = read_form_id(&sub.data);
                        }
                        b"XCLL" if sub.data.len() >= 28 => {
                            // XCLL layout (shared 28-byte prefix across all games):
                            //   ambient RGBA, directional RGBA, fog-near RGBA
                            //   fog_near f32, fog_far f32
                            //   directional rotation X (i32, degrees)
                            //   directional rotation Y (i32, degrees)
                            //
                            // Oblivion stops at 36 bytes (no extended tail).
                            // FNV adds 12 bytes — dir_fade / fog_clip /
                            // fog_power. Skyrim+ extends to 92 bytes with
                            // the 6×RGBA ambient cube + specular + extended
                            // fog + light fade range. Two separate gates
                            // mirror the on-disk shape (#379).
                            //
                            // Byte order in colour fields is RGB, not
                            // BGR/D3DCOLOR; the 4th byte is unused padding
                            // — xEdit defines XCLL colours as (Red, Green,
                            // Blue, Unknown). The cool-blue saloon ambient
                            // is correct; warmth comes from placed LIGH
                            // oil-lanterns, not from the cell fill.
                            // See #389 / FNV-2-L3.
                            let mut r = SubReader::new(&sub.data);
                            let ambient = r.rgb_color().unwrap_or([0.0; 3]);
                            let directional_color = r.rgb_color().unwrap_or([0.0; 3]);
                            let fog_color = r.rgb_color().unwrap_or([0.0; 3]);
                            let fog_near = r.f32_or_default();
                            let fog_far = r.f32_or_default();
                            let rot_x = (r.i32_or_default() as f32).to_radians();
                            let rot_y = (r.i32_or_default() as f32).to_radians();

                            let (dir_fade, fog_clip, fog_power) = if sub.data.len() >= 40 {
                                (
                                    Some(r.f32_or_default()),
                                    Some(r.f32_or_default()),
                                    Some(r.f32_or_default()),
                                )
                            } else {
                                (None, None, None)
                            };

                            let (
                                directional_ambient,
                                specular_color,
                                specular_alpha,
                                fresnel_power,
                                fog_far_color,
                                fog_max,
                                lf_begin,
                                lf_end,
                            ) = if sub.data.len() >= 92 {
                                // 6 × RGBA ambient cube (#367) — alpha pad
                                // discarded. Specular's 4th byte IS used as
                                // an alpha (handled below).
                                let mut ambient_cube = [[0.0f32; 3]; 6];
                                for face in &mut ambient_cube {
                                    *face = r.rgb_color().unwrap_or([0.0; 3]);
                                }
                                let spec = r.rgba_color().unwrap_or([0.0; 4]);
                                (
                                    Some(ambient_cube),
                                    Some([spec[0], spec[1], spec[2]]),
                                    Some(spec[3]),
                                    Some(r.f32_or_default()),
                                    Some(r.rgb_color().unwrap_or([0.0; 3])),
                                    Some(r.f32_or_default()),
                                    Some(r.f32_or_default()),
                                    Some(r.f32_or_default()),
                                )
                            } else {
                                (None, None, None, None, None, None, None, None)
                            };

                            lighting = Some(CellLighting {
                                ambient,
                                directional_color,
                                directional_rotation: [rot_x, rot_y],
                                fog_color,
                                fog_near,
                                fog_far,
                                directional_fade: dir_fade,
                                fog_clip,
                                fog_power,
                                fog_far_color,
                                fog_max,
                                light_fade_begin: lf_begin,
                                light_fade_end: lf_end,
                                directional_ambient,
                                specular_color,
                                specular_alpha,
                                fresnel_power,
                            });
                        }
                        _ => {}
                    }
                }

                if is_interior && !editor_id.is_empty() {
                    let key = editor_id.to_ascii_lowercase();
                    let ownership = ownership_owner.map(|owner| CellOwnership {
                        owner_form_id: owner,
                        faction_rank: ownership_rank,
                        global_var_form_id: ownership_global,
                    });
                    cells.insert(
                        key,
                        CellData {
                            form_id: header.form_id,
                            editor_id: editor_id.clone(),
                            display_name: display_name.clone(),
                            references: Vec::new(),
                            is_interior: true,
                            grid: None,
                            lighting: lighting.clone(),
                            landscape: None,
                            water_height,
                            image_space_form,
                            water_type_form,
                            acoustic_space_form,
                            music_type_form,
                            music_type_enum,
                            climate_override,
                            location_form,
                            regions: regions.clone(),
                            lighting_template_form,
                            ownership,
                        },
                    );
                    current_cell = Some((header.form_id, editor_id));
                } else {
                    current_cell = None;
                }
            } else {
                reader.skip_record(&header);
            }
        }
    }
    Ok(())
}

/// Parse REFR and LAND records within a cell children group.
pub(crate) fn parse_refr_group(
    reader: &mut EsmReader,
    end: usize,
    refs: &mut Vec<PlacedRef>,
    landscape: &mut Option<LandscapeData>,
) -> Result<()> {
    while reader.position() < end && reader.remaining() > 0 {
        if reader.is_group() {
            // Nested groups within cell children — recurse.
            let sub = reader.read_group_header()?;
            let sub_end = reader.group_content_end(&sub);
            parse_refr_group(reader, sub_end, refs, landscape)?;
            continue;
        }

        let header = reader.read_record_header()?;
        // ACRE — Oblivion-only placed-creature reference (#396). FO3+
        // folded creature placements into ACHR; ACRE's wire layout
        // matches ACHR byte-for-byte on Oblivion (NAME/DATA/XSCL/XESP),
        // so it routes through the same handler.
        if &header.record_type == b"REFR"
            || &header.record_type == b"ACHR"
            || &header.record_type == b"ACRE"
        {
            let subs = reader.read_sub_records(&header)?;
            let mut base_form_id = 0u32;
            let mut position = [0.0f32; 3];
            let mut rotation = [0.0f32; 3];
            let mut scale = 1.0f32;
            let mut enable_parent: Option<EnableParent> = None;
            let mut teleport: Option<TeleportDest> = None;
            let mut primitive: Option<PrimitiveBounds> = None;
            let mut linked_refs: Vec<LinkedRef> = Vec::new();
            let mut rooms: Vec<u32> = Vec::new();
            let mut portals: Vec<PortalLink> = Vec::new();
            let mut radius_override: Option<f32> = None;
            let mut alt_texture_ref: Option<u32> = None;
            let mut land_texture_ref: Option<u32> = None;
            let mut texture_slot_swaps: Vec<TextureSlotSwap> = Vec::new();
            let mut emissive_light_ref: Option<u32> = None;
            let mut material_swap_ref: Option<u32> = None;
            // #692 — per-REFR ownership override. Scopes ownership to a
            // single placed object (chest, bed, locker) when the parent
            // cell is public. Same XOWN / XRNK / XGLB layout as CELL.
            let mut ownership_owner: Option<u32> = None;
            let mut ownership_rank: Option<i32> = None;
            let mut ownership_global: Option<u32> = None;

            for sub in &subs {
                let mut r = SubReader::new(&sub.data);
                match &sub.sub_type {
                    b"NAME" => {
                        base_form_id = r.u32_or_default();
                    }
                    b"DATA" if sub.data.len() >= 24 => {
                        // 6 floats: posX, posY, posZ, rotX, rotY, rotZ
                        position = r.f32_array::<3>().unwrap_or([0.0; 3]);
                        rotation = r.f32_array::<3>().unwrap_or([0.0; 3]);
                    }
                    b"XSCL" => {
                        scale = r.f32().unwrap_or(1.0);
                    }
                    // XESP — enable-parent gating (Skyrim+). 4-byte
                    // parent FormID + 1-byte flags; bit 0 = inverted.
                    // Pre-#349 every default-disabled "spawn after
                    // quest stage" REFR rendered immediately on cell
                    // load; the cell loader now skips these via
                    // `enable_parent.default_disabled()`.
                    b"XESP" if sub.data.len() >= 5 => {
                        let form_id = r.u32_or_default();
                        let inverted = r.u8_or_default() & 1 != 0;
                        enable_parent = Some(EnableParent { form_id, inverted });
                    }
                    // XTEL — Teleport destination (doors). Layout per
                    // UESP: DestRef(u32) + pos(3×f32) + rot(3×f32) +
                    // optional flags(u32) = 28 or 32 bytes. We read the
                    // first 28 bytes and ignore the trailing flags so
                    // FO3/FNV (sometimes 28-byte variant) parses the
                    // same as Skyrim/FO4. Pre-#412 every interior door
                    // was dead on activation.
                    b"XTEL" if sub.data.len() >= 28 => {
                        let destination = r.u32_or_default();
                        let dest_pos = r.f32_array::<3>().unwrap_or([0.0; 3]);
                        let dest_rot = r.f32_array::<3>().unwrap_or([0.0; 3]);
                        teleport = Some(TeleportDest {
                            destination,
                            position: dest_pos,
                            rotation: dest_rot,
                        });
                    }
                    // XPRM — Primitive bounds for trigger / activator
                    // volumes. Layout per UESP: bounds(3×f32) +
                    // color(3×f32) + unknown(f32) + shape_type(u32) =
                    // 32 bytes. Pre-#412 triggers were invisible.
                    b"XPRM" if sub.data.len() >= 32 => {
                        let bounds = r.f32_array::<3>().unwrap_or([0.0; 3]);
                        let color = r.f32_array::<3>().unwrap_or([0.0; 3]);
                        let unknown = r.f32_or_default();
                        let shape_type = r.u32_or_default();
                        primitive = Some(PrimitiveBounds {
                            bounds,
                            color,
                            unknown,
                            shape_type,
                        });
                    }
                    // XLKR — Linked refs. Layout: keyword(u32) +
                    // target(u32) = 8 bytes. Multiple XLKR allowed per
                    // REFR (patrol markers, door pairs, activator
                    // targets). Pre-#412 NPCs didn't patrol and doors
                    // didn't pair.
                    b"XLKR" if sub.data.len() >= 8 => {
                        let keyword = r.u32_or_default();
                        let target = r.u32_or_default();
                        linked_refs.push(LinkedRef { keyword, target });
                    }
                    // XRMR — Room membership. Layout per UESP:
                    // count(u32) + count × room_ref(u32). Pre-#412 FO4
                    // cell-subdivided interior culling couldn't work.
                    // Guard the count against the remaining bytes so a
                    // corrupt record can't over-read.
                    b"XRMR" if sub.data.len() >= 4 => {
                        let count = r.u32_or_default() as usize;
                        let max = r.remaining() / 4;
                        let n = count.min(max);
                        rooms.reserve(n);
                        for _ in 0..n {
                            rooms.push(r.u32_or_default());
                        }
                    }
                    // XPOD — Portal origin/destination pair. 8 bytes.
                    // Multiple XPOD sub-records allowed (one per portal
                    // connected to this REFR).
                    b"XPOD" if sub.data.len() >= 8 => {
                        let origin = r.u32_or_default();
                        let destination = r.u32_or_default();
                        portals.push(PortalLink {
                            origin,
                            destination,
                        });
                    }
                    // XRDS — Light radius override. Single f32.
                    b"XRDS" => {
                        radius_override = r.f32().ok();
                    }
                    // XATO — alternate TXST form ID. 4 bytes. When this
                    // REFR places a base mesh whose textures should be
                    // swapped for another TXST (the 140 MNAM-only vanilla
                    // FO4 TXSTs land here), the cell loader resolves
                    // this against `EsmCellIndex.texture_sets` and
                    // overlays the 8 slot paths + MNAM onto the spawned
                    // mesh. See audit FO4-DIM6-02 / #584.
                    b"XATO" => {
                        alt_texture_ref = r.u32().ok();
                    }
                    // XTNM — landscape TXST override form ID. Same
                    // 4-byte layout as XATO but scopes the override to
                    // LAND references (LTEX default swap). Kept for
                    // completeness; the LAND-side consumer path is
                    // separate from the mesh path wired for XATO. #584.
                    b"XTNM" => {
                        land_texture_ref = r.u32().ok();
                    }
                    // XTXR — per-slot texture swap. 8 bytes: TXST
                    // FormID(u32) + slot_index(u32). Lets a REFR
                    // override a single slot of the host mesh's texture
                    // set without replacing the whole TXST. Multiple
                    // XTXR sub-records allowed per REFR; we collect
                    // them in authoring order. #584.
                    b"XTXR" if sub.data.len() >= 8 => {
                        let texture_set = r.u32_or_default();
                        let slot_index = r.u32_or_default();
                        texture_slot_swaps.push(TextureSlotSwap {
                            texture_set,
                            slot_index,
                        });
                    }
                    // XEMI — per-REFR emissive LIGH FormID. 4 bytes.
                    // Used by FO4 signage / floodlights to attach a
                    // placement-specific light to the base mesh. Parsed
                    // for completeness; consumer wiring (per-REFR
                    // emissive light spawn) is follow-up work. #584.
                    b"XEMI" => {
                        emissive_light_ref = r.u32().ok();
                    }
                    // XMSP — per-REFR MSWP (Material Swap) FormID. 4 bytes.
                    // Resolves against `EsmCellIndex.material_swaps` at
                    // cell-load to substitute BGSM/BGEM material paths on
                    // the base mesh. Pre-#971 the ~2,500 vanilla FO4
                    // MSWP records sat indexed but unused because the
                    // REFR arm was missing — Raider armour colour
                    // variants, settlement clutter colours, station-
                    // wagon rust, and Vault decay overlays all rendered
                    // with the base mesh's textures. See FO4-D4-NEW-08.
                    b"XMSP" => {
                        material_swap_ref = r.u32().ok();
                    }
                    // #692 — per-REFR ownership override (mirrors the
                    // CELL walker arms). XOWN owner FormID, XRNK
                    // faction-rank gate, XGLB global-var gate. Same
                    // 4-byte layout as the CELL form. Cross-game.
                    b"XOWN" => {
                        ownership_owner = r.u32().ok();
                    }
                    b"XRNK" => {
                        ownership_rank = r.i32().ok();
                    }
                    b"XGLB" => {
                        ownership_global = r.u32().ok();
                    }
                    _ => {}
                }
            }

            if base_form_id != 0 {
                let ownership = ownership_owner.map(|owner| CellOwnership {
                    owner_form_id: owner,
                    faction_rank: ownership_rank,
                    global_var_form_id: ownership_global,
                });
                refs.push(PlacedRef {
                    base_form_id,
                    position,
                    rotation,
                    scale,
                    enable_parent,
                    teleport,
                    primitive,
                    linked_refs,
                    rooms,
                    portals,
                    radius_override,
                    alt_texture_ref,
                    land_texture_ref,
                    texture_slot_swaps,
                    emissive_light_ref,
                    material_swap_ref,
                    ownership,
                });
            }
        } else if &header.record_type == b"LAND" {
            // Parse landscape heightmap, normals, and vertex colors.
            //
            // At least one vanilla FNV LAND record (form `0x00150FC0`)
            // reliably fails the body read on every ESM open — cause
            // not yet identified, single cell affected. Observable
            // symptom is a flat/untextured tile if that cell is ever
            // rendered. Demoted from `warn` to `debug` per the audit's
            // soft-fail guidance (#385 / D5-F5); the error context
            // rides through so anyone investigating sees the real
            // failure mode instead of a generic message.
            match parse_land_record(reader, &header) {
                Ok(land) => *landscape = Some(land),
                Err(e) => log::debug!(
                    "LAND record parse failed (form {:08X}): {e:#}",
                    header.form_id
                ),
            }
        } else {
            // Skip other record types (PGRE, PMIS, NAVM, etc.)
            reader.skip_record(&header);
        }
    }
    Ok(())
}

/// Decode a LAND record's VHGT, VNML, and VCLR sub-records.
///
/// VHGT encoding (from UESP): the heightmap is delta-encoded with a
/// column-then-row accumulator scheme. See the UESP wiki "Vertex Height
/// Data" section for the canonical algorithm.
pub(crate) fn parse_land_record(
    reader: &mut EsmReader,
    header: &crate::esm::reader::RecordHeader,
) -> Result<LandscapeData> {
    let subs = reader.read_sub_records(header)?;

    let mut heights = vec![0.0f32; 33 * 33];
    let mut normals: Option<Vec<u8>> = None;
    let mut vertex_colors: Option<Vec<u8>> = None;
    let mut quadrants: [TerrainQuadrant; 4] = Default::default();
    // Track the most recently parsed ATXT header so we can attach the
    // following VTXT alpha data to it. ESM sub-records are ordered:
    // ...ATXT, VTXT, ATXT, VTXT... (each ATXT is followed by its VTXT).
    let mut pending_atxt: Option<(usize, u32, u16)> = None; // (quadrant, ltex_form_id, layer)

    for sub in &subs {
        match sub.sub_type.as_slice() {
            b"VHGT" if sub.data.len() >= 1093 => {
                // UESP VHGT algorithm: delta-encoded heightmap.
                let mut r = SubReader::new(&sub.data);
                let base_offset = r.f32_or_default();
                let mut offset = base_offset * 8.0;
                for row in 0..33usize {
                    let first_delta = r.u8_or_default() as i8 as f32 * 8.0;
                    offset += first_delta;
                    heights[row * 33] = offset;
                    let mut col_accum = offset;
                    for col in 1..33usize {
                        let d = r.u8_or_default() as i8 as f32 * 8.0;
                        col_accum += d;
                        heights[row * 33 + col] = col_accum;
                    }
                }
            }
            b"VNML" if sub.data.len() >= 3267 => {
                normals = Some(sub.data[..3267].to_vec());
            }
            b"VCLR" if sub.data.len() >= 3267 => {
                vertex_colors = Some(sub.data[..3267].to_vec());
            }
            b"BTXT" if sub.data.len() >= 8 => {
                // Base texture: formid(4) + quadrant(1) + unused(3).
                let mut r = SubReader::new(&sub.data);
                let ltex_id = r.u32_or_default();
                let quadrant = r.u8_or_default() as usize;
                if quadrant < 4 {
                    quadrants[quadrant].base = Some(ltex_id);
                }
            }
            b"ATXT" if sub.data.len() >= 8 => {
                // Additional texture header: formid(4) + quadrant(1) + unused(1) + layer(u16).
                let mut r = SubReader::new(&sub.data);
                let ltex_id = r.u32_or_default();
                let quadrant = r.u8_or_default() as usize;
                let _unused = r.u8_or_default();
                let layer = r.u16_or_default();
                if quadrant < 4 {
                    pending_atxt = Some((quadrant, ltex_id, layer));
                }
            }
            b"VTXT" => {
                // Alpha layer data for the preceding ATXT.
                // Array of 8-byte entries: position(u16) + unused(u16) + opacity(f32).
                if let Some((quadrant, ltex_id, layer)) = pending_atxt.take() {
                    let mut alpha = vec![0.0f32; 17 * 17];
                    let mut r = SubReader::new(&sub.data);
                    while r.remaining() >= 8 {
                        let pos = r.u16_or_default() as usize;
                        let _unused = r.u16_or_default();
                        let opacity = r.f32_or_default();
                        if pos < 17 * 17 {
                            alpha[pos] = opacity;
                        }
                    }
                    quadrants[quadrant].layers.push(TerrainTextureLayer {
                        ltex_form_id: ltex_id,
                        layer,
                        alpha: Some(alpha),
                    });
                }
            }
            _ => {}
        }
    }

    // Sort additional layers by layer index within each quadrant.
    for q in &mut quadrants {
        q.layers.sort_by_key(|l| l.layer);
    }

    Ok(LandscapeData {
        heights,
        normals,
        vertex_colors,
        quadrants,
    })
}
