//! Debug: render the vanilla HUD with the same override sequence the engine
//! drives (HUDMainMenu.user3=1, bar pins, heading) and dump PNG frames so
//! crate output can be diffed against engine screenshots.
use byroredux_bsa::BsaArchive;
use byroredux_menuxml::menu::MenuAssets;
use byroredux_menuxml::{MenuRenderer, ScreenTraits};

const FONT_PATHS: [&str; 5] = [
    "fonts\\Kingthings_Regular.fnt",
    "fonts\\Kingthings_Shadowed.fnt",
    "fonts\\Tahoma_Bold_Small.fnt",
    "fonts\\Daedric_Font.fnt",
    "fonts\\Handwritten.fnt",
];

struct Assets {
    misc: BsaArchive,
    textures: BsaArchive,
}
impl MenuAssets for Assets {
    fn menu_xml(&self, path: &str) -> Option<Vec<u8>> { self.misc.extract(path).ok() }
    fn texture(&self, path: &str) -> Option<Vec<u8>> { self.textures.extract(path).ok() }
    fn font(&self, index: u8) -> Option<Vec<u8>> {
        self.misc.extract(FONT_PATHS.get(index as usize - 1)?).ok()
    }
    fn font_texture(&self, path: &str) -> Option<Vec<u8>> { self.misc.extract(path).ok() }
}

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
    let mut a: u32 = 1;
    let mut b: u32 = 0;
    for &byte in &raw {
        a = (a + byte as u32) % 65_521;
        b = (b + a) % 65_521;
    }
    // stored (uncompressed) deflate blocks
    let mut zl = vec![0x78, 0x01];
    let mut pos = 0usize;
    while pos < raw.len() {
        let chunk = (raw.len() - pos).min(65_535);
        let last = pos + chunk == raw.len();
        zl.push(u8::from(last));
        zl.extend_from_slice(&(chunk as u16).to_le_bytes());
        zl.extend_from_slice(&(!(chunk as u16)).to_le_bytes());
        zl.extend_from_slice(&raw[pos..pos + chunk]);
        pos += chunk;
    }
    zl.extend_from_slice(&((b << 16) | a).to_be_bytes());

    let mut out = Vec::new();
    out.extend_from_slice(b"\x89PNG\r\n\x1a\n");
    let mut chunk = |len: u32, tag: &[u8; 4], data: &[u8], out: &mut Vec<u8>| {
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

fn main() {
    let dir = std::env::var("BYROREDUX_OBLIVION_DATA")
        .unwrap_or_else(|_| "/mnt/data/SteamLibrary/steamapps/common/Oblivion/Data".into());
    let assets = Assets {
        misc: BsaArchive::open(std::path::Path::new(&dir).join("Oblivion - Misc.bsa")).unwrap(),
        textures: BsaArchive::open(
            std::path::Path::new(&dir).join("Oblivion - Textures - Compressed.bsa"),
        )
        .unwrap(),
    };
    let mut renderer =
        MenuRenderer::load(&assets, "menus\\main\\hud_main_menu.xml", ScreenTraits::new(1280.0, 720.0))
            .unwrap();
    let out = std::path::PathBuf::from("/tmp/hud_crate");
    std::fs::create_dir_all(&out).unwrap();

    let mut render = |name: &str, h: f32, m: f32, f: f32, heading: f32| {
        renderer.set_override("HUDMainMenu", "user3", 1.0);
        renderer.set_override("hudmain_health_full", "user0", h);
        renderer.set_override("hudmain_magic_full", "user0", m);
        renderer.set_override("hudmain_fatigue_full", "user0", f);
        renderer.set_override("hudmain_compass_window", "user0", heading);
        let px = renderer.render_frame(&assets);
        write_png(&out.join(name), 1280, 720, px).unwrap();
        println!("wrote {name}");
    };
    render("crate_full.png", 1.0, 1.0, 1.0, 0.0);
    render("crate_pinned_h0.png", 0.35, 0.7, 1.0, 0.0);
    render("crate_pinned_h90.png", 0.35, 0.7, 1.0, 90.0);
    // and the reverse order (engine smoke order: 1.0 first, then 0.35)
    render("crate_full2.png", 1.0, 1.0, 1.0, 90.0);

    // --- probe 2: compass tiles at both headings ---
    {
        use byroredux_menuxml::eval::EvalState;
        use byroredux_menuxml::parse::parse_document;
        struct Src2<'a>(&'a Assets);
        impl byroredux_menuxml::parse::MenuFileSource for Src2<'_> {
            fn menu_xml(&self, path: &str) -> Option<Vec<u8>> { self.0.menu_xml(path) }
        }
        let xml = assets.menu_xml("menus\\main\\hud_main_menu.xml").unwrap();
        let text = String::from_utf8(xml).unwrap();
        let mut src = Src2(&assets);
        let doc = parse_document(&text, &mut src);
        for heading in [0.0f32, 90.0] {
            let overrides: byroredux_menuxml::eval::Overrides = [
                ("hudmainmenu".to_string(), "user3".to_string()),
                ("hudmain_health_full".to_string(), "user0".to_string()),
                ("hudmain_magic_full".to_string(), "user0".to_string()),
                ("hudmain_fatigue_full".to_string(), "user0".to_string()),
                ("hudmain_compass_window".to_string(), "user0".to_string()),
            ]
            .into_iter()
            .map(|(k, v)| if k.ends_with("compass_window") { ((k, v), byroredux_menuxml::Scalar::Num(heading)) } else if k.ends_with("health_full") { ((k, v), byroredux_menuxml::Scalar::Num(0.35)) } else if k.ends_with("magic_full") { ((k, v), byroredux_menuxml::Scalar::Num(0.7)) } else if k.ends_with("fatigue_full") { ((k, v), byroredux_menuxml::Scalar::Num(1.0)) } else { ((k, v), byroredux_menuxml::Scalar::Num(1.0)) })
            .collect();
            let empty = std::collections::HashMap::new();
            let mut eval = EvalState::new(&doc, byroredux_menuxml::ScreenTraits::new(1280.0, 720.0), &empty, &overrides);
            eval.resolve_all();
            println!("== heading {heading} ==");
            for (idx, tile) in doc.tiles.iter().enumerate() {
                let n = tile.name.as_deref().unwrap_or("").to_lowercase();
                if n.contains("compass") || n.ends_with("health_full") || n.ends_with("magic_full") {
                    let mut g = |t: &str| eval.trait_value(idx, t);
                    println!("  {n}: x={:?} y={:?} w={:?} u0={:?} vis={:?}", g("x"), g("y"), g("width"), g("user0"), g("visible"));
                }
            }
        }
    }
    // --- trait probe: evaluate the health/magic/fatigue tiles directly ---
    {
        use byroredux_menuxml::eval::EvalState;
        use byroredux_menuxml::layout::build_draw_list;
        use byroredux_menuxml::parse::parse_document;
        let xml = assets.menu_xml("menus\\main\\hud_main_menu.xml").unwrap();
        let text = String::from_utf8(xml).unwrap();
        struct Src<'a>(&'a Assets);
        impl byroredux_menuxml::parse::MenuFileSource for Src<'_> {
            fn menu_xml(&self, path: &str) -> Option<Vec<u8>> { self.0.menu_xml(path) }
        }
        let mut src = Src(&assets);
        let doc = parse_document(&text, &mut src);
        let overrides = [
            (("hudmainmenu".to_string(), "user3".into()), 1.0),
            (("hudmain_health_full".into(), "user0".into()), 1.0),
            (("hudmain_magic_full".into(), "user0".into()), 1.0),
            (("hudmain_fatigue_full".into(), "user0".into()), 1.0),
            (("hudmain_compass_window".into(), "user0".into()), 90.0),
        ]
        .into_iter()
        .map(|(k, v)| (k, byroredux_menuxml::Scalar::Num(v)))
        .collect::<byroredux_menuxml::eval::Overrides>();
        let empty = std::collections::HashMap::new();
        let mut eval = EvalState::new(&doc, ScreenTraits::new(1280.0, 720.0), &empty, &overrides);
        eval.resolve_all();
        for (idx, tile) in doc.tiles.iter().enumerate() {
            let n = tile.name.as_deref().unwrap_or("").to_lowercase();
            if n.contains("health") || n.contains("magic_full") || n.contains("fatigue_full") {
                let mut g = |t: &str| eval.trait_value(idx, t);
                println!(
                    "tile {:?}: kind={:?} x={:?} y={:?} w={:?} h={:?} u0={:?} vis={:?} depth={:?}",
                    tile.name, tile.kind, g("x"), g("y"), g("width"), g("height"),
                    g("user0"), g("visible"), g("depth")
                );
            }
        }
        let items = build_draw_list(&doc, &mut eval);
        for item in &items {
            if let byroredux_menuxml::layout::DrawItem::Image { tile: _, rect, filename, .. } = item {
                if filename.to_lowercase().contains("health") || filename.to_lowercase().contains("magic") || filename.to_lowercase().contains("fatigue") {
                    println!("draw {filename}: rect={:?}", rect);
                }
            }
        }
    }
}
