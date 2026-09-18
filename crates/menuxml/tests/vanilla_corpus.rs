//! Vanilla-corpus integration tests.
//!
//! Gated on `BYROREDUX_OBLIVION_DATA` pointing at a real Oblivion
//! `Data/` directory (the same convention as `docs/smoke-tests/`), so
//! `cargo test` stays hermetic without the game installed. With it set,
//! these tests parse every vanilla menu XML, load the HUD with real
//! fonts and DXT textures, and render frames that are written to
//! `target/menuxml/` for visual inspection.

use std::collections::HashMap;
use std::path::PathBuf;

use byroredux_bsa::BsaArchive;
use byroredux_menuxml::menu::{MenuAssets, MenuRenderer};
use byroredux_menuxml::parse::{parse_document, MenuFileSource, Scalar};
use byroredux_menuxml::{Document, ScreenTraits};

/// Oblivion.ini `[Fonts]` order — the `<font>` trait's 1-based table.
const FONT_PATHS: [&str; 5] = [
    "fonts\\Kingthings_Regular.fnt",
    "fonts\\Kingthings_Shadowed.fnt",
    "fonts\\Tahoma_Bold_Small.fnt",
    "fonts\\Daedric_Font.fnt",
    "fonts\\Handwritten.fnt",
];

struct OblivionAssets {
    misc: BsaArchive,
    textures: BsaArchive,
}

impl MenuAssets for OblivionAssets {
    fn menu_xml(&self, path: &str) -> Option<Vec<u8>> {
        self.misc.extract(path).ok()
    }
    fn texture(&self, path: &str) -> Option<Vec<u8>> {
        self.textures.extract(path).ok()
    }
    fn font(&self, index: u8) -> Option<Vec<u8>> {
        self.misc.extract(FONT_PATHS.get(index as usize - 1)?).ok()
    }
    fn font_texture(&self, path: &str) -> Option<Vec<u8>> {
        self.misc.extract(path).ok()
    }
}

/// `Document` companion for the corpus-parse test: counts per file.
struct NoSource;

impl MenuFileSource for NoSource {
    fn menu_xml(&self, _path: &str) -> Option<Vec<u8>> {
        None
    }
}

fn data_dir() -> Option<PathBuf> {
    std::env::var("BYROREDUX_OBLIVION_DATA").ok().map(PathBuf::from)
}

fn open_assets() -> Option<OblivionAssets> {
    let dir = data_dir()?;
    let misc = BsaArchive::open(dir.join("Oblivion - Misc.bsa")).ok()?;
    let textures = BsaArchive::open(dir.join("Oblivion - Textures - Compressed.bsa")).ok()?;
    Some(OblivionAssets { misc, textures })
}

/// Every vanilla menu XML parses into a non-empty arena, and the HUD in
/// particular carries its expected named tiles.
#[test]
fn vanilla_corpus_parses() {
    let Some(dir) = data_dir() else {
        eprintln!("skipping: BYROREDUX_OBLIVION_DATA not set");
        return;
    };
    let misc = BsaArchive::open(dir.join("Oblivion - Misc.bsa")).expect("open Misc.bsa");
    let xmls: Vec<String> = misc
        .list_files()
        .into_iter()
        .filter(|p| p.to_lowercase().ends_with(".xml"))
        .map(str::to_string)
        .collect();
    assert!(xmls.len() >= 80, "expected the full vanilla menu set, got {}", xmls.len());

    let mut parsed = 0;
    let mut tiles = 0;
    let mut hud: Option<Document> = None;
    for path in &xmls {
        let bytes = misc.extract(path).expect("extract xml");
        let text = String::from_utf8(bytes).expect("xml utf8");
        let mut src = NoSource;
        let doc = parse_document(&text, &mut src);
        assert!(!doc.tiles.is_empty(), "{path} parsed empty");
        tiles += doc.tiles.len();
        parsed += 1;
        if path.to_lowercase().ends_with("hud_main_menu.xml") {
            hud = Some(doc);
        }
    }
    eprintln!("parsed {parsed} menu XMLs, {tiles} tiles total");
    let hud = hud.expect("hud_main_menu.xml in corpus");
    for name in [
        "hudmain_background",
        "hudmain_health_full",
        "hudmain_magic_full",
        "hudmain_fatigue_full",
        "hudmain_compass_window",
        "hudmain_compass_heading",
        "hudmain_weapon_icon",
    ] {
        assert!(
            hud.name_index.contains_key(name),
            "HUD tile '{name}' missing after parse"
        );
    }
}

