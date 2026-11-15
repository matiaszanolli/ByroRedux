use super::*;
use crate::esm::reader::{FormIdRemap, GameKind};
use crate::esm::records::test_support::sub;

#[test]
fn parse_navi_extracts_version() {
    let subs = vec![
        sub(b"EDID", b"NavMaster\0"),
        sub(b"NVER", 11u32.to_le_bytes()),
    ];
    let n = parse_navi(0x5678, &subs, &None);
    assert_eq!(n.editor_id, "NavMaster");
    assert_eq!(n.version, 11);
}

/// XCT-ROUTE-003 — `NVCI` door links: navmesh, then three counted FormID
/// arrays, the third the doors; a row that does not reconcile is skipped.
#[test]
fn parse_navi_collects_nvci_door_links() {
    let row = |standard: &[u32], doors: &[u32], trailing: &[u8]| {
        let mut data = 0x1111u32.to_le_bytes().to_vec();
        for list in [standard, &[], doors] {
            data.extend((list.len() as u32).to_le_bytes());
            list.iter().for_each(|form| data.extend(form.to_le_bytes()));
        }
        data.extend_from_slice(trailing);
        data
    };
    let subs = vec![
        sub(b"NVCI", row(&[0x2222], &[0x0010_0001, 0x0010_0002], &[])),
        sub(b"NVCI", row(&[], &[0x0010_0002, 0x0010_0003], &[])),
        sub(b"NVCI", row(&[], &[0x0010_0009], &[0xAB])),
    ];
    let n = parse_navi(0x5678, &subs, &None);
    assert_eq!(n.door_links, vec![0x0010_0001, 0x0010_0002, 0x0010_0003], "deduplicated; the trailing-byte row is skipped");
}

#[test]
fn parse_navm_extracts_version() {
    let subs = vec![sub(b"NVER", 11u32.to_le_bytes())];
    let n = parse_navm(0xAABB, &subs, &None);
    assert_eq!(n.form_id, 0xAABB);
    assert_eq!(n.version, 11);
}

#[test]
fn parse_regn_picks_worldspace_and_color() {
    let subs = vec![
        sub(b"EDID", b"WastelandRegion\0"),
        sub(b"WNAM", 0x0001_B000u32.to_le_bytes()),
        sub(b"RCLR", [128, 96, 64, 0]),
    ];
    let r = parse_regn(0xBEEF, &subs, &None);
    assert_eq!(r.editor_id, "WastelandRegion");
    // #5421 — WNAM is the owning WRLD (was mis-documented `weather_form`).
    assert_eq!(r.worldspace_form, Some(0x0001_B000));
    assert_eq!(r.color, Some([128, 96, 64]));
}

#[test]
fn parse_eczn_picks_owner_rank_flags() {
    let mut data = Vec::new();
    data.extend_from_slice(&0x0001_CAFEu32.to_le_bytes()); // owner form
    data.push(3); // rank
    data.push(15); // min level
    data.push(0x02); // flags
    let subs = vec![sub(b"EDID", b"NcrZone\0"), sub(b"DATA", &data)];
    let z = parse_eczn(0x9876, &subs, GameKind::Fallout3NV, &None);
    assert_eq!(z.editor_id, "NcrZone");
    assert_eq!(z.owner_form, 0x0001_CAFE);
    assert_eq!(z.rank, 3);
    assert_eq!(z.min_level, 15);
    assert_eq!(z.flags, 0x02);
}

/// Skyrim / FO4 / Starfield carry a location after the owner and a max
/// level after the flags (xEdit ECZN, form version >= 34); reading the
/// FO3/FNV layout there took the location's low bytes for rank / min level.
#[test]
fn parse_eczn_creation_layout_has_location_and_max_level() {
    let mut data = Vec::new();
    data.extend_from_slice(&0x0001_CAFEu32.to_le_bytes()); // owner
    data.extend_from_slice(&0x0002_BEEFu32.to_le_bytes()); // location
    data.push(1); // rank
    data.push(6); // min level
    data.push(0x03); // never resets + match PC below minimum
    data.push(20); // max level
    let subs = vec![sub(b"DATA", &data)];
    let z = parse_eczn(0x10, &subs, GameKind::Skyrim, &None);
    assert_eq!(z.owner_form, 0x0001_CAFE);
    assert_eq!(z.location_form, 0x0002_BEEF);
    assert_eq!((z.rank, z.min_level, z.max_level), (1, 6, 20));
    assert!(z.match_pc_below_minimum());
    // The 8-byte pre-34 form stops after the location.
    let short = vec![sub(b"DATA", &data[..8])];
    let z = parse_eczn(0x11, &short, GameKind::Skyrim, &None);
    assert_eq!((z.min_level, z.max_level, z.flags), (0, 0, 0));
}

/// #3715 — `parse_eczn` never took a remap at all; `owner_form` is
/// looked up against faction/actor records, which are global-keyed.
#[test]
fn parse_eczn_owner_form_is_remapped() {
    let remap = Some(FormIdRemap::regular(2, vec![0]));
    let mut data = Vec::new();
    data.extend_from_slice(&0x0100_5555u32.to_le_bytes()); // self-reference
    data.push(1); // rank
    data.push(5); // min level
    data.push(0); // flags
    let subs = vec![sub(b"DATA", &data)];
    let z = parse_eczn(0xABCD, &subs, GameKind::Fallout3NV, &remap);
    assert_eq!(z.owner_form, 0x0200_5555);
}

