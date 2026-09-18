//! Parser + evaluator + layout + rasterizer regression tests.
//!
//! Synthetic fixtures mirror the vanilla constructs each test pins
//! (overlapping comments, `onlyif` gating, locus chains, switch-case
//! `copy`); the on-disk vanilla corpus is exercised by the env-gated
//! integration test when `BYROREDUX_OBLIVION_DATA` is set.

use std::collections::HashMap;

use crate::eval::{EvalState, Overrides, ScreenTraits};
use crate::layout::build_draw_list;
use crate::menu::{MenuAssets, MenuRenderer};
use crate::parse::{parse_document, MenuFileSource, RawTrait, Scalar, TileKind};

/// File source over an in-memory map (synthetic fixtures).
struct MapSource {
    files: HashMap<String, String>,
}

impl MenuFileSource for MapSource {
    fn menu_xml(&self, path: &str) -> Option<Vec<u8>> {
        self.files
            .get(&path.to_lowercase())
            .map(|s| s.as_bytes().to_vec())
    }
}

fn src(files: &[(&str, &str)]) -> MapSource {
    MapSource {
        files: files
            .iter()
            .map(|(k, v)| (k.to_lowercase(), v.to_string()))
            .collect(),
    }
}

// ---------------------------------------------------------------------------
// Parser
// ---------------------------------------------------------------------------

#[test]
fn parses_menu_with_nested_tiles_and_traits() {
    let files = src(&[(
        "menus\\main\\hud_main_menu.xml",
        r#"
        <!-- hud_main_menu.xml -->
        <menu name="HUDMainMenu">
            <alpha> 0 </alpha>
            <locus> &true; </locus>
            <user3> 1 </user3>
            <rect name="child_a">
                <x> 10 </x>
                <y> 20 </y>
                <image name="grandchild">
                    <filename> Menus\HUD\hud_back.dds </filename>
                    <width> 620 </width>
                </image>
            </rect>
        </menu>
        "#,
    )]);
    let mut s = files;
    let root_xml = s.files.get("menus\\main\\hud_main_menu.xml").unwrap().clone();
    let doc = parse_document(&root_xml, &mut s);
    assert_eq!(doc.menu_name, "HUDMainMenu");
    assert_eq!(doc.tiles.len(), 3);
    assert_eq!(doc.tiles[0].kind, TileKind::Menu);
    assert_eq!(doc.tiles[0].traits.get("alpha"), Some(&RawTrait::Num(0.0)));
    // &true; expands to 2.
    assert_eq!(doc.tiles[0].traits.get("locus"), Some(&RawTrait::Num(2.0)));
    let child = doc.name_index["child_a"];
    assert_eq!(doc.tiles[child].parent, Some(0));
    let grand = doc.name_index["grandchild"];
    assert_eq!(doc.tiles[grand].parent, Some(child));
    match doc.tiles[grand].traits.get("filename") {
        Some(RawTrait::Str(s)) => assert_eq!(s.trim(), "Menus\\HUD\\hud_back.dds"),
        other => panic!("filename should be a string trait, got {other:?}"),
    }
}

/// Vanilla's overlapping comment spans must parse leniently: the
/// `<alpha>` op chain that lives between a closed comment and a second
/// comment swallowing a stray `</text-->` must survive.
#[test]
fn overlapping_comment_spans_parse_leniently() {
    let xml = r#"
        <menu name="M">
            <image name="effect_icon">
                <alpha>
                    <copy src="parent()" trait="alpha"/>
                    <onlyif>
                        <copy src="parent()" trait="user1"/>	<!-- time left < 0 for constant effects --><!--
                        <gte>0</gte>
                    </onlyif>
                </alpha>
            <!--image name="dead_code">
                <string> x </string>
            </text-->
            </image>
        </menu>
        "#;
    let mut s = src(&[]);
    let doc = parse_document(xml, &mut s);
    let icon = doc.name_index["effect_icon"];
    match doc.tiles[icon].traits.get("alpha") {
        Some(RawTrait::Ops(ops)) => assert_eq!(ops.len(), 2, "copy + onlyif survive the comment mess"),
        other => panic!("alpha should be an op chain, got {other:?}"),
    }
}

