//! Walker functions extracted from ../mod.rs (stage B refactor).
//!
//! Functions: parse_cell_group, parse_refr_group, parse_land_record.

use super::helpers::{read_form_id, read_form_id_array, read_zstring};
use super::*;

/// Walk the CELL group hierarchy to find interior cells and their placed references.
pub(super) fn parse_cell_group(
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
                let mut is_interior = false;
                let mut lighting = None;
                let mut water_height: Option<f32> = None;
                let mut image_space_form: Option<u32> = None;
                let mut water_type_form: Option<u32> = None;
                let mut acoustic_space_form: Option<u32> = None;
                let mut music_type_form: Option<u32> = None;
                let mut location_form: Option<u32> = None;
                let mut regions: Vec<u32> = Vec::new();
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
                        b"DATA" if sub.data.len() >= 1 => is_interior = sub.data[0] & 1 != 0,
                        b"XCLW" if sub.data.len() >= 4 => {
                            // XCLW: f32 water plane height in world units
                            // (Z-up). Same layout across Oblivion / FO3 / FNV
                            // / Skyrim — the cell's water surface sits at
                            // this Z (interior) or Z-in-worldspace (exterior).
                            // See #397 / #356.
                            water_height = Some(f32::from_le_bytes([
                                sub.data[0],
                                sub.data[1],
                                sub.data[2],
                                sub.data[3],
                            ]));
                        }
                        // Skyrim extended CELL sub-records (#356). Each is
                        // a 4-byte FormID; the walker previously dropped
                        // them on the `_` arm so the renderer / audio /
                        // quest system had no per-cell context.
                        b"XCIM" => image_space_form = read_form_id(&sub.data),
                        b"XCWT" => water_type_form = read_form_id(&sub.data),
                        b"XCAS" => acoustic_space_form = read_form_id(&sub.data),
                        b"XCMO" => music_type_form = read_form_id(&sub.data),
                        b"XLCN" => location_form = read_form_id(&sub.data),
                        // XCLR is a packed FormID array — region tags
                        // referenced by REGN records. Variable length;
                        // empty list is normal.
                        b"XCLR" => regions = read_form_id_array(&sub.data),
                        // #692 — XOWN owner, XRNK faction-rank gate,
                        // XGLB global-variable FormID. Same shape on
                        // CELL + REFR. Cross-game (Oblivion / FO3 /
                        // FNV / Skyrim+).
                        b"XOWN" if sub.data.len() >= 4 => {
                            ownership_owner = read_form_id(&sub.data);
                        }
                        b"XRNK" if sub.data.len() >= 4 => {
                            ownership_rank = Some(i32::from_le_bytes([
                                sub.data[0],
                                sub.data[1],
                                sub.data[2],
                                sub.data[3],
                            ]));
                        }
                        b"XGLB" if sub.data.len() >= 4 => {
                            ownership_global = read_form_id(&sub.data);
                        }
                        b"XCLL" if sub.data.len() >= 28 => {
                            // XCLL layout (shared prefix for all games):
                            //   0-3:   Ambient RGBA (byte 0=R, 1=G, 2=B, 3=unused)
                            //   4-7:   Directional RGBA
                            //   8-11:  Fog color near RGBA
                            //   12-15: Fog near (f32)
                            //   16-19: Fog far (f32)
                            //   20-23: Directional rotation X (i32, degrees)
                            //   24-27: Directional rotation Y (i32, degrees)
                            //
                            // Oblivion: 36 bytes. FNV: 40 bytes.
                            // Skyrim+: 92 bytes with extended fields.
                            // Detect by length — unambiguous.
                            //
                            // Byte order is RGB (not D3DCOLOR/BGR): xEdit
                            // defines XCLL color fields as (Red, Green, Blue,
                            // Unknown). This matches the LIGH DATA byte order
                            // after the #389 revert. A dim cool-blue ambient
                            // on a saloon interior is correct — the warm look
                            // comes from the placed LIGH oil-lanterns, not
                            // from the cell fill.
                            let d = &sub.data;
                            let ambient = [
                                d[0] as f32 / 255.0,
                                d[1] as f32 / 255.0,
                                d[2] as f32 / 255.0,
                            ];
                            let directional_color = [
                                d[4] as f32 / 255.0,
                                d[5] as f32 / 255.0,
                                d[6] as f32 / 255.0,
                            ];
                            let rot_x = {
                                let raw = i32::from_le_bytes([d[20], d[21], d[22], d[23]]);
                                (raw as f32).to_radians()
                            };
                            let rot_y = {
                                let raw = i32::from_le_bytes([d[24], d[25], d[26], d[27]]);
                                (raw as f32).to_radians()
                            };
                            // Fog fields land in bytes 8..20, always
                            // present under the outer `>= 28` gate. Pre-#483
                            // a redundant `if d.len() >= 20` nested here
                            // with a dead-code `else` branch that couldn't
                            // be reached (the outer gate already proves
                            // `len >= 28 > 20`). See FNV-2-L3.
                            // RGB byte order (bytes 8=R, 9=G, 10=B).
                            let fog_color = [
                                d[8] as f32 / 255.0,
                                d[9] as f32 / 255.0,
                                d[10] as f32 / 255.0,
                            ];
                            let fog_near = f32::from_le_bytes([d[12], d[13], d[14], d[15]]);
                            let fog_far = f32::from_le_bytes([d[16], d[17], d[18], d[19]]);

                            // XCLL extended layout — per UESP + Gamebryo 2.3
                            // `NiDirectionalLight` source + nif.xml:
                            //   28-31: Directional fade (f32)      — FNV + Skyrim
                            //   32-35: Fog clip distance (f32)     — FNV + Skyrim
                            //   36-39: Fog power (f32)             — FNV + Skyrim
                            //   40-63: Directional ambient 6×RGBA  — Skyrim only
                            //   64-67: Specular color RGBA         — Skyrim only
                            //   68-71: Fresnel power (f32)         — Skyrim only
                            //   72-75: Fog far color RGBA          — Skyrim only
                            //   76-79: Fog max (f32)               — Skyrim only
                            //   80-83: Light fade begin (f32)      — Skyrim only
                            //   84-87: Light fade end (f32)        — Skyrim only
                            //   88-91: Inherits flags (u32, unused)
                            //
                            // FNV XCLL is 40 bytes — it carries `dir_fade`,
                            // `fog_clip`, `fog_power` in the 28..40 tail. Pre-
                            // #379 the whole block 28-87 was gated on `>= 92`
                            // so FNV cells reported all three as `None` and
                            // the renderer fell back to a flat 0.6× fill.
                            // Two separate gates now mirror the on-disk shape.
                            let (dir_fade, fog_clip, fog_power) = if d.len() >= 40 {
                                (
                                    Some(f32::from_le_bytes([d[28], d[29], d[30], d[31]])),
                                    Some(f32::from_le_bytes([d[32], d[33], d[34], d[35]])),
                                    Some(f32::from_le_bytes([d[36], d[37], d[38], d[39]])),
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
                            ) = if d.len() >= 92 {
                                // Unpack the 6 × RGBA ambient cube (#367). RGB
                                // byte order (o=R, o+1=G, o+2=B); the alpha/
                                // padding byte at o+3 is ignored.
                                let mut ambient_cube = [[0.0f32; 3]; 6];
                                for (face, out) in ambient_cube.iter_mut().enumerate() {
                                    let o = 40 + face * 4;
                                    *out = [
                                        d[o] as f32 / 255.0,
                                        d[o + 1] as f32 / 255.0,
                                        d[o + 2] as f32 / 255.0,
                                    ];
                                }
                                (
                                    Some(ambient_cube),
                                    Some([
                                        d[64] as f32 / 255.0,
                                        d[65] as f32 / 255.0,
                                        d[66] as f32 / 255.0,
                                    ]),
                                    Some(d[67] as f32 / 255.0),
                                    Some(f32::from_le_bytes([d[68], d[69], d[70], d[71]])),
                                    Some([
                                        d[72] as f32 / 255.0,
                                        d[73] as f32 / 255.0,
                                        d[74] as f32 / 255.0,
                                    ]),
                                    Some(f32::from_le_bytes([d[76], d[77], d[78], d[79]])),
                                    Some(f32::from_le_bytes([d[80], d[81], d[82], d[83]])),
                                    Some(f32::from_le_bytes([d[84], d[85], d[86], d[87]])),
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
                            location_form,
                            regions: regions.clone(),
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
pub(super) fn parse_refr_group(
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
            // #692 — per-REFR ownership override. Scopes ownership to a
            // single placed object (chest, bed, locker) when the parent
            // cell is public. Same XOWN / XRNK / XGLB layout as CELL.
            let mut ownership_owner: Option<u32> = None;
            let mut ownership_rank: Option<i32> = None;
            let mut ownership_global: Option<u32> = None;

            // Helpers to decode LE values from sub-record slices. Kept
            // local so the REFR match arm doesn't sprout u32 / f32
            // shuffling at each new sub-type.
            let read_u32 = |bytes: &[u8], off: usize| -> u32 {
                u32::from_le_bytes([
                    bytes[off],
                    bytes[off + 1],
                    bytes[off + 2],
                    bytes[off + 3],
                ])
            };
            let read_f32 = |bytes: &[u8], off: usize| -> f32 {
                f32::from_le_bytes([
                    bytes[off],
                    bytes[off + 1],
                    bytes[off + 2],
                    bytes[off + 3],
                ])
            };

            for sub in &subs {
                match &sub.sub_type {
                    b"NAME" if sub.data.len() >= 4 => {
                        base_form_id = read_u32(&sub.data, 0);
                    }
                    b"DATA" if sub.data.len() >= 24 => {
                        // 6 floats: posX, posY, posZ, rotX, rotY, rotZ
                        for i in 0..3 {
                            position[i] = read_f32(&sub.data, i * 4);
                        }
                        for i in 0..3 {
                            rotation[i] = read_f32(&sub.data, 12 + i * 4);
                        }
                    }
                    b"XSCL" if sub.data.len() >= 4 => {
                        scale = read_f32(&sub.data, 0);
                    }
                    // XESP — enable-parent gating (Skyrim+). 4-byte
                    // parent FormID + 1-byte flags; bit 0 = inverted.
                    // Pre-#349 every default-disabled "spawn after
                    // quest stage" REFR rendered immediately on cell
                    // load; the cell loader now skips these via
                    // `enable_parent.default_disabled()`.
                    b"XESP" if sub.data.len() >= 5 => {
                        let form_id = read_u32(&sub.data, 0);
                        let inverted = sub.data[4] & 1 != 0;
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
                        let destination = read_u32(&sub.data, 0);
                        let dest_pos = [
                            read_f32(&sub.data, 4),
                            read_f32(&sub.data, 8),
                            read_f32(&sub.data, 12),
                        ];
                        let dest_rot = [
                            read_f32(&sub.data, 16),
                            read_f32(&sub.data, 20),
                            read_f32(&sub.data, 24),
                        ];
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
                        let bounds = [
                            read_f32(&sub.data, 0),
                            read_f32(&sub.data, 4),
                            read_f32(&sub.data, 8),
                        ];
                        let color = [
                            read_f32(&sub.data, 12),
                            read_f32(&sub.data, 16),
                            read_f32(&sub.data, 20),
                        ];
                        let unknown = read_f32(&sub.data, 24);
                        let shape_type = read_u32(&sub.data, 28);
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
                        let keyword = read_u32(&sub.data, 0);
                        let target = read_u32(&sub.data, 4);
                        linked_refs.push(LinkedRef { keyword, target });
                    }
                    // XRMR — Room membership. Layout per UESP:
                    // count(u32) + count × room_ref(u32). Pre-#412 FO4
                    // cell-subdivided interior culling couldn't work.
                    // Guard the count against the remaining bytes so a
                    // corrupt record can't over-read.
                    b"XRMR" if sub.data.len() >= 4 => {
                        let count = read_u32(&sub.data, 0) as usize;
                        let max = (sub.data.len() - 4) / 4;
                        let n = count.min(max);
                        rooms.reserve(n);
                        for i in 0..n {
                            rooms.push(read_u32(&sub.data, 4 + i * 4));
                        }
                    }
                    // XPOD — Portal origin/destination pair. 8 bytes.
                    // Multiple XPOD sub-records allowed (one per portal
                    // connected to this REFR).
                    b"XPOD" if sub.data.len() >= 8 => {
                        let origin = read_u32(&sub.data, 0);
                        let destination = read_u32(&sub.data, 4);
                        portals.push(PortalLink { origin, destination });
                    }
                    // XRDS — Light radius override. Single f32.
                    b"XRDS" if sub.data.len() >= 4 => {
                        radius_override = Some(read_f32(&sub.data, 0));
                    }
                    // XATO — alternate TXST form ID. 4 bytes. When this
                    // REFR places a base mesh whose textures should be
                    // swapped for another TXST (the 140 MNAM-only vanilla
                    // FO4 TXSTs land here), the cell loader resolves
                    // this against `EsmCellIndex.texture_sets` and
                    // overlays the 8 slot paths + MNAM onto the spawned
                    // mesh. See audit FO4-DIM6-02 / #584.
                    b"XATO" if sub.data.len() >= 4 => {
                        alt_texture_ref = Some(read_u32(&sub.data, 0));
                    }
                    // XTNM — landscape TXST override form ID. Same
                    // 4-byte layout as XATO but scopes the override to
                    // LAND references (LTEX default swap). Kept for
                    // completeness; the LAND-side consumer path is
                    // separate from the mesh path wired for XATO. #584.
                    b"XTNM" if sub.data.len() >= 4 => {
                        land_texture_ref = Some(read_u32(&sub.data, 0));
                    }
                    // XTXR — per-slot texture swap. 8 bytes: TXST
                    // FormID(u32) + slot_index(u32). Lets a REFR
                    // override a single slot of the host mesh's texture
                    // set without replacing the whole TXST. Multiple
                    // XTXR sub-records allowed per REFR; we collect
                    // them in authoring order. #584.
                    b"XTXR" if sub.data.len() >= 8 => {
                        let texture_set = read_u32(&sub.data, 0);
                        let slot_index = read_u32(&sub.data, 4);
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
                    b"XEMI" if sub.data.len() >= 4 => {
                        emissive_light_ref = Some(read_u32(&sub.data, 0));
                    }
                    // #692 — per-REFR ownership override (mirrors the
                    // CELL walker arms). XOWN owner FormID, XRNK
                    // faction-rank gate, XGLB global-var gate. Same
                    // 4-byte layout as the CELL form. Cross-game.
                    b"XOWN" if sub.data.len() >= 4 => {
                        ownership_owner = Some(read_u32(&sub.data, 0));
                    }
                    b"XRNK" if sub.data.len() >= 4 => {
                        ownership_rank = Some(i32::from_le_bytes([
                            sub.data[0],
                            sub.data[1],
                            sub.data[2],
                            sub.data[3],
                        ]));
                    }
                    b"XGLB" if sub.data.len() >= 4 => {
                        ownership_global = Some(read_u32(&sub.data, 0));
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
pub(super) fn parse_land_record(
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
                let base_offset =
                    f32::from_le_bytes([sub.data[0], sub.data[1], sub.data[2], sub.data[3]]);
                let mut offset = base_offset * 8.0;
                for row in 0..33usize {
                    let first_delta = sub.data[4 + row * 33] as i8 as f32 * 8.0;
                    offset += first_delta;
                    heights[row * 33] = offset;
                    let mut col_accum = offset;
                    for col in 1..33usize {
                        let d = sub.data[4 + row * 33 + col] as i8 as f32 * 8.0;
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
                let ltex_id =
                    u32::from_le_bytes([sub.data[0], sub.data[1], sub.data[2], sub.data[3]]);
                let quadrant = sub.data[4] as usize;
                if quadrant < 4 {
                    quadrants[quadrant].base = Some(ltex_id);
                }
            }
            b"ATXT" if sub.data.len() >= 8 => {
                // Additional texture header: formid(4) + quadrant(1) + unused(1) + layer(2).
                let ltex_id =
                    u32::from_le_bytes([sub.data[0], sub.data[1], sub.data[2], sub.data[3]]);
                let quadrant = sub.data[4] as usize;
                let layer = u16::from_le_bytes([sub.data[6], sub.data[7]]);
                if quadrant < 4 {
                    pending_atxt = Some((quadrant, ltex_id, layer));
                }
            }
            b"VTXT" => {
                // Alpha layer data for the preceding ATXT.
                // Array of 8-byte entries: position(u16) + unused(u16) + opacity(f32).
                if let Some((quadrant, ltex_id, layer)) = pending_atxt.take() {
                    let mut alpha = vec![0.0f32; 17 * 17];
                    let entry_count = sub.data.len() / 8;
                    for i in 0..entry_count {
                        let off = i * 8;
                        if off + 8 > sub.data.len() {
                            break;
                        }
                        let pos = u16::from_le_bytes([sub.data[off], sub.data[off + 1]]) as usize;
                        let opacity = f32::from_le_bytes([
                            sub.data[off + 4],
                            sub.data[off + 5],
                            sub.data[off + 6],
                            sub.data[off + 7],
                        ]);
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