#[test]
fn parse_lgtm_decodes_xcll_prefix() {
    // Use distinct byte patterns so an off-by-one on any field
    // surfaces as a visible assertion failure.
    let mut data = Vec::with_capacity(92);
    data.extend_from_slice(&[80, 82, 85, 0]); // ambient
    data.extend_from_slice(&[200, 195, 180, 0]); // directional
    data.extend_from_slice(&[40, 45, 50, 0]); // fog color
    data.extend_from_slice(&64.0f32.to_le_bytes()); // fog near
    data.extend_from_slice(&4000.0f32.to_le_bytes()); // fog far
    data.extend_from_slice(&15i32.to_le_bytes()); // rot X
    data.extend_from_slice(&(-30i32).to_le_bytes()); // rot Y
    data.extend_from_slice(&0.5f32.to_le_bytes()); // dir fade
    data.extend_from_slice(&6000.0f32.to_le_bytes()); // fog clip
    data.extend_from_slice(&1.25f32.to_le_bytes()); // fog power
    data.extend_from_slice(&[0xAA; 32]); // DATA ambient bundle is unused
    data.extend_from_slice(&[90, 91, 92, 0]); // fog far color
    data.extend_from_slice(&0.75f32.to_le_bytes()); // fog max
    data.extend_from_slice(&512.0f32.to_le_bytes()); // light fade begin
    data.extend_from_slice(&1024.0f32.to_le_bytes()); // light fade end
    data.extend_from_slice(&0u32.to_le_bytes()); // unused

    let mut dalc = Vec::with_capacity(32);
    for face in 0u8..6 {
        let base = 10 + face * 10;
        dalc.extend_from_slice(&[base, base + 1, base + 2, 0]);
    }
    dalc.extend_from_slice(&[210, 211, 212, 128]);
    dalc.extend_from_slice(&3.5f32.to_le_bytes());

    let subs = vec![
        sub(b"EDID", b"LgtmInteriorDim\0"),
        sub(b"DATA", &data),
        sub(b"DALC", &dalc),
    ];
    let l = parse_lgtm(0xDEAD, &subs, GameKind::Skyrim, &None);
    assert_eq!(l.editor_id, "LgtmInteriorDim");
    assert!((l.ambient[0] - 80.0 / 255.0).abs() < 1e-6);
    assert!((l.directional[1] - 195.0 / 255.0).abs() < 1e-6);
    assert!((l.fog_color[2] - 50.0 / 255.0).abs() < 1e-6);
    assert_eq!(l.fog_near, 64.0);
    assert_eq!(l.fog_far, 4000.0);
    assert!((l.directional_rotation[0] - 15.0f32.to_radians()).abs() < 1e-6);
    assert!((l.directional_rotation[1] - (-30.0f32).to_radians()).abs() < 1e-6);
    assert_eq!(l.directional_fade, Some(0.5));
    assert_eq!(l.fog_clip, Some(6000.0));
    assert_eq!(l.fog_power, Some(1.25));
    assert_eq!(
        l.fog_far_color,
        Some([90.0 / 255.0, 91.0 / 255.0, 92.0 / 255.0])
    );
    assert_eq!(l.fog_max, Some(0.75));
    assert_eq!(l.light_fade_begin, Some(512.0));
    assert_eq!(l.light_fade_end, Some(1024.0));
    let cube = l.directional_ambient.expect("DALC ambient cube");
    assert_eq!(cube[0], [10.0 / 255.0, 11.0 / 255.0, 12.0 / 255.0]);
    assert_eq!(cube[5], [60.0 / 255.0, 61.0 / 255.0, 62.0 / 255.0]);
    assert_eq!(
        l.specular_color,
        Some([210.0 / 255.0, 211.0 / 255.0, 212.0 / 255.0])
    );
    assert_eq!(l.specular_alpha, Some(128.0 / 255.0));
    assert_eq!(l.fresnel_power, Some(3.5));
}

fn floats_le(values: &[f32]) -> Vec<u8> {
    values.iter().flat_map(|v| v.to_le_bytes()).collect()
}

/// #4416 — FO3/FNV's single `DNAM` decodes by size: 132/148 bytes
/// without Skin Dimmer, 152 bytes with it (every later field one
/// float further on).
#[test]
fn fo3_imgs_dnam_decodes_the_grade_by_size() {
    use crate::esm::reader::GameKind;
    for (size, first) in [(132usize, 24usize), (148, 24), (152, 25)] {
        let mut f = vec![0.0f32; size / 4];
        f[first] = 0.8; // saturation
        f[first + 2] = 1.4; // contrast
        f[first + 3] = 0.95; // brightness
        f[first + 4..first + 8].copy_from_slice(&[0.2, 0.4, 0.6, 0.35]);
        let subs = vec![sub(b"EDID", b"FNVInterior\0"), sub(b"DNAM", floats_le(&f))];
        let space = parse_imgs(0x10, &subs, GameKind::Fallout3NV)
            .image_space
            .unwrap_or_else(|| panic!("{size}-byte DNAM"));
        assert_eq!(
            (space.saturation, space.contrast, space.brightness),
            (0.8, 1.4, 0.95),
            "{size}"
        );
        assert_eq!(space.tint_color, [0.2, 0.4, 0.6, 0.35], "{size}");
    }
    let odd = vec![sub(b"DNAM", [0u8; 56])];
    assert!(parse_imgs(0x11, &odd, GameKind::Fallout3NV)
        .image_space
        .is_none());
}