#[test]
fn include_splices_prefab_traits_and_children() {
    let files = src(&[
        (
            "menus\\root.xml",
            r#"
            <menu name="Root">
                <image name="host">
                    <include src="prefabs\button_long.xml"/>
                    <id> 14 </id>
                </image>
            </menu>
            "#,
        ),
        (
            "menus\\prefabs\\button_long.xml",
            r#"
            <!-- button_long.xml -->
            <width> 177 </width>
            <user1> &true; </user1>
            <text name="button_text">
                <string> <copy src="parent()" trait="user0"/> </string>
            </text>
            "#,
        ),
    ]);
    let mut s = files;
    let root_xml = s.files.get("menus\\root.xml").unwrap().clone();
    let doc = parse_document(&root_xml, &mut s);
    let host = doc.name_index["host"];
    // Prefab traits merged into the host, host's own id retained.
    assert_eq!(doc.tiles[host].traits.get("width"), Some(&RawTrait::Num(177.0)));
    assert_eq!(doc.tiles[host].id, Some(14));
    // Prefab child became a host child.
    let text = doc.name_index["button_text"];
    assert_eq!(doc.tiles[text].parent, Some(host));
}

/// Include cycles must terminate with a warning, not a stack overflow.
#[test]
fn include_cycles_terminate() {
    let files = src(&[
        (
            "menus\\a.xml",
            r#"<menu name="A"><image name="x"><include src="b.xml"/></image></menu>"#,
        ),
        (
            "menus\\b.xml",
            r#"<include src="b.xml"/><width> 5 </width>"#,
        ),
    ]);
    let mut s = files;
    let root_xml = s.files.get("menus\\a.xml").unwrap().clone();
    let doc = parse_document(&root_xml, &mut s);
    let x = doc.name_index["x"];
    assert_eq!(doc.tiles[x].traits.get("width"), Some(&RawTrait::Num(5.0)));
}

// ---------------------------------------------------------------------------
// Evaluator — pins the wiki's fold semantics against vanilla constructs
// ---------------------------------------------------------------------------

/// Evaluate every trait of `xml` and return (doc, per-tile resolved
/// values via a fresh state). Uses a 1280×720 screen (cropx = 160).
fn eval_all(xml: &str) -> (crate::parse::Document, HashMap<(usize, String), Scalar>) {
    let mut s = src(&[]);
    let doc = parse_document(xml, &mut s);
    let strings = HashMap::new();
    let overrides = Overrides::new();
    let mut eval = EvalState::new(
        // SAFETY-free trick: leak is unnecessary — build state inside.
        &doc,
        ScreenTraits::new(1280.0, 720.0),
        &strings,
        &overrides,
    );
    let mut memo = HashMap::new();
    for tile in 0..doc.tiles.len() {
        let keys: Vec<String> = doc.tiles[tile].traits.keys().cloned().collect();
        for key in keys {
            let v = eval.trait_value(tile, &key);
            memo.insert((tile, key), v);
        }
    }
    (doc, memo)
}