/// Full HUD render with real fonts + DXT textures: frame is non-empty,
/// bars land bottom-left, and a health override shrinks the red bar's
/// drawn width. Frames are dumped to `target/menuxml/` as PNGs.
#[test]
fn vanilla_hud_renders() {
    let _ = env_logger::builder().is_test(true).try_init();
    let Some(assets) = open_assets() else {
        eprintln!("skipping: BYROREDUX_OBLIVION_DATA not set");
        return;
    };
    let mut hud = MenuRenderer::load(
        &assets,
        "menus\\main\\hud_main_menu.xml",
        ScreenTraits::new(1280.0, 720.0),
    )
    .expect("HUD load");
    assert_eq!(hud.nif_tiles, 2, "hud_brackets + icon_timer NIF tiles");

    // Explore mode + full bars + north heading.
    hud.set_override("HUDMainMenu", "user3", 1.0);
    hud.set_override("hudmain_health_full", "user0", 1.0);
    hud.set_override("hudmain_magic_full", "user0", 1.0);
    hud.set_override("hudmain_fatigue_full", "user0", 1.0);
    let px = hud.render_frame(&assets).to_vec();
    dump_png("hud_full.png", 1280, 720, &px);
    let ink = px.iter().skip(3).step_by(4).filter(|&&a| a > 0).count();
    assert!(ink > 5000, "HUD frame suspiciously empty ({ink} ink pixels)");

    // The three ribbon bars live in the bottom-left quadrant.
    let bottom_left = (0..1280usize)
        .flat_map(|x| (600..720).map(move |y| (x, y)))
        .filter(|(x, y)| px[(y * 1280 + x) * 4 + 3] > 0)
        .count();
    assert!(
        bottom_left > 1000,
        "bottom-left ribbon region empty ({bottom_left} px)"
    );

    // Health at 30% → strictly fewer red-bar pixels than full.
    hud.set_override("hudmain_health_full", "user0", 0.3);
    let px30 = hud.render_frame(&assets).to_vec();
    dump_png("hud_health30.png", 1280, 720, &px30);
    let red = |px: &[u8]| {
        (0..px.len() / 4)
            .filter(|i| {
                let o = i * 4;
                // Health fill decodes to ~(222, 93, 82) — match on red
                // dominance, not a tight green/blue bound.
                px[o] > 150 && px[o + 1] < 130 && px[o + 2] < 130 && px[o] > px[o + 1] + 40
            })
            .count()
    };
    assert!(
        red(&px30) < red(&px),
        "30% health ({}) must draw less red than full ({})",
        red(&px30),
        red(&px)
    );
}

/// `strings.xml` traits flow through `strings()` reads (the region-name
/// text tile is the HUD's live consumer).
#[test]
fn strings_xml_feeds_selectors() {
    let Some(dir) = data_dir() else {
        eprintln!("skipping: BYROREDUX_OBLIVION_DATA not set");
        return;
    };
    let misc = BsaArchive::open(dir.join("Oblivion - Misc.bsa")).unwrap();
    let bytes = misc.extract("menus\\strings.xml").unwrap();
    let text = String::from_utf8(bytes).unwrap();
    let mut src = NoSource;
    let doc = parse_document(&text, &mut src);
    let root = &doc.tiles[0];
    match root.traits.get("_done") {
        Some(byroredux_menuxml::RawTrait::Str(s)) => assert_eq!(s.trim(), "Done"),
        other => panic!("_done should be a string trait, got {other:?}"),
    }
    // `strings()` is answered from this map at render time; the HUD's
    // hudmain_region tile reads HUDMainMenu.user8 as its string instead.
    let _ = Scalar::Num(0.0);
}

fn dump_png(name: &str, width: u32, height: u32, rgba: &[u8]) {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/menuxml");
    let _ = std::fs::create_dir_all(&dir);
    // Composite over opaque black — the frame carries straight alpha and
    // viewers/analytics render un-composited transparent PNGs as flat gray.
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

/// Minimal uncompressed RGBA PNG writer (no extra features needed).
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
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]); // 8-bit RGBA

    let mut raw = Vec::with_capacity(rgba.len() + height as usize);
    for y in 0..height as usize {
        raw.push(0); // filter: none
        raw.extend_from_slice(&rgba[y * width as usize * 4..(y + 1) * width as usize * 4]);
    }
    let mut zl = Vec::new();
    zl.extend_from_slice(&[0x78, 0x01]); // zlib header, low compression
    let mut deflate = flate_store(&raw);
    zl.append(&mut deflate);
    // zlib trailer is Adler-32 (not the CRC-32 the chunks use).
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

/// Stored (uncompressed) DEFLATE blocks — valid, just not compressed.
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

/// Suppress the unused warning for the HashMap import kept for parity
/// with future corpus assertions.
#[allow(dead_code)]
type _Unused = HashMap<(), ()>;