/// #4416 — Skyrim/FO4 carry the grade in `CNAM` (saturation,
/// brightness, contrast) and `TNAM` (amount first, then RGB); their
/// `DNAM` is depth of field and must not be read as the grade. The
/// two `Default*` records carry only the legacy 56-byte `ENAM`.
#[test]
fn skyrim_imgs_reads_cnam_tnam_and_the_legacy_enam() {
    use crate::esm::reader::GameKind;
    let subs = vec![
        sub(b"EDID", b"WhiterunInterior\0"),
        sub(b"CNAM", floats_le(&[0.9, 1.1, 1.3])),
        sub(b"TNAM", floats_le(&[0.25, 1.0, 0.5, 0.0])),
        sub(b"DNAM", floats_le(&[7.0, 7.0, 7.0, 7.0])),
    ];
    let space = parse_imgs(0x20, &subs, GameKind::Skyrim)
        .image_space
        .unwrap();
    assert_eq!(
        (space.saturation, space.brightness, space.contrast),
        (0.9, 1.1, 1.3)
    );
    assert_eq!(space.tint_color, [1.0, 0.5, 0.0, 0.25]);

    let mut legacy = vec![3.0f32; 7];
    legacy.extend([0.8, 1.2, 1.4, 0.1, 0.3, 0.6, 0.9]);
    let subs = vec![sub(b"ENAM", floats_le(&legacy))];
    let space = parse_imgs(0x21, &subs, GameKind::Fallout4)
        .image_space
        .unwrap();
    assert_eq!(
        (space.saturation, space.brightness, space.contrast),
        (0.8, 1.2, 1.4)
    );
    assert_eq!(space.tint_color, [0.3, 0.6, 0.9, 0.1]);
}

/// Games without a decodable IMGS grade yield `None` — the identity.
#[test]
fn imgs_without_a_known_layout_decodes_to_none() {
    use crate::esm::reader::GameKind;
    let subs = vec![sub(b"EDID", b"LegacyImagespace\0")];
    let imgs = parse_imgs(0x000A_5678, &subs, GameKind::Skyrim);
    assert_eq!(imgs.editor_id, "LegacyImagespace");
    assert!(imgs.image_space.is_none());
    let dnam = vec![sub(b"DNAM", [0u8; 152])];
    assert!(parse_imgs(0x1, &dnam, GameKind::Oblivion)
        .image_space
        .is_none());
}

/// VLM-HF-004: the FO4 / FO76 `LGTM` `DATA` carries the same height-fog tail as the XCLL, from
/// byte 92; a Skyrim LGTM, whatever follows its body, does not.
#[test]
fn parse_lgtm_fo4_decodes_height_fog_tail() {
    let mut data = vec![0u8; 92];
    data[12..16].copy_from_slice(&64.0f32.to_le_bytes()); // fog near
    data[16..20].copy_from_slice(&4000.0f32.to_le_bytes()); // fog far
    data.extend_from_slice(&32.0f32.to_le_bytes()); // 92 near height mid
    data.extend_from_slice(&10000.0f32.to_le_bytes()); // 96 near height range
    data.extend_from_slice(&[255, 0, 0, 0]); // 100 high near colour
    data.extend_from_slice(&[0, 255, 0, 0]); // 104 high far colour
    for scale in [0.95f32, 1.0, 1.0, 1.0, 1.0] {
        data.extend_from_slice(&scale.to_le_bytes()); // 108.. high density, four scales
    }
    data.extend_from_slice(&0.0f32.to_le_bytes()); // 128 far height mid
    data.extend_from_slice(&10000.0f32.to_le_bytes()); // 132 far height range
    let subs = vec![sub(b"EDID", b"CommonwealthLgtm\0"), sub(b"DATA", &data)];
    for game in [GameKind::Fallout4, GameKind::Fallout76] {
        let l = parse_lgtm(0xF04, &subs, game, &None);
        assert_eq!(l.fog_far, 4000.0, "the shared body still decodes");
        let tail = l.height_fog.expect("a 136-byte DATA carries the tail");
        assert_eq!((tail.near_height_mid, tail.near_height_range), (32.0, 10000.0));
        assert_eq!(tail.fog_color_high_near, [1.0, 0.0, 0.0]);
        assert_eq!(tail.fog_color_high_far, [0.0, 1.0, 0.0]);
        assert_eq!(tail.high_density_scale, 0.95);
        assert_eq!(tail.far_height_range, Some(10000.0));
    }
    assert!(parse_lgtm(0xF04, &subs, GameKind::Skyrim, &None).height_fog.is_none());
    assert!(parse_lgtm(0xF04, &[sub(b"DATA", &data[..92])], GameKind::Fallout4, &None).height_fog.is_none());
}