/// The health-bar width chain: `user0 (clamped 0..1) * 163`.
#[test]
fn fold_chain_computes_bar_width() {
    let (doc, memo) = eval_all(r#"
        <menu name="M">
            <image name="bar">
                <width>
                    <copy src="me()" trait="user0"/>
                    <max> 0 </max>
                    <min> 1 </min>
                    <mul> 163 </mul>
                </width>
                <user0> 0.5 </user0>
            </image>
        </menu>"#);
    let bar = doc.name_index["bar"];
    assert_eq!(memo[&(bar, "width".into())].as_num(), 81.5);
}

/// `onlyif` returns the working value when the argument is true, else 0 —
/// the weapon-icon alpha pattern.
#[test]
fn onlyif_gates_working_value() {
    let (doc, memo) = eval_all(r#"
        <menu name="M">
            <image name="w">
                <alpha>
                    <copy> 128 </copy>
                    <onlyif>
                        <copy src="me()" trait="user2"/>
                        <gte> 50 </gte>
                    </onlyif>
                    <add> 128 </add>
                </alpha>
                <user2> 75 </user2>
            </image>
            <image name="w2">
                <alpha>
                    <copy> 128 </copy>
                    <onlyif>
                        <copy src="me()" trait="user2"/>
                        <gte> 50 </gte>
                    </onlyif>
                    <add> 128 </add>
                </alpha>
                <user2> 25 </user2>
            </image>
        </menu>"#);
    let w = doc.name_index["w"];
    let w2 = doc.name_index["w2"];
    // cond true → 128 + 128
    assert_eq!(memo[&(w, "alpha".into())].as_num(), 256.0);
    // cond false → 0 + 128
    assert_eq!(memo[&(w2, "alpha".into())].as_num(), 128.0);
}

/// Comparisons return 2 (&true;) — the background-alpha chain
/// `user3==0 → sub 1 → mul 255` only lands on 255 because eq yields 2.
#[test]
fn booleans_are_two() {
    let (doc, memo) = eval_all(r#"
        <menu name="M">
            <image name="bg">
                <alpha>
                    <copy src="MENUTRAIT" trait="user3"/>
                    <eq> 0 </eq>
                    <sub> 1 </sub>
                    <mul> 255 </mul>
                </alpha>
            </image>
        </menu>"#);
    let bg = doc.name_index["bg"];
    // With the override absent, user3 defaults to 0 → eq true (2) →
    // 2-1=1 → 255.
    assert_eq!(memo[&(bg, "alpha".into())].as_num(), 255.0);
}

/// Switch-case `copy`: selecting `_name_` with numeric working value
/// picks `_name_<value>`.
#[test]
fn copy_switch_case_picks_indexed_trait() {
    let (doc, memo) = eval_all(r#"
        <menu name="M">
            <user4> 2 </user4>
            <user5> 3 </user5>
            <nif name="brackets">
                <animation>
                    <copy src="M" trait="user4"/>
                    <mult> 10 </mult>
                    <add src="M" trait="user5"/>
                    <copy src="me()" trait="_animation_"/>
                </animation>
                <_animation_23> B_C </_animation_23>
                <_animation_24> B_D </_animation_24>
            </nif>
        </menu>"#);
    let brackets = doc.name_index["brackets"];
    match &memo[&(brackets, "animation".into())] {
        Scalar::Str(s) => assert_eq!(s.trim(), "B_C"),
        other => panic!("animation should resolve to case string, got {other:?}"),
    }
}

/// `screen()` traits: width/height/cropx with the 4:3 crop model.
#[test]
fn screen_traits_answer_widescreen_crop() {
    let (doc, memo) = eval_all(r#"
        <menu name="M">
            <rect name="r">
                <x>
                    <copy src="screen()" trait="cropx"/>
                    <add src="screen()" trait="cropx"/>
                    <sub> 60 </sub>
                    <max src="screen()" trait="cropx"/>
                    <add> 67 </add>
                </x>
            </rect>
        </menu>"#);
    let r = doc.name_index["r"];
    // 160+160-60 = 260 > 160 → 260 + 67
    assert_eq!(memo[&(r, "x".into())].as_num(), 327.0);
}

/// Nested children as parenthesised sub-expression:
/// `x = screen.width - (cropx * 2) - 24`.
#[test]
fn nested_children_group() {
    let (doc, memo) = eval_all(r#"
        <menu name="M">
            <rect name="r">
                <x>
                    <copy src="screen()" trait="width"/>
                    <sub>
                        <copy src="screen()" trait="cropx"/>
                        <mult>2</mult>
                    </sub>
                    <sub> 24 </sub>
                </x>
            </rect>
        </menu>"#);
    let r = doc.name_index["r"];
    assert_eq!(memo[&(r, "x".into())].as_num(), 1280.0 - 320.0 - 24.0);
}

/// Engine overrides beat authored traits — the health-fraction channel.
#[test]
fn overrides_win_over_authored() {
    let mut s = src(&[]);
    let doc = parse_document(
        r#"<menu name="M"><image name="bar"><user0> 1.0 </user0></image></menu>"#,
        &mut s,
    );
    let strings = HashMap::new();
    let mut overrides = Overrides::new();
    overrides.insert(("bar".into(), "user0".into()), Scalar::Num(0.25));
    let mut eval = EvalState::new(&doc, ScreenTraits::new(1280.0, 720.0), &strings, &overrides);
    let bar = doc.name_index["bar"];
    assert_eq!(eval.trait_value(bar, "user0").as_num(), 0.25);
}

// ---------------------------------------------------------------------------
// Layout
// ---------------------------------------------------------------------------

/// Locus chain: children position against the nearest locus ancestor;
/// non-locus tiles pass the origin through.
#[test]
fn locus_chain_positions_correctly() {
    let (doc, _memo) = eval_all(r#"
        <menu name="M">
            <locus> &true; </locus>
            <x> 100 </x>
            <y> 200 </y>
            <image name="bg">
                <locus> &true; </locus>
                <x> 10 </x>
                <y> 20 </y>
                <width> 620 </width>
                <height> 70 </height>
                <filename> Menus\HUD\hud_back.dds </filename>
                <image name="frame">
                    <x> -70 </x>
                    <y> -5 </y>
                    <width> 761 </width>
                    <height> 90 </height>
                    <filename> Menus\HUD\hud_frame.dds </filename>
                </image>
            </image>
        </menu>"#);
    let strings = HashMap::new();
    let overrides = Overrides::new();
    let mut eval = EvalState::new(&doc, ScreenTraits::new(1280.0, 720.0), &strings, &overrides);
    let items = build_draw_list(&doc, &mut eval);
    // Menu root itself has no filename; two images drawn.
    assert_eq!(items.len(), 2);
    let by_tile: HashMap<_, _> = items
        .iter()
        .map(|i| (i.tile(), i))
        .collect::<std::collections::HashMap<_, _>>();
    let bg = doc.name_index["bg"];
    let frame = doc.name_index["frame"];
    match by_tile[&bg] {
        crate::layout::DrawItem::Image { rect, .. } => {
            assert_eq!((rect.x, rect.y), (110.0, 220.0));
        }
        other => panic!("bg should be an image, got {other:?}"),
    }
    // frame is a child of bg but NOT a locus tile — it still positions
    // against bg (the nearest locus ancestor).
    match by_tile[&frame] {
        crate::layout::DrawItem::Image { rect, .. } => {
            assert_eq!((rect.x, rect.y), (40.0, 215.0));
        }
        other => panic!("frame should be an image, got {other:?}"),
    }
}

/// Depth ordering: siblings draw ascending `depth` regardless of
/// document order.
#[test]
fn depth_sorts_siblings() {
    let (doc, _memo) = eval_all(r#"
        <menu name="M">
            <image name="front"> <depth> 18 </depth> <filename> a.dds </filename> </image>
            <image name="back"> <depth> 0 </depth> <filename> b.dds </filename> </image>
        </menu>"#);
    let strings = HashMap::new();
    let overrides = Overrides::new();
    let mut eval = EvalState::new(&doc, ScreenTraits::new(1280.0, 720.0), &strings, &overrides);
    let items = build_draw_list(&doc, &mut eval);
    let back = doc.name_index["back"];
    assert_eq!(items[0].tile(), back, "depth-0 back draws first");
}

// ---------------------------------------------------------------------------
// Rasterizer
// ---------------------------------------------------------------------------

#[test]
fn blit_scales_and_tints() {
    use crate::layout::Rect;
    use crate::raster::Framebuffer;
    use crate::tex::Rgba8;

    let mut tex = Rgba8::new(2, 2);
    // Red opaque quadrant pattern.
    for (i, px) in tex.pixels.chunks_exact_mut(4).enumerate() {
        let odd = i % 2 == 1;
        px.copy_from_slice(&[255, 0, 0, if odd { 255 } else { 0 }]);
    }
    let mut fb = Framebuffer::new(4, 2);
    fb.blit(
        &tex,
        Rect { x: 0.0, y: 0.0, w: 4.0, h: 2.0 },
        (0.0, 0.0),
        [255.0, 255.0, 255.0],
        255.0,
        None,
    );
    // 2x horizontal stretch: columns 0-1 sample texel col 0 (alpha 0),
    // columns 2-3 sample col 1 (opaque red).
    assert_eq!(&fb.pixels[8..12], &[255, 0, 0, 255]);
    assert_eq!(&fb.pixels[20..24], &[0, 0, 0, 0], "col 1 samples texel 0");
    assert_eq!(&fb.pixels[24..28], &[255, 0, 0, 255]);
    // Transparent half untouched.
    assert_eq!(&fb.pixels[0..4], &[0, 0, 0, 0]);
}

#[test]
fn crop_selects_atlas_cell_at_stretch_zoom() {
    use crate::layout::Rect;
    use crate::raster::Framebuffer;
    use crate::tex::Rgba8;

    // 64×1 atlas: two 32px cells — black then white.
    let mut tex = Rgba8::new(64, 1);
    for x in 32..64 {
        let o = x * 4;
        tex.pixels[o..o + 4].copy_from_slice(&[255, 255, 255, 255]);
    }
    let mut fb = Framebuffer::new(8, 1);
    // 32px tile at stretch zoom; crop 32 display px → texel offset
    // 32 * (64/8) / ... — the compass-icon math: display crop on a
    // tile whose size matches the cell → 1:1 texel crop.
    fb.blit(
        &tex,
        Rect { x: 0.0, y: 0.0, w: 32.0, h: 1.0 },
        (32.0, 0.0),
        [255.0, 255.0, 255.0],
        255.0,
        None,
    );
    // The white cell should now fill the first 32 display pixels…
    // clipped to the 8px framebuffer.
    for x in 0..8 {
        let o = x * 4;
        assert_eq!(fb.pixels[o + 3], 255, "pixel {x} should be opaque white");
    }
}

#[test]
fn text_line_advances_and_tints() {
    use crate::font::{Font, Glyph};
    use crate::raster::Framebuffer;
    use crate::tex::Rgba8;

    // 8×8 atlas, 'A' cell fully inked in the left half.
    let mut atlas = Rgba8::new(8, 8);
    for y in 0..8 {
        for x in 0..4 {
            let o = (y * 8 + x) * 4;
            atlas.pixels[o..o + 4].copy_from_slice(&[255, 255, 255, 255]);
        }
    }
    let glyphs = (0..256)
        .map(|c| {
            let inked = c < 128;
            Glyph {
                u0: 0.0,
                v0: 0.0,
                u1: if inked { 0.5 } else { 0.0 },
                v1: if inked { 1.0 } else { 0.0 },
                width: if inked { 4.0 } else { 0.0 },
                height: if inked { 8.0 } else { 0.0 },
                advance: if inked { 6.0 } else { 0.0 },
                y_offset: 0.0,
                inked,
            }
        })
        .collect();
    let font = Font {
        point_size: 8.0,
        texture_name: "test".into(),
        glyphs,
        atlas,
    };
    let mut fb = Framebuffer::new(32, 8);
    fb.text_line(&font, "AA", 0.0, 0.0, 0, [255.0, 0.0, 0.0], 255.0, None);
    // First glyph: x 0..4 red; pen then at 6 → second glyph x 6..10.
    let o = 0 * 4;
    assert_eq!(&fb.pixels[o..o + 4], &[255, 0, 0, 255]);
    let o = 5 * 4;
    assert_eq!(fb.pixels[o + 3], 0, "gap between glyphs stays empty");
    let o = 6 * 4;
    assert_eq!(&fb.pixels[o..o + 4], &[255, 0, 0, 255]);
    assert_eq!(font.measure_width("AA"), 12.0);
}

// ---------------------------------------------------------------------------
// MenuRenderer (synthetic assets end-to-end)
// ---------------------------------------------------------------------------

/// Minimal in-memory asset set: one XML, one 1×1 DDS, one font.
struct SynthAssets {
    xml: Vec<u8>,
    strings: Vec<u8>,
}

impl MenuAssets for SynthAssets {
    fn menu_xml(&self, path: &str) -> Option<Vec<u8>> {
        match path.to_lowercase().as_str() {
            "menus\\main\\hud_main_menu.xml" => Some(self.xml.clone()),
            "menus\\strings.xml" => Some(self.strings.clone()),
            _ => None,
        }
    }
    fn texture(&self, _path: &str) -> Option<Vec<u8>> {
        // A 1×1 uncompressed DDS is awkward to hand-roll; the renderer
        // skips missing textures, which this test relies on.
        None
    }
    fn font(&self, _index: u8) -> Option<Vec<u8>> {
        None
    }
    fn font_texture(&self, _path: &str) -> Option<Vec<u8>> {
        None
    }
}

#[test]
fn renderer_produces_frame_and_applies_overrides() {
    let assets = SynthAssets {
        xml: br#"
        <menu name="HUDMainMenu">
            <locus> &true; </locus>
            <image name="hudmain_health_full">
                <width>
                    <copy src="me()" trait="user0"/>
                    <min> 1 </min>
                    <mul> 163 </mul>
                </width>
                <height> 11 </height>
                <user0> 1.0 </user0>
            </image>
        </menu>"#
        .to_vec(),
        strings: br#"<rect name="Strings"><_done> Done </_done></rect>"#.to_vec(),
    };
    let mut r = MenuRenderer::load(
        &assets,
        "menus\\main\\hud_main_menu.xml",
        ScreenTraits::new(1280.0, 720.0),
    )
    .expect("load");
    let px = r.render_frame(&assets);
    assert_eq!(px.len(), 1280 * 720 * 4);
    // All-transparent frame (texture missing → skip, no panic).
    assert!(px.iter().skip(3).step_by(4).all(|&a| a == 0));
    // Override plumbing exists.
    r.set_override("hudmain_health_full", "user0", 0.5);
    let _ = r.render_frame(&assets);
}
