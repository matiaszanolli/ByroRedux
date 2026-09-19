//! FO3 vanilla-corpus integration tests (M48.5 legacy-UI track).
//!
//! Gated on `BYROREDUX_FO3_DATA` pointing at a real Fallout 3 `Data/`
//! directory (falls back to the default Steam install when present), so
//! `cargo test` stays hermetic elsewhere. These tests pin the corpus
//! facts the FO3 profile depends on and render the HUD end-to-end:
//! meters grafted from the ops-driven `meter.xml` prefab, compass
//! instantiated from `hudtemplates.xml`. Frames dump to
//! `target/menuxml/fo3_*` for visual inspection.

use std::collections::HashMap;
use std::path::PathBuf;

use byroredux_bsa::BsaArchive;
use byroredux_menuxml::font::Font;
use byroredux_menuxml::menu::{MenuAssets, MenuRenderer};
use byroredux_menuxml::profile::MenuProfile;
use byroredux_menuxml::tex::Rgba8;
use byroredux_menuxml::ScreenTraits;

const HUD_MENU: &str = "menus\\main\\hud_main_menu.xml";
const METER_PREFAB: &str = "menus\\prefabs\\meter.xml";

struct Fo3Assets {
    misc: BsaArchive,
    textures: BsaArchive,
    profile: MenuProfile,
}

impl MenuAssets for Fo3Assets {
    fn menu_xml(&self, path: &str) -> Option<Vec<u8>> {
        self.misc.extract(path).ok()
    }
    fn texture(&self, path: &str) -> Option<Vec<u8>> {
        self.textures.extract(path).ok()
    }
    fn font(&self, index: u8) -> Option<Vec<u8>> {
        // Fonts live in the *texture* BSA for FO3 — `Fallout_default.ini`
        // `[Fonts]` sFontFile_1..8 order.
        let path = self.profile.font_paths.get(index as usize - 1)?;
        self.textures.extract(path).ok()
    }
    fn font_texture(&self, path: &str) -> Option<Vec<u8>> {
        // Try both archives; atlas candidates are profile-derived.
        self.textures
            .extract(path)
            .or_else(|_| self.misc.extract(path))
            .ok()
    }
}

fn data_dir() -> Option<PathBuf> {
    if let Some(dir) = std::env::var("BYROREDUX_FO3_DATA").ok() {
        return Some(PathBuf::from(dir));
    }
    let default = PathBuf::from("/mnt/data/SteamLibrary/steamapps/common/Fallout 3 goty/Data");
    default.is_dir().then_some(default)
}

fn open_assets() -> Option<Fo3Assets> {
    let dir = data_dir()?;
    let misc = BsaArchive::open(dir.join("Fallout - Misc.bsa")).ok()?;
    let textures = BsaArchive::open(dir.join("Fallout - Textures.bsa")).ok()?;
    Some(Fo3Assets {
        misc,
        textures,
        profile: MenuProfile::fallout3(),
    })
}

/// Corpus facts the FO3 profile + HUD driver depend on.
#[test]
fn fo3_corpus_facts() {
    let Some(assets) = open_assets() else {
        eprintln!("skipping: FO3 data not found");
        return;
    };

    // The Misc BSA carries the menu corpus (no fonts — those live in the
    // texture BSA, unlike Oblivion).
    let xmls: Vec<String> = assets
        .misc
        .list_files()
        .into_iter()
        .filter(|p| p.to_lowercase().ends_with(".xml"))
        .map(str::to_string)
        .collect();
    assert!(xmls.len() >= 90, "expected the vanilla menu set, got {}", xmls.len());
    assert!(
        xmls.iter().any(|p| p.eq_ignore_ascii_case(METER_PREFAB)),
        "meter.xml prefab present"
    );

    // The font table resolves: every slot's .fnt parses with the
    // Oblivion-identical binary layout (296 B header, 256 × 14-f32
    // glyphs) and its atlas decodes.
    for index in 1..=assets.profile.font_paths.len() as u8 {
        let fnt = assets.font(index).expect("font slot extractable");
        assert_eq!(fnt.len(), 14632, "slot {index}: Oblivion-layout .fnt");
        let name = String::from_utf8_lossy(&fnt[12..])
            .split('\0')
            .next()
            .unwrap_or("")
            .to_string();
        let atlas = (assets.profile.font_atlas)(&name)
            .iter()
            .find_map(|p| assets.font_texture(p))
            .expect("atlas beside the .fnt");
        Font::parse(&fnt, &atlas).unwrap_or_else(|e| panic!("font {index} parses: {e}"));
    }

    // The HUD art the driver grafts/instantiates decodes from the
    // texture BSA through the `textures\`-prefix fallback arm.
    let tick = assets
        .texture("textures\\interface\\hud\\hud_tick_mark.dds")
        .and_then(|b| Rgba8::decode_dds(&b))
        .expect("tick mark decodes");
    eprintln!("hud_tick_mark.dds: {}x{}", tick.width, tick.height);
    let strip = assets
        .texture("textures\\interface\\hud\\glow_hud_comp_direction_strip.dds")
        .and_then(|b| Rgba8::decode_dds(&b))
        .expect("compass strip decodes");
    eprintln!(
        "glow_hud_comp_direction_strip.dds: {}x{} (compass window 345x64)",
        strip.width, strip.height
    );
    assert!(strip.width > 345, "strip wider than the window");
}