/// VLM-HF-011: the LGTM `WGDR` names a `GDRY`, load-order remapped like every cross-reference.
#[test]
fn parse_lgtm_decodes_the_god_ray_reference() {
    let subs = vec![sub(b"EDID", b"GodRayLgtm\0"), sub(b"DATA", vec![0u8; 20]), sub(b"WGDR", 0x0000_3C5Au32.to_le_bytes())];
    let l = parse_lgtm(0xF04, &subs, GameKind::Fallout4, &None);
    assert_eq!(l.god_rays_form, Some(0x3C5A));
    assert_eq!(parse_lgtm(0xF04, &subs[..2], GameKind::Fallout4, &None).god_rays_form, None);
    // A DLC plugin authors its own forms as 0x01…; the load order keys them at slot 0x02….
    let remap = Some(FormIdRemap {
        plugin_slot: crate::esm::reader::GlobalSlot::Regular(0x02),
        master_slots: vec![crate::esm::reader::GlobalSlot::Regular(0x00)],
        scheme: Default::default(),
    });
    assert_eq!(
        parse_lgtm(0xF04, &[sub(b"WGDR", 0x0100_3C5Au32.to_le_bytes())], GameKind::Fallout4, &remap).god_rays_form,
        Some(0x0200_3C5A),
        "a reference to the DLC's own record is remapped into its load-order slot"
    );
}

#[test]
fn parse_lgtm_short_data_returns_defaults() {
    // DATA under 20 bytes → all field captures short-circuit.
    let subs = vec![sub(b"EDID", b"ShortLgtm\0"), sub(b"DATA", [1, 2, 3, 4])];
    let l = parse_lgtm(0xBEEF, &subs, GameKind::Skyrim, &None);
    assert_eq!(l.editor_id, "ShortLgtm");
    assert_eq!(l.ambient, [0.0; 3]);
    assert_eq!(l.fog_near, 0.0);
    assert!(l.directional_fade.is_none());
}

/// #5002 (SF-2026-09-29-D4-03) — Starfield's 108-byte LGTM DATA uses
/// the SF XCLL layout, not Skyrim's tail. Fixture carries
/// `ShipInteriorLT` (0x6658)'s distinguishing authored values
/// (Fog Max@44 = 0, fades@48/52 = 163 840) plus distinct bytes for
/// every height-fog field. Pre-fix, Skyrim offsets read fog_max from
/// High Density Scale's slot and the fades from the dimensionless
/// Fog Near/Far scales (later lifted ×70 by `normalize`), and the
/// height block was dropped entirely.
#[test]
fn starfield_lgtm_data_decodes_the_sf_xcll_layout() {
    let mut data = Vec::with_capacity(108);
    data.extend(vec![0u8; 12]); // ambient/directional/fog colours (12 B)
    data.extend(&floats_le(&[64.0, 4096.0])); // fog near/far
    data.extend(&0i32.to_le_bytes()); // directional azimuth slot (20)
    data.extend(&0i32.to_le_bytes()); // directional elevation slot (24)
    data.extend(&floats_le(&[1.5])); // 28: Gravity Scale (NOT dir fade)
    data.extend(&floats_le(&[8192.0, 2.0])); // 32/36: fog clip / power
    data.extend(&[90, 91, 92, 0]); // 40: fog far colour
    data.extend(&floats_le(&[0.0])); // 44: Fog Max (ShipInteriorLT authors 0)
    data.extend(&floats_le(&[163_840.0, 163_840.0])); // 48/52: light fades
    data.extend(&[1, 2, 3, 0]); // 56: unknown colour
    data.extend(&floats_le(&[120.0, 480.0])); // 60/64: near height mid/range
    data.extend(&[10, 11, 12, 0]); // 68: high near colour
    data.extend(&[13, 14, 15, 0]); // 72: high far colour
    data.extend(&floats_le(&[0.9, 1.1, 1.3, 1.4, 1.5])); // 76-95: five scales
    data.extend(&floats_le(&[640.0, 2560.0])); // 96/100: far height mid/range
    data.extend(&[2u8, 0, 0, 0]); // 104: interior type (Ship Cell) + pad
    assert_eq!(data.len(), 108);
    let subs = vec![sub(b"EDID", b"ShipInteriorLT\0"), sub(b"DATA", data)];
    let l = parse_lgtm(0x6658, &subs, GameKind::Starfield, &None);
    assert_eq!(l.editor_id, "ShipInteriorLT");
    assert_eq!(l.fog_near, 64.0);
    assert_eq!(l.fog_far, 4096.0);
    assert_eq!(
        l.directional_fade,
        None,
        "byte 28 is Gravity Scale on Starfield, not Directional Fade"
    );
    assert_eq!(l.fog_clip, Some(8192.0));
    assert_eq!(l.fog_power, Some(2.0));
    assert_eq!(
        l.fog_far_color,
        Some([90.0 / 255.0, 91.0 / 255.0, 92.0 / 255.0])
    );
    assert_eq!(
        l.fog_max, Some(0.0),
        "authored Fog Max must survive (was 1.0 default)"
    );
    assert_eq!(l.light_fade_begin, Some(163_840.0));
    assert_eq!(l.light_fade_end, Some(163_840.0));
    assert!(
        l.directional_ambient.is_none(),
        "SF LGTM carries no DALC ambient cube"
    );
    let sf = l
        .starfield
        .expect("108-byte SF DATA must decode its height-fog model");
    assert_eq!(sf.gravity_scale, 1.5);
    assert_eq!(sf.near_height_mid, 120.0);
    assert_eq!(sf.near_height_range, 480.0);
    assert_eq!(sf.high_density_scale, 0.9);
    assert_eq!(sf.fog_near_scale, 1.1);
    assert_eq!(sf.fog_far_scale, 1.3);
    assert_eq!(sf.fog_high_near_scale, 1.4);
    assert_eq!(sf.fog_high_far_scale, 1.5);
    assert_eq!(sf.far_height_mid, 640.0);
    assert_eq!(sf.far_height_range, 2560.0);
    assert_eq!(sf.interior_type, 2);
}
#[test]
fn parse_acti_extracts_scri_and_model() {
    let subs = vec![
        sub(b"EDID", b"NukaColaMachine01\0"),
        sub(b"FULL", b"Nuka-Cola Machine\0"),
        sub(b"MODL", b"activators\\nukacolamachine01.nif\0"),
        sub(b"SCRI", 0x0010_ABCDu32.to_le_bytes()),
        sub(b"SNAM", 0x0009_0000u32.to_le_bytes()),
    ];
    let a = parse_acti(0x0002_9E7A, &subs, &None);
    assert_eq!(a.editor_id, "NukaColaMachine01");
    assert_eq!(a.full_name, "Nuka-Cola Machine");
    assert_eq!(a.model_path, "activators\\nukacolamachine01.nif");
    assert_eq!(a.script_form_id, 0x0010_ABCD);
    assert_eq!(a.sound_form_id, 0x0009_0000);
    // Radio form defaults to 0 when RNAM/RADR absent.
    assert_eq!(a.radio_form_id, 0);
    // Water type defaults to 0 when WNAM is absent.
    assert_eq!(a.water_type_form, 0);
}

/// Skyrim's placed river/stream sections are activators whose `WNAM`
/// names the WATR they render and behave as. Vanilla
/// `TundraStreamStraight01WaterA` (0001608E) authors
/// `PNAM SNAM WNAM(00015429 CreekWaterFlow) FNAM`; the WNAM FormID must
/// survive parsing and go through the load-order remap like every other
/// cross-record reference.
#[test]
fn parse_acti_extracts_wnam_water_type() {
    let subs = vec![
        sub(b"EDID", b"TundraStreamStraight01WaterA\0"),
        sub(b"MODL", b"Water\\TundraStreamStraight01WaterA.nif\0"),
        sub(b"PNAM", [0xcc, 0x4c, 0x33, 0x00]),
        sub(b"SNAM", 0x0003_D0A7u32.to_le_bytes()),
        sub(b"WNAM", 0x0001_5429u32.to_le_bytes()),
        sub(b"FNAM", [0x01, 0x00]),
    ];
    let a = parse_acti(0x0001_608E, &subs, &None);
    assert_eq!(a.water_type_form, 0x0001_5429);
    assert_eq!(a.sound_form_id, 0x0003_D0A7);
}

/// Regression for #4067. `CommonNamedFields::from_subs_with_remap`
/// now remaps `SCRI` itself instead of leaving each caller to do it —
/// `parse_acti`/`parse_term` never did, so a DLC-defined scripted
/// activator or terminal resolved its script in the DLC's own
/// plugin-local space instead of global load-order space, exactly
/// like the `EsmIndex::scripts` map it must be looked up against.
/// Single-master loads (an identity remap) never exercised this —
/// hence `parse_acti_extracts_scri_and_model` above passing `&None`
/// couldn't have caught it.
#[test]
fn parse_acti_remaps_scri_to_global_space() {
    // Plugin slot 2, one master at slot 0. mod_index 1 == master_slots.len()
    // is this plugin's OWN self-reference — the case that actually moves
    // under the remap (0x00... master refs happen to land identity here,
    // which container.rs's sibling test already covers; a self-ref proves
    // the remap fired rather than merely happening to pass through).
    let remap = crate::esm::reader::FormIdRemap::regular(2, vec![0]);
    let self_ref_local: u32 = (1u32 << 24) | 0x0000_1234;
    let self_ref_global: u32 = (2u32 << 24) | 0x0000_1234;
    let subs = vec![
        sub(b"EDID", b"DlcTerminal01\0"),
        sub(b"SCRI", self_ref_local.to_le_bytes()),
    ];
    let a = parse_acti(0x0002_9E7A, &subs, &Some(remap));
    assert_eq!(
        a.script_form_id, self_ref_global,
        "ActiRecord.script_form_id must land in global space, matching \
         EsmIndex::scripts' keying — pre-#4067 this stayed at the raw \
         plugin-local id ({self_ref_local:#010X})"
    );
}