/// Full FO3 HUD render: meters grafted from `meter.xml` under the
/// `HitPoints`/`ActionPoints` container rects, compass instantiated from
/// `hudtemplates.xml` — the assembly the source engine performed in C++.
#[test]
fn fo3_hud_renders() {
    let _ = env_logger::builder().is_test(true).try_init();
    let Some(assets) = open_assets() else {
        eprintln!("skipping: FO3 data not found");
        return;
    };
    let mut hud = MenuRenderer::load_with_profile(
        &assets,
        HUD_MENU,
        ScreenTraits::new(1280.0, 720.0),
        MenuProfile::fallout3(),
    )
    .expect("FO3 HUD load");
    assert!(hud.nif_tiles >= 1, "BreathMeter NIF tile present");

    let hp = hud
        .graft_fragment(&assets, METER_PREFAB, "HitPoints", "hp_meter")
        .expect("HP meter graft");
    let ap = hud
        .graft_fragment(&assets, METER_PREFAB, "ActionPoints", "ap_meter")
        .expect("AP meter graft");
    let _compass = hud
        .instantiate_template("template_compass_window", "HUDMainMenu", "hud_compass")
        .expect("compass instantiation");
    assert_ne!(hp, ap);

    // Engine-authored layout: HP bottom-left, AP bottom-right, compass
    // top-left (the template's own x20 y65), 300px meters like the
    // template's `_TotalWidth`.
    let drive = |hud: &mut MenuRenderer, name: &str, x: f32, y: f32| {
        hud.set_override(name, "x", x);
        hud.set_override(name, "y", y);
        hud.set_override(name, "width", 300.0);
        hud.set_override(name, "_Value", 1.0);
        hud.set_override(name, "visible", 2.0);
        hud.set_override(name, "alpha", 255.0);
    };
    drive(&mut hud, "hp_meter", 30.0, 620.0);
    drive(&mut hud, "ap_meter", 950.0, 620.0);
    hud.set_override("hud_compass", "tile", 2.0);
    hud.set_override("hud_compass", "visible", 2.0);
    hud.set_override("hud_compass", "alpha", 255.0);

    let px_full = hud.render_frame(&assets).to_vec();
    dump_png("fo3_hud_full.png", 1280, 720, &px_full);
    let ink = |region: (u32, u32, u32, u32)| {
        (region.1..region.3)
            .flat_map(|y| (region.0..region.2).map(move |x| (x, y)))
            .filter(|(x, y)| px_full[(*y as usize * 1280 + *x as usize) * 4 + 3] > 0)
            .count()
    };
    let total: usize = px_full.iter().skip(3).step_by(4).filter(|&&a| a > 0).count();
    assert!(total > 2000, "HUD frame suspiciously empty ({total} ink px)");
    let hp_ink = ink((0, 560, 640, 720));
    let ap_ink = ink((640, 560, 1280, 720));
    let compass_ink = ink((0, 0, 640, 300));
    eprintln!("ink: hp={hp_ink} ap={ap_ink} compass={compass_ink} total={total}");
    assert!(hp_ink > 300, "HP meter region has ink ({hp_ink})");
    assert!(ap_ink > 300, "AP meter region has ink ({ap_ink})");
    assert!(compass_ink > 300, "compass region has ink ({compass_ink})");

    // Sample the HP tick color for the smoke classifier.
    let colored: Vec<[u8; 4]> = (600..720)
        .flat_map(|y| (40..340).map(move |x| (x, y)))
        .filter(|(x, y)| px_full[(*y as usize * 1280 + *x as usize) * 4 + 3] > 200)
        .map(|(x, y)| {
            let o = (y as usize * 1280 + x as usize) * 4;
            [
                px_full[o],
                px_full[o + 1],
                px_full[o + 2],
                px_full[o + 3],
            ]
        })
        .take(2000)
        .collect();
    if !colored.is_empty() {
        let n = colored.len();
        let avg = colored
            .iter()
            .fold([0u32; 4], |acc, c| [
                acc[0] + c[0] as u32,
                acc[1] + c[1] as u32,
                acc[2] + c[2] as u32,
                acc[3] + c[3] as u32,
            ])
            .map(|s| (s / n as u32) as u8);
        eprintln!("hp region avg opaque color: {avg:?}");
    }

    // HP at 30% → strictly less ink in the HP region.
    hud.set_override("hp_meter", "_Value", 0.3);
    let px30 = hud.render_frame(&assets).to_vec();
    dump_png("fo3_hud_hp30.png", 1280, 720, &px30);
    let ink30 = (560u32..720)
        .flat_map(|y| (0..640).map(move |x| (x, y)))
        .filter(|(x, y)| px30[(*y as usize * 1280 + *x as usize) * 4 + 3] > 0)
        .count();
    assert!(ink30 < hp_ink, "30% HP ({ink30}) must ink less than full ({hp_ink})");
}