/// TD2-109 / #2068 — `parse_acti` now sources its EDID/FULL/MODL/SCRI/
/// VMAD bundle from `CommonNamedFields::from_subs`, which means each
/// field is copied across by hand into `ActiRecord`. Every other field
/// is asserted by `parse_acti_extracts_scri_and_model` above; the
/// decoded VMAD attachment was not covered by anything, so a dropped
/// `script_instance` mapping would have gone unnoticed. Activators are
/// the most common scripted base record in Skyrim cells, and the M47.2
/// attach path reads exactly this field.
#[test]
fn parse_acti_decodes_vmad_into_script_instance() {
    // version 5, objectFormat 2, 1 script "DoorScript", 0 props —
    // same shape as `common::tests::common_named_fields_decodes_vmad_script_instance`.
    let mut vmad = Vec::new();
    vmad.extend_from_slice(&5i16.to_le_bytes());
    vmad.extend_from_slice(&2i16.to_le_bytes());
    vmad.extend_from_slice(&1u16.to_le_bytes());
    let name = b"DoorScript";
    vmad.extend_from_slice(&(name.len() as u16).to_le_bytes());
    vmad.extend_from_slice(name);
    vmad.push(0); // script status
    vmad.extend_from_slice(&0u16.to_le_bytes()); // propCount = 0

    let subs = vec![
        sub(b"EDID", b"SkyrimLever01\0"),
        sub(b"MODL", b"clutter\\lever01.nif\0"),
        sub(b"VMAD", &vmad),
    ];
    let a = parse_acti(0x0001_0000, &subs, &None);
    assert_eq!(a.editor_id, "SkyrimLever01");
    assert_eq!(a.model_path, "clutter\\lever01.nif");
    let inst = a
        .script_instance
        .expect("VMAD must survive the CommonNamedFields hand-off");
    assert_eq!(inst.scripts.len(), 1);
    assert_eq!(inst.scripts[0].name, "DoorScript");
    // Skyrim activators carry VMAD *instead of* SCRI.
    assert_eq!(a.script_form_id, 0);
}

/// Regression: #4171. Fixture transcribed from `FalloutNV.esm`'s
/// `P04CompanionFireTerminal` / `…Sub` pair — the same sub-record
/// sequence and the same payload sizes the shipped record carries.
///
/// The test this replaces asserted `password == "tranquility"` from a
/// hand-written `ANAM` z-string. No FNV terminal ships an ANAM longer
/// than one byte (895 of them, all size 1), so the fixture described a
/// record that cannot exist and vouched for the parser that read it —
/// the same mutual-corroboration failure as #4077.
#[test]
fn fnv_terminal_reads_desc_body_and_itxt_rnam_menu_pairs() {
    let subs = vec![
        sub(b"EDID", b"P04CompanionFireTerminal\0"),
        sub(b"FULL", b"Companion Dismissal Terminal\0"),
        sub(b"MODL", b"clutter\\junk\\terminal01.nif\0"),
        sub(b"DESC", b"Welcome, Courier.\0"),
        // 4-byte packed field, not footer text — top corpus value.
        sub(b"DNAM", [0x00, 0x02, 0x00, 0x00]),
        sub(b"ITXT", b"Dismiss all companions.\0"),
        sub(b"RNAM", b"Accessing confirmation menu...\0"),
        sub(b"ANAM", [0x02]),
        sub(b"ITXT", b"No.\0"),
        // 246 of FNV's 895 RNAM are exactly this: four zero bytes.
        sub(b"RNAM", [0, 0, 0, 0]),
        sub(b"ANAM", [0x00]),
        sub(b"ITXT", b"Yes.\0"),
        sub(b"RNAM", b"Notification sent...\0"),
        sub(b"ANAM", [0x00]),
        sub(b"SCRI", 0x0004_2CD2u32.to_le_bytes()),
    ];
    let t = parse_term(0x0004_2424, &subs, &None);
    assert_eq!(t.editor_id, "P04CompanionFireTerminal");
    assert_eq!(t.body_text, "Welcome, Courier.");
    assert_eq!(t.script_form_id, 0x0004_2CD2);

    assert_eq!(t.menu_items.len(), 3);
    assert_eq!(t.menu_items[0].text, "Dismiss all companions.");
    assert_eq!(
        t.menu_items[0].result_text,
        "Accessing confirmation menu..."
    );
    assert_eq!(t.menu_items[0].flags, 0x02);
    // An all-zero RNAM is an authored empty result, not a dropped one.
    assert_eq!(t.menu_items[1].text, "No.");
    assert_eq!(t.menu_items[1].result_text, "");
    assert_eq!(t.menu_items[2].text, "Yes.");
    assert_eq!(t.menu_items[2].result_text, "Notification sent...");
    // FNV authors no ITID.
    assert!(t.menu_items.iter().all(|i| i.item_id == 0));
    // FNV authors no BSIZ.
    assert_eq!(t.body_size, 0);
}