fn dump_png(name: &str, width: u32, height: u32, rgba: &[u8]) {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/menuxml");
    let _ = std::fs::create_dir_all(&dir);
    // Composite over opaque black for viewers/analytics.
    let mut opaque = rgba.to_vec();
    for px in opaque.chunks_exact_mut(4) {
        let a = px[3] as u32;
        px[0] = ((px[0] as u32 * a) / 255) as u8;
        px[1] = ((px[1] as u32 * a) / 255) as u8;
        px[2] = ((px[2] as u32 * a) / 255) as u8;
        px[3] = 255;
    }
    if let Err(e) = write_png(&dir.join(name), width, height, &opaque) {
        eprintln!("png dump failed: {e}");
    }
}

/// Minimal uncompressed RGBA PNG writer (same as the Oblivion corpus
/// test's).
fn write_png(path: &std::path::Path, width: u32, height: u32, rgba: &[u8]) -> std::io::Result<()> {
    fn crc32(data: &[u8]) -> u32 {
        let mut table = [0u32; 256];
        for (n, slot) in table.iter_mut().enumerate() {
            let mut c = n as u32;
            for _ in 0..8 {
                c = if c & 1 != 0 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 };
            }
            *slot = c;
        }
        let mut crc = 0xFFFF_FFFFu32;
        for &b in data {
            crc = table[((crc ^ b as u32) & 0xFF) as usize] ^ (crc >> 8);
        }
        crc ^ 0xFFFF_FFFF
    }

    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend_from_slice(&width.to_be_bytes());
    ihdr.extend_from_slice(&height.to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);

    let mut raw = Vec::with_capacity(rgba.len() + height as usize);
    for y in 0..height as usize {
        raw.push(0);
        raw.extend_from_slice(&rgba[y * width as usize * 4..(y + 1) * width as usize * 4]);
    }
    let mut zl = Vec::new();
    zl.extend_from_slice(&[0x78, 0x01]);
    let mut deflate = flate_store(&raw);
    zl.append(&mut deflate);
    let mut a: u32 = 1;
    let mut b: u32 = 0;
    for &byte in &raw {
        a = (a + byte as u32) % 65_521;
        b = (b + a) % 65_521;
    }
    zl.extend_from_slice(&((b << 16) | a).to_be_bytes());

    let mut out = Vec::new();
    out.extend_from_slice(b"\x89PNG\r\n\x1a\n");
    let chunk = |len: u32, tag: &[u8; 4], data: &[u8], out: &mut Vec<u8>| {
        out.extend_from_slice(&len.to_be_bytes());
        let mut body = Vec::with_capacity(4 + data.len());
        body.extend_from_slice(tag);
        body.extend_from_slice(data);
        out.extend_from_slice(&body);
        out.extend_from_slice(&crc32(&body).to_be_bytes());
    };
    chunk(13, b"IHDR", &ihdr, &mut out);
    chunk(zl.len() as u32, b"IDAT", &zl, &mut out);
    chunk(0, b"IEND", &[], &mut out);
    std::fs::write(path, out)
}

fn flate_store(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    for chunk in data.chunks(65_535) {
        let last = (chunk.as_ptr() as usize + chunk.len()) == (data.as_ptr() as usize + data.len());
        out.push(if last { 1 } else { 0 });
        out.extend_from_slice(&(chunk.len() as u16).to_le_bytes());
        out.extend_from_slice(&(!(chunk.len() as u16)).to_le_bytes());
        out.extend_from_slice(chunk);
    }
    if data.is_empty() {
        out.push(1);
        out.extend_from_slice(&[0, 0, 0xFF, 0xFF]);
    }
    out
}

#[allow(dead_code)]
type _Unused = HashMap<(), ()>;