/// Regression: #4171. Fixture transcribed from `Fallout4.esm`'s
/// `VRWorkshopShared_VRTerminalMusicSubMenu`, with the localized
/// reader actually installed — the defect the issue calls out is that
/// `parse_term` never routed ITXT/BTXT/RNAM through the lstring path,
/// so fixing the `.STRINGS` language token alone would still have left
/// every FO4 terminal blank.
#[test]
fn fo4_terminal_resolves_btxt_and_itxt_through_the_lstring_reader() {
    use crate::esm::records::common::{LocalizedPluginGuard, StringsTableGuard};
    use crate::esm::strings_table::{StringTableSet, StringsTable};

    // [count][data_size][(id, offset) × count][blob]
    let entries: [(u32, &str); 4] = [
        (0x0001, "Jukebox"),
        (0x0002, "Select a track."),
        (0x0003, "Atom Bomb Baby"),
        (0x0004, "Civilization"),
    ];
    let mut blob = Vec::new();
    let mut offsets = Vec::new();
    for (_, text) in entries {
        offsets.push(blob.len() as u32);
        blob.extend_from_slice(text.as_bytes());
        blob.push(0);
    }
    let mut raw = Vec::new();
    raw.extend_from_slice(&(entries.len() as u32).to_le_bytes());
    raw.extend_from_slice(&(blob.len() as u32).to_le_bytes());
    for (i, (id, _)) in entries.iter().enumerate() {
        raw.extend_from_slice(&id.to_le_bytes());
        raw.extend_from_slice(&offsets[i].to_le_bytes());
    }
    raw.extend_from_slice(&blob);

    let set = StringTableSet {
        strings: Some(StringsTable::parse(&raw, false).unwrap()),
        dlstrings: None,
        ilstrings: None,
    };
    let _localized = LocalizedPluginGuard::new(true);
    let _strings = StringsTableGuard::new(set);

    let subs = vec![
        sub(b"EDID", b"VRWorkshopShared_VRTerminalMusicSubMenu\0"),
        sub(b"FULL", 0x0001u32.to_le_bytes()),
        // 0x40000001 — a float, identical on all 778 FO4 terminals.
        sub(b"MNAM", [0x01, 0x00, 0x00, 0x40]),
        sub(b"BSIZ", 1u32.to_le_bytes()),
        sub(b"BTXT", 0x0002u32.to_le_bytes()),
        sub(b"ISIZ", 2u32.to_le_bytes()),
        sub(b"ITXT", 0x0003u32.to_le_bytes()),
        sub(b"ANAM", [0x08]),
        sub(b"ITID", 1u16.to_le_bytes()),
        sub(b"ITXT", 0x0004u32.to_le_bytes()),
        sub(b"ANAM", [0x08]),
        sub(b"ITID", 2u16.to_le_bytes()),
    ];
    let t = parse_term(0x0002_5001, &subs, &None);
    assert_eq!(t.full_name, "Jukebox");
    assert_eq!(
        t.body_text, "Select a track.",
        "BTXT must resolve through the lstring reader, not read as a cstring"
    );
    assert_eq!(t.body_size, 1, "BSIZ is a u32 on FO4");

    // ISIZ (2) is the authored item count and must match what the
    // repeating group produced — it does for all 778 shipped records.
    assert_eq!(t.menu_items.len(), 2);
    assert_eq!(t.menu_items[0].text, "Atom Bomb Baby");
    assert_eq!(t.menu_items[0].item_id, 1);
    assert_eq!(t.menu_items[0].flags, 0x08);
    assert_eq!(t.menu_items[1].text, "Civilization");
    assert_eq!(t.menu_items[1].item_id, 2);
    // FO4 authors RNAM on only 545 of 2 227 entries; absent is empty.
    assert!(t.menu_items.iter().all(|i| i.result_text.is_empty()));
}

/// #4641 — the FO4 TERM residual #4171 left: `NAM0`/`WNAM` header and
/// welcome text, the multi-`BTXT` body array with per-body conditions,
/// and `UNAM` display text on `ANAM = 8` menu items. 82 shipped FO4
/// terminals author 2–7 conditional bodies; 1 804 of 1 818 `ANAM = 8`
/// items carry their shown text in UNAM.
#[test]
fn fo4_term_decodes_nam0_wnam_unam_and_conditional_btxt_bodies() {
    // 24-byte CTDA (FO4 shape): comparator byte 0 (Eq), unused[3],
    // comparand f32, function u32, param_1 u32, param_2 u32, run-on
    // u32 — same layout `dialogue.rs`'s fixtures build.
    let ctda = |function: u32, param_1: u32| {
        let mut d = Vec::with_capacity(24);
        d.push(0u8);
        d.extend_from_slice(&[0, 0, 0]);
        d.extend_from_slice(&1.0f32.to_le_bytes());
        d.extend_from_slice(&function.to_le_bytes());
        d.extend_from_slice(&param_1.to_le_bytes());
        d.extend_from_slice(&0u32.to_le_bytes());
        d.extend_from_slice(&0u32.to_le_bytes());
        d
    };
    let subs = vec![
        sub(b"EDID", b"DungeonTerminalWithLogs\0"),
        sub(b"NAM0", b"ROBCO TERMLINK\0"),
        sub(b"WNAM", b"Welcome to RobCo Industries\0"),
        sub(b"BSIZ", 2u32.to_le_bytes()),
        sub(b"BTXT", b"Log entry 1: unconditionally shown.\0"),
        sub(b"BTXT", b"Log entry 2: locked until the safe is opened.\0"),
        // The second body's condition — must land on body 2, not the
        // menu item that follows it.
        sub(b"CTDA", ctda(72, 0x0002_0ABC)),
        sub(b"ISIZ", 1u32.to_le_bytes()),
        sub(b"ITXT", b"Ignore\0"),
        sub(b"ANAM", [0x08]),
        sub(b"ITID", 1u16.to_le_bytes()),
        sub(b"UNAM", b"Open the safe.\0"),
        // The item's own condition — must land on the item.
        sub(b"CTDA", ctda(72, 0x0002_0DEF)),
    ];
    let t = parse_term(0x0002_6001, &subs, &None);
    assert_eq!(t.header_text, "ROBCO TERMLINK");
    assert_eq!(t.welcome_text, "Welcome to RobCo Industries");

    assert_eq!(t.bodies.len(), 2, "each BTXT opens its own body");
    assert_eq!(t.bodies[0].text, "Log entry 1: unconditionally shown.");
    assert!(
        t.bodies[0].conditions.is_empty(),
        "body 1 authored no conditions"
    );
    assert_eq!(t.bodies[1].text, "Log entry 2: locked until the safe is opened.");
    assert_eq!(
        t.bodies[1].conditions.len(),
        1,
        "body 2's CTDA must stay on body 2"
    );
    assert_eq!(t.bodies[1].conditions[0].param_1, 0x0002_0ABC);
    assert_eq!(
        t.body_text,
        "Log entry 1: unconditionally shown.\nLog entry 2: locked until the safe is opened.",
        "the flat convenience field joins every body"
    );

    assert_eq!(t.menu_items.len(), 1);
    assert_eq!(t.menu_items[0].text, "Ignore");
    assert_eq!(
        t.menu_items[0].display_text, "Open the safe.",
        "UNAM carries the text an ANAM=8 item actually shows"
    );
    assert_eq!(
        t.menu_items[0].conditions.len(),
        1,
        "the item's CTDA must land on the item, not a body"
    );
    assert_eq!(t.menu_items[0].conditions[0].param_1, 0x0002_0DEF);
}

/// Regression: #4171. The three sub-records the old parser mined for
/// text are numeric or flag payloads, and none of them may produce a
/// menu entry. `MNAM` is the visible half of this: on FO4 its
/// `01 00 00 40` decoded as a one-character z-string and pushed a
/// spurious item onto every one of the 778 terminals.
#[test]
fn anam_dnam_and_mnam_never_contribute_terminal_text() {
    let subs = vec![
        sub(b"EDID", b"GoodspringsSchoolTerminal\0"),
        sub(b"MNAM", [0x01, 0x00, 0x00, 0x40]),
        sub(b"DNAM", [0x00, 0x02, 0x04, 0x00]),
        sub(b"ANAM", [0x08]),
    ];
    let t = parse_term(0x0008_1111, &subs, &None);
    assert!(
        t.menu_items.is_empty(),
        "MNAM/ANAM must not synthesize menu items: {:?}",
        t.menu_items
    );
    assert!(t.body_text.is_empty(), "DNAM is not body text");
    assert_eq!(t.body_size, 0);
}

/// Regression: #4171. A terminal with no menu at all must stay empty
/// rather than panic — the old `parse_term_unlocked_terminal_has_empty_password`
/// covered this shape, and the coverage is worth keeping even though
/// the field it asserted on is gone.
#[test]
fn terminal_without_a_menu_parses_to_an_empty_menu() {
    let subs = vec![
        sub(b"EDID", b"GoodspringsSchoolTerminal\0"),
        sub(b"FULL", b"School Terminal\0"),
        sub(b"DESC", b"Primer by Mr. Goodsprings.\0"),
    ];
    let t = parse_term(0x0008_1111, &subs, &None);
    assert_eq!(t.body_text, "Primer by Mr. Goodsprings.");
    assert!(t.menu_items.is_empty());
    assert_eq!(t.body_size, 0);
}

/// Regression for #2663 (SCR-D7-NEW11-02) — `parse_term` used to
/// discard `CommonNamedFields::script_instance` even though the
/// shared helper fully decoded it (mirrors
/// `parse_acti_decodes_vmad_into_script_instance`). FO4 ships 207
/// VMAD-bearing TERM records — the parser's own "TERM is FO3/FNV-only"
/// comment justifying the drop was factually wrong.
#[test]
fn parse_term_decodes_vmad_into_script_instance() {
    // version 5, objectFormat 2, 1 script "TerminalMenuScript", 0 props.
    let mut vmad = Vec::new();
    vmad.extend_from_slice(&5i16.to_le_bytes());
    vmad.extend_from_slice(&2i16.to_le_bytes());
    vmad.extend_from_slice(&1u16.to_le_bytes());
    let name = b"TerminalMenuScript";
    vmad.extend_from_slice(&(name.len() as u16).to_le_bytes());
    vmad.extend_from_slice(name);
    vmad.push(0); // script status
    vmad.extend_from_slice(&0u16.to_le_bytes()); // propCount = 0

    let subs = vec![
        sub(b"EDID", b"VRWorkshopShared_VRTerminalMusicSubMenu\0"),
        sub(b"MODL", b"setdressing\\workshop\\terminal01.nif\0"),
        sub(b"VMAD", &vmad),
    ];
    let t = parse_term(0x0002_5001, &subs, &None);
    assert_eq!(t.editor_id, "VRWorkshopShared_VRTerminalMusicSubMenu");
    let inst = t
        .script_instance
        .expect("VMAD must survive the CommonNamedFields hand-off (#2663)");
    assert_eq!(inst.scripts.len(), 1);
    assert_eq!(inst.scripts[0].name, "TerminalMenuScript");
}

// ── #808 / FNV-D2-NEW-01 stubs ─────────────────────────────────
