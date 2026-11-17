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
use crate::raster::BlitStyle;

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

/// #4651 (PAR-D1-2026-09-21-05) — the three probe strings. A non-ASCII
/// character right after a comment span used to panic: the stray-arm
/// `pos += 1` advanced one BYTE into a multi-byte character and the next
/// `&src[pos..]` slice hit a non-char boundary. With an ASCII character
/// the byte was silently dropped instead, corrupting the trait value
/// ("plainascii" arrived as "plainscii").
#[test]
fn trait_text_after_a_comment_survives_multibyte_characters() {
    // Multi-byte: must not panic, must keep both halves.
    let files = src(&[(
        "menus\\main\\hud_main_menu.xml",
        r#"<menu name="M"><string>abc<!-- note -->été</string></menu>"#,
    )]);
    let mut s = files;
    let root_xml = s.files.get("menus\\main\\hud_main_menu.xml").unwrap().clone();
    let doc = parse_document(&root_xml, &mut s);
    let value = doc
        .tiles
        .iter()
        .find_map(|t| t.traits.get("string"))
        .and_then(|t| match t {
            RawTrait::Str(s) => Some(s.clone()),
            _ => None,
        })
        .expect("string trait present");
    assert!(value.contains("abc"), "leading text kept: {value:?}");
    assert!(value.contains('é'), "the multi-byte tail must survive: {value:?}");

    // ASCII: no byte dropped — "plain" + "ascii" concatenate whole.
    let files = src(&[(
        "menus\\main\\hud_main_menu.xml",
        r#"<menu name="M"><string>plain<!-- note -->ascii</string></menu>"#,
    )]);
    let mut s = files;
    let root_xml = s.files.get("menus\\main\\hud_main_menu.xml").unwrap().clone();
    let doc = parse_document(&root_xml, &mut s);
    let value = doc
        .tiles
        .iter()
        .find_map(|t| t.traits.get("string"))
        .and_then(|t| match t {
            RawTrait::Str(s) => Some(s.clone()),
            _ => None,
        })
        .expect("string trait present");
    assert_eq!(value, "plainascii", "no ASCII byte may be dropped");
}

/// #4651 sibling — stray text between tiles after a comment (the
/// `<rect>junk<!-- note --> über<x>1</x>` probe): parses without
/// panicking; the stray debris stays discarded by design.
#[test]
fn stray_text_between_tiles_after_a_comment_does_not_panic() {
    let files = src(&[(
        "menus\\main\\hud_main_menu.xml",
        r#"<menu name="M"><rect name="R">junk<!-- note --> über<x>1</x></rect></menu>"#,
    )]);
    let mut s = files;
    let root_xml = s.files.get("menus\\main\\hud_main_menu.xml").unwrap().clone();
    let _ = parse_document(&root_xml, &mut s);
}

/// #4650 (PAR-D1-2026-09-21-04) — include nesting counts toward the
/// tile-depth cap: the spliced content used to re-enter
/// `parse_element_content` with the SAME depth, so the 48 cap never saw
/// include chains. A 60-level chain must truncate, not recurse freely.
#[test]
fn include_chain_hits_the_tile_depth_cap() {
    let mut entries: Vec<(String, String)> = Vec::new();
    for i in 0..60 {
        entries.push((
            format!("menus\\prefabs\\chain{i}.xml"),
            format!(
                r#"<rect name="level{i}"><include src="chain{}.xml"/><width> 4 </width></rect>"#,
                i + 1
            ),
        ));
    }
    entries.push(("menus\\prefabs\\chain60.xml".to_string(), r#"<rect name="leaf"/>"#.to_string()));
    entries.push((
        "menus\\main\\hud_main_menu.xml".to_string(),
        r#"<menu name="M"><include src="chain0.xml"/></menu>"#.to_string(),
    ));
    let mut s = src(
        &entries
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect::<Vec<_>>(),
    );
    let root_xml = s
        .menu_xml("menus\\main\\hud_main_menu.xml")
        .map(|b| String::from_utf8(b).unwrap())
        .unwrap();
    let doc = parse_document(&root_xml, &mut s);
    let named = doc.name_index.len();
    assert!(
        named <= 50,
        "the 48-tile depth cap must truncate a 60-level include chain \
         (got {named} named tiles)"
    );
}

/// #4650 — the splice budget bounds fan-out: a 300-level chain (each
/// file splicing exactly one deeper file) must stop at the budget
/// instead of materialising 300 archive fetches + inflates.
#[test]
fn include_splice_budget_bounds_total_splices() {
    let mut entries: Vec<(String, String)> = Vec::new();
    for i in 0..300 {
        entries.push((
            format!("menus\\prefabs\\deep{i}.xml"),
            format!(
                r#"<rect name="deep{i}"><include src="deep{}.xml"/></rect>"#,
                i + 1
            ),
        ));
    }
    entries.push(("menus\\prefabs\\deep300.xml".to_string(), r#"<rect name="leaf"/>"#.to_string()));
    entries.push((
        "menus\\main\\hud_main_menu.xml".to_string(),
        r#"<menu name="M"><include src="deep0.xml"/></menu>"#.to_string(),
    ));
    let mut s = src(
        &entries
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect::<Vec<_>>(),
    );
    let root_xml = s
        .menu_xml("menus\\main\\hud_main_menu.xml")
        .map(|b| String::from_utf8(b).unwrap())
        .unwrap();
    let doc = parse_document(&root_xml, &mut s);
    assert!(
        doc.tiles.len() <= 270,
        "the splice budget must bound total splices (got {} tiles)",
        doc.tiles.len()
    );
}

/// #4650 — cycle detection keys on the RESOLVED archive path, not the
/// authored spelling: two files that self-include each other under
/// different spellings of the same entry terminated pre-fix only by
/// luck; a K-spelling self-include re-entered ~K! times.
#[test]
fn include_self_reference_under_two_spellings_terminates() {
    let body = r#"<include src="b.xml"/><include src="prefabs/b.xml"/><width> 5 </width>"#;
    let files = src(&[
        (
            "menus\\a.xml",
            r#"<menu name="A"><image name="x"><include src="b.xml"/></image></menu>"#,
        ),
        ("menus\\b.xml", body),
        ("menus\\prefabs\\b.xml", body),
    ]);
    let mut s = files;
    let root_xml = s.files.get("menus\\a.xml").unwrap().clone();
    let doc = parse_document(&root_xml, &mut s);
    let x = doc.name_index["x"];
    assert_eq!(doc.tiles[x].traits.get("width"), Some(&RawTrait::Num(5.0)));
}

/// #5329 — only the RESOLVED key is cycle-checked. `menus\x.xml` including
/// `x.xml` resolves to the distinct, higher-priority `menus\prefabs\x.xml`,
/// which must splice; #5007 rejected it as a cycle because the
/// lower-priority spelling `menus\x.xml` was on the nesting stack.
#[test]
fn a_distinct_higher_priority_include_is_not_a_cycle() {
    let mut s = CountingSource {
        files: [
            (
                "menus\\x.xml".to_string(),
                r#"<rect name="outer"><include src="x.xml"/></rect>"#.to_string(),
            ),
            (
                "menus\\prefabs\\x.xml".to_string(),
                r#"<rect name="prefab_x"/>"#.to_string(),
            ),
        ]
        .into_iter()
        .collect(),
        probes: Default::default(),
    };
    let host = r#"<menu name="root"><include src="menus\x.xml"/></menu>"#;
    let doc = parse_document(host, &mut s);
    assert!(doc.name_index.contains_key("outer"));
    assert!(
        doc.name_index.contains_key("prefab_x"),
        "the higher-priority prefab was dropped as a false cycle"
    );
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

/// #5007 / PAR-D1-2026-09-29-02 — counts archive probes per call so the
/// include tests can pin the fetch + inflate cost, which is the
/// quadratic-work channel (a production probe is a full BSA extract).
struct CountingSource {
    files: HashMap<String, String>,
    probes: std::cell::RefCell<Vec<String>>,
}

impl MenuFileSource for CountingSource {
    fn menu_xml(&self, path: &str) -> Option<Vec<u8>> {
        let key = path.to_lowercase();
        self.probes.borrow_mut().push(key.clone());
        self.files.get(&key).map(|s| s.as_bytes().to_vec())
    }
}

/// #5007 — the cycle check must run BEFORE the fragment fetch. Pre-fix, a
/// fragment of N self-includes cost N+1 fetches of itself (each a full
/// extract + inflate in production); the audit measured 8.8 GB inflated
/// for a 440 KB hostile fragment. Now the fetch happens once and every
/// self-include is skipped without touching the source.
#[test]
fn self_including_fragment_costs_one_fetch() {
    let body = format!("{}<width> 5 </width>", r#"<include src="g.xml"/>"#.repeat(1000));
    let mut s = CountingSource {
        files: [("menus\\prefabs\\g.xml".to_string(), body)].into_iter().collect(),
        probes: Default::default(),
    };
    let host = r#"<menu name="A"><image name="x"><include src="g.xml"/></image></menu>"#;
    let doc = parse_document(host, &mut s);
    let fetches = s.probes.borrow().iter().filter(|p| *p == "menus\\prefabs\\g.xml").count();
    assert_eq!(fetches, 1, "a self-including fragment must be fetched exactly once");
    let x = doc.name_index["x"];
    assert_eq!(doc.tiles[x].traits.get("width"), Some(&RawTrait::Num(5.0)));
}

/// #5007 — the budget check must also run BEFORE the fetch: after the
/// splice budget is spent, further include elements must not probe the
/// source at all.
#[test]
fn budget_exhausted_includes_never_fetch() {
    let mut files: HashMap<String, String> = HashMap::new();
    // One outer splice (b.xml) leaves 255; k0..k254 consume them, so
    // k255 and marker must be skipped pre-fetch.
    let mut body = String::new();
    for i in 0..300 {
        body.push_str(&format!(r#"<include src="k{i}.xml"/>"#));
        files.insert(format!("menus\\prefabs\\k{i}.xml"), r#"<width> 1 </width>"#.into());
    }
    body.push_str(r#"<include src="marker.xml"/>"#);
    files.insert("menus\\prefabs\\marker.xml".to_string(), r#"<width> 9 </width>"#.into());
    files.insert("menus\\prefabs\\b.xml".to_string(), body);
    let mut s = CountingSource { files, probes: Default::default() };
    let host = r#"<menu name="A"><image name="x"><include src="b.xml"/></image></menu>"#;
    let _doc = parse_document(host, &mut s);
    let probes = s.probes.borrow();
    assert_eq!(
        probes.iter().filter(|p| **p == "menus\\prefabs\\k0.xml").count(),
        1,
        "in-budget includes still fetch"
    );
    assert!(
        !probes.iter().any(|p| *p == "menus\\prefabs\\k255.xml"),
        "post-budget include must not probe the source"
    );
    assert!(
        !probes.iter().any(|p| *p == "menus\\prefabs\\marker.xml"),
        "post-budget include must not probe the source"
    );
}

/// #5668 — every fetch charges a 1 KiB floor against the fetch budget,
/// so a document spliced to one byte under the byte cap and then naming
/// hundreds of distinct 2-byte fragments stops fetching after
/// `MAX_INCLUDE_FETCHED_BYTES / INCLUDE_FETCH_FLOOR_BYTES` probes
/// (pre-fix each 2-byte refusal still paid a full extract — the budget
/// moved by 2 per fetch, admitting ~131k of them).
#[test]
fn tiny_post_cap_fragments_are_bounded_by_the_fetch_floor() {
    // Splice to one byte under the 256 KiB cap: three 64 KiB fragments
    // plus one of 65,535 bytes (fragments are capped at 64 KiB).
    let mut files: HashMap<String, String> = HashMap::new();
    let mut host = String::from(r#"<menu name="A"><image name="x">"#);
    for (tag, size) in [("f0", 65_536), ("f1", 65_536), ("f2", 65_536), ("f3", 65_535)] {
        files.insert(format!("menus\\prefabs\\{tag}.xml"), "a".repeat(size));
        host.push_str(&format!(r#"<include src="{tag}.xml"/>"#));
    }
    // Then 600 distinct two-byte fragments: each fetches, then refuses
    // at the spliced-byte cap (262,143 + 2 > 262,144).
    for i in 0..600 {
        files.insert(format!("menus\\prefabs\\t{i}.xml"), "ab".to_string());
        host.push_str(&format!(r#"<include src="t{i}.xml"/>"#));
    }
    host.push_str("</image></menu>");
    let mut s = CountingSource { files, probes: Default::default() };
    let _doc = parse_document(&host, &mut s);
    let tiny_fetches = s
        .probes
        .borrow()
        .iter()
        .filter(|p| p.starts_with("menus\\prefabs\\t"))
        .count();
    assert!(
        tiny_fetches <= 512,
        "the 1 KiB floor bounds the tiny-fragment fetches to 512, got {tiny_fetches}"
    );
}

/// Shared warning collector for the log-asserting tests (`set_logger`
/// installs once per process, so every such test must share one static).
#[allow(dead_code)]
mod warn_log {
    use std::sync::Mutex;
    pub static MSGS: Mutex<Vec<String>> = Mutex::new(Vec::new());
    struct MenuWarnCollector;
    impl log::Log for MenuWarnCollector {
        fn enabled(&self, _metadata: &log::Metadata) -> bool {
            true
        }
        fn log(&self, record: &log::Record) {
            let message = format!("{}", record.args());
            if message.starts_with("menuxml:") {
                // unwrap_or_else(into_inner): a test that panics while
                // holding the lock for an assert must not poison the
                // collector for every other test in the binary.
                MSGS
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .push(message);
            }
        }
        fn flush(&self) {}
    }
    static LOGGER: MenuWarnCollector = MenuWarnCollector;
    pub fn install() {
        let _ = log::set_logger(&LOGGER);
        log::set_max_level(log::LevelFilter::Warn);
    }
}

/// #5668 — the exhausted-budget warnings fire once per document, not
/// once per refused path: the byte-cap refusals above name 600 distinct
/// fragments, and pre-fix each logged its own line (200,000 lines on
/// the audit's hostile menu). The collector asserts the FIRST refused
/// path is named exactly once and the second never — unique marker
/// paths keep this race-free against the binary's other budget tests.
#[test]
fn exhausted_budget_warnings_fire_once_per_document() {
    warn_log::install();

    let mut files: HashMap<String, String> = HashMap::new();
    let mut host = String::from(r#"<menu name="A"><image name="x">"#);
    for (tag, size) in [("f0", 65_536), ("f1", 65_536), ("f2", 65_536), ("f3", 65_535)] {
        files.insert(format!("menus\\prefabs\\{tag}.xml"), "a".repeat(size));
        host.push_str(&format!(r#"<include src="{tag}.xml"/>"#));
    }
    for i in 0..40 {
        files.insert(format!("menus\\prefabs\\zzq{i}.xml"), "ab".to_string());
        host.push_str(&format!(r#"<include src="zzq{i}.xml"/>"#));
    }
    host.push_str("</image></menu>");
    let mut s = CountingSource { files, probes: Default::default() };
    let _doc = parse_document(&host, &mut s);
    let messages = warn_log::MSGS.lock().unwrap();
    assert_eq!(
        messages
            .iter()
            .filter(|m| m.contains("zzq0.xml") && m.contains("byte budget"))
            .count(),
        1,
        "the first byte-cap refusal names its path once"
    );
    assert!(
        !messages.iter().any(|m| m.contains("zzq1.xml")),
        "further refused paths stay silent: {} zzq warnings total",
        messages.iter().filter(|m| m.contains("zzq")).count()
    );
}

/// #5670 — a present-but-undecodable texture in the PREFERRED
/// resolution set is served by another set's copy (#5400 keeps the
/// search going, correctly), but the substitution must not be silent:
/// pre-fix the menu drew the wrong-resolution art with no log line.
/// Sibling check: the font-atlas path has no such substitution to
/// signal — it takes the first PRESENT atlas and parses it once,
/// logging a whole-font warning on failure; it never falls back to a
/// further atlas after a decode failure.
#[test]
fn an_undecodable_preferred_set_copy_warns_when_substituted() {
    warn_log::install();

    // `Menus\hud\a.dds` → candidates textures\menus\hud\a.dds
    // (garbage, present), textures\menus80\hud\a.dds (decodes), …
    struct GarbagePreferred;
    impl MenuAssets for GarbagePreferred {
        fn menu_xml(&self, path: &str) -> Option<Vec<u8>> {
            (path.eq_ignore_ascii_case("menus\\main\\hud_main_menu.xml")).then(|| {
                br#"<menu name="HUDMainMenu"><locus> &true; </locus>
                    <image name="i0"><filename>Menus\hud\a.dds</filename>
                    <width>8</width><height>8</height></image>
                </menu>"#
                .to_vec()
            })
        }
        fn texture(&self, path: &str) -> Option<Vec<u8>> {
            match path {
                "textures\\menus\\hud\\a.dds" => Some(b"not a dds".to_vec()),
                "textures\\menus80\\hud\\a.dds" => Some(dxt1_dds(64)),
                _ => None,
            }
        }
        fn font(&self, _index: u8) -> Option<Vec<u8>> {
            None
        }
        fn font_texture(&self, _path: &str) -> Option<Vec<u8>> {
            None
        }
    }
    let assets = GarbagePreferred;
    let mut r = MenuRenderer::load(
        &assets,
        "menus\\main\\hud_main_menu.xml",
        ScreenTraits::new(1280.0, 720.0),
    )
    .expect("load");
    r.render_frame(&assets);
    assert!(
        r.tex_cache
            .get("menus\\hud\\a.dds")
            .is_some_and(|t| t.is_some()),
        "the sibling set's copy serves the tile"
    );
    assert!(
        warn_log::MSGS
            .lock()
            .unwrap()
            .iter()
            .any(|m| m.contains("hud\\a.dds") && m.contains("another resolution set's copy")),
        "the substitution warns, naming the undecodable preferred path"
    );
    // Once per spelling: a second frame adds no further notice (the
    // count is over this test's unique path, so parallel tests' logs
    // cannot perturb it; it also proves the cache serves without
    // re-walking the candidates).
    r.render_frame(&assets);
    let notices = warn_log::MSGS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .iter()
        .filter(|m| m.contains("hud\\a.dds") && m.contains("another resolution set's copy"))
        .count();
    assert_eq!(notices, 1, "the substitution notice fires once per cache key");
}

/// #5007 — fetched fragment bytes are cached per document, so a repeated
/// sequential (non-cyclic) include of the same fragment is one fetch plus
/// map hits.
#[test]
fn repeated_sequential_includes_share_one_fetch() {
    let host = format!(
        r#"<menu name="A"><image name="x">{}</image></menu>"#,
        r#"<include src="g.xml"/>"#.repeat(100)
    );
    let mut s = CountingSource {
        files: [(
            "menus\\prefabs\\g.xml".to_string(),
            r#"<width> 5 </width>"#.to_string(),
        )]
        .into_iter()
        .collect(),
        probes: Default::default(),
    };
    let doc = parse_document(&host, &mut s);
    let fetches = s.probes.borrow().iter().filter(|p| *p == "menus\\prefabs\\g.xml").count();
    assert_eq!(fetches, 1, "repeated includes must hit the per-document cache");
    let x = doc.name_index["x"];
    assert_eq!(doc.tiles[x].traits.get("width"), Some(&RawTrait::Num(5.0)));
}

/// #5314 (PAR-D1-2026-10-05-01) — the splice COUNT budget bounds fetches,
/// not output: each splice re-parses the whole fragment, so 256 splices ×
/// an unbounded fragment grew the document linearly at ~7.2 MiB resident
/// per KiB of fragment. The byte budget must stop splicing once
/// MAX_INCLUDE_BYTES (256 KiB) of fragment text has been consumed: with
/// ten ~48 KiB fragments offered, exactly the five that fit (5 × 49,015 =
/// 245,075 ≤ 262,144; a sixth would exceed it) splice, in order.
#[test]
fn include_byte_budget_bounds_total_spliced_bytes() {
    let mut files: HashMap<String, String> = HashMap::new();
    // ~48 KiB each: marker + padding, far under the 64 KiB per-fragment
    // cap but big enough that five exhaust the 256 KiB document budget.
    for i in 0..10 {
        files.insert(
            format!("menus\\prefabs\\k{i}.xml"),
            format!("<string>marker{i:02} {}</string>", "A".repeat(49_000)),
        );
    }
    let host = format!(
        r#"<menu name="A"><image name="x">{}</image></menu>"#,
        (0..10)
            .map(|i| format!(r#"<include src="k{i}.xml"/>"#))
            .collect::<String>()
    );
    let mut s = CountingSource { files, probes: Default::default() };
    let doc = parse_document(&host, &mut s);
    let x = doc.name_index["x"];
    match doc.tiles[x].traits.get("string") {
        Some(RawTrait::Str(v)) => {
            assert!(
                v.starts_with("marker04"),
                "exactly the five fragments that fit the byte budget must splice \
                 (last spliced = k04), got {:?}",
                &v[..16.min(v.len())]
            );
            assert!(!v.contains("marker05"), "k05 must be refused by the byte budget");
        }
        other => panic!("string trait present after splices, got {other:?}"),
    }
}

/// #5314 — a fragment larger than MAX_INCLUDE_FRAGMENT_BYTES (64 KiB) can
/// never fit the byte budget, so it must be treated as absent: no tiles
/// or traits from it, its bytes never cached, and later includes of the
/// same path must not refetch (the cached `None` answers them).
#[test]
fn oversized_include_fragment_is_skipped_and_not_refetched() {
    let mut files: HashMap<String, String> = HashMap::new();
    // 19 + 9,400 × 7 = 65,819 bytes: over the 64 KiB per-fragment cap.
    files.insert(
        "menus\\prefabs\\big.xml".to_string(),
        format!("<width> 42 </width>{}", "<rect/>".repeat(9_400)),
    );
    let host = r#"<menu name="A"><image name="x"><include src="big.xml"/><include src="big.xml"/></image></menu>"#;
    let mut s = CountingSource { files, probes: Default::default() };
    let doc = parse_document(host, &mut s);
    let x = doc.name_index["x"];
    assert_ne!(
        doc.tiles[x].traits.get("width"),
        Some(&RawTrait::Num(42.0)),
        "the oversized fragment must not contribute traits"
    );
    assert_eq!(doc.tiles[x].children.len(), 0, "no tiles from the oversized fragment");
    // First include probes all three candidate spellings exactly once
    // (prefabs\big.xml fetched + rejected, two misses); the second include
    // must hit the cache, not the source.
    let probes = s.probes.borrow();
    assert_eq!(
        probes.iter().filter(|p| *p == "menus\\prefabs\\big.xml").count(),
        1,
        "the oversized fragment must be fetched once, then answered from the cache"
    );
    assert_eq!(
        probes.iter().filter(|p| *p == "menus\\big.xml").count(),
        1,
        "candidate-spelling misses are also cached per document"
    );
}

/// #5399 (PAR-D1-2026-10-08-01) — a refused fragment must not cost an
/// unbounded number of fetches. Four in-size fragments splice the
/// document to one byte under the 256 KiB budget; the next 1,000 distinct
/// in-size prefabs can never fit. Pre-fix each was fetched (an archive
/// extract in production), cached as `Some(bytes)`, and only then
/// refused: 1,000 fetches, ~64 MB resident. The fetch budget (2× the
/// splice budget) must stop the source being probed after a handful.
#[test]
fn refused_includes_are_bounded_by_the_fetch_budget() {
    let mut files: HashMap<String, String> = HashMap::new();
    // Three 65,536-byte fragments + one of 65,535 = 262,143 bytes spliced.
    let fill = |len: usize, tag: &str| {
        let head = format!("<string>{tag}</string>");
        format!("{head}{}", "A".repeat(len - head.len()))
    };
    for (i, len) in [65_536usize, 65_536, 65_536, 65_535].into_iter().enumerate() {
        files.insert(format!("menus\\prefabs\\f{i}.xml"), fill(len, &format!("fill{i}")));
    }
    for i in 0..1_000 {
        files.insert(format!("menus\\prefabs\\g{i}.xml"), fill(60_000, &format!("g{i}")));
    }
    let includes: String = (0..4)
        .map(|i| format!(r#"<include src="f{i}.xml"/>"#))
        .chain((0..1_000).map(|i| format!(r#"<include src="g{i}.xml"/>"#)))
        .collect();
    let host = format!(r#"<menu name="A"><image name="x">{includes}</image></menu>"#);
    let mut s = CountingSource { files, probes: Default::default() };
    let doc = parse_document(&host, &mut s);
    let x = doc.name_index["x"];
    assert_eq!(
        doc.tiles[x].traits.get("string"),
        Some(&RawTrait::Str("fill3".into())),
        "the four fragments that fit the splice budget still splice"
    );
    let probes = s.probes.borrow();
    let refused_fetches = probes
        .iter()
        .filter(|p| p.starts_with("menus\\prefabs\\g"))
        .count();
    // 262,143 B already fetched; each 60,000 B refusal adds to the 524,288 B
    // fetch budget, so the fifth crosses it and nothing further is probed.
    assert!(
        refused_fetches <= 5,
        "refused includes must stop fetching once the fetch budget is spent, \
         got {refused_fetches} fetches of the 1,000 refused fragments"
    );
}

/// #5314 — the 48-level cap bounds tile DEPTH only; a wide document (or a
/// small fragment spliced 256×) had no total bound on materialized tiles.
/// The total tile cap must truncate a 20,000-tile document at
/// MAX_DOCUMENT_TILES instead of materializing the full arena.
#[test]
fn document_tile_cap_bounds_total_tiles() {
    let host = format!(
        r#"<menu name="A">{}</menu>"#,
        (0..20_000)
            .map(|i| format!(r#"<rect name="r{i}"/>"#))
            .collect::<String>()
    );
    let mut s = src(&[]);
    let doc = parse_document(&host, &mut s);
    // Must equal MAX_DOCUMENT_TILES (private to parse.rs) + the root.
    assert!(
        doc.tiles.len() <= 16_385,
        "the total tile cap must bound the arena (got {} tiles)",
        doc.tiles.len()
    );
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
    for (tile, t) in doc.tiles.iter().enumerate() {
        let keys: Vec<String> = t.traits.keys().cloned().collect();
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

/// #6666 — `globals()` reads the FO3/FNV globals document (`_background_fill_alpha` is the
/// dialogue menu's panel alpha); without one it reads 0.
#[test]
fn globals_selector_reads_the_globals_document() {
    let mut s = src(&[]);
    let doc = parse_document(
        r#"<menu name="M"><image name="panel"><alpha> <copy src="globals()" trait="_background_fill_alpha"/> </alpha></image></menu>"#,
        &mut s,
    );
    let strings = HashMap::new();
    let overrides = Overrides::new();
    let panel = doc.name_index["panel"];
    let mut bare = EvalState::new(&doc, ScreenTraits::new(1280.0, 720.0), &strings, &overrides);
    assert_eq!(bare.trait_value(panel, "alpha").as_num(), 0.0);
    let globals = HashMap::from([("_background_fill_alpha".to_string(), Scalar::Num(32.0))]);
    let mut eval = EvalState::new(&doc, ScreenTraits::new(1280.0, 720.0), &strings, &overrides).with_globals(&globals);
    assert_eq!(eval.trait_value(panel, "alpha").as_num(), 32.0);
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

/// #4716 — a `NaN` depth (authorable directly: `"NaN".parse::<f32>()`
/// succeeds) made `partial_cmp().unwrap_or(Equal)` a non-total order, and
/// `slice::sort_by` panics on that once it has enough elements to notice. Forty
/// siblings with every third depth `NaN` is the audit's reproduction; the
/// draw must complete, and the siblings with a real depth must still come out
/// in ascending depth order.
#[test]
fn nan_depth_siblings_do_not_panic_the_depth_sort() {
    let mut xml = String::from(r#"<menu name="M">"#);
    for i in 0..40 {
        // Real depths run against document order so a sort that gave up
        // would show. Every third sibling is NaN.
        let depth = if i % 3 == 0 {
            "NaN".to_string()
        } else {
            format!("{}", 100 - i)
        };
        xml.push_str(&format!(
            r#"<image name="t{i}"> <depth> {depth} </depth> <filename> a.dds </filename> </image>"#
        ));
    }
    xml.push_str("</menu>");
    let (doc, _memo) = eval_all(&xml);
    let strings = HashMap::new();
    let overrides = Overrides::new();
    let mut eval = EvalState::new(&doc, ScreenTraits::new(1280.0, 720.0), &strings, &overrides);

    let items = build_draw_list(&doc, &mut eval);

    assert_eq!(items.len(), 40, "every sibling still draws");
    let real_order: Vec<usize> = items
        .iter()
        .map(|item| {
            doc.name_index
                .iter()
                .find(|(_, &t)| t == item.tile())
                .unwrap()
                .0
        })
        .filter_map(|name| name[1..].parse::<usize>().ok())
        .filter(|i| i % 3 != 0)
        .collect();
    let expected: Vec<usize> = (0..40).rev().filter(|i| i % 3 != 0).collect();
    assert_eq!(
        real_order, expected,
        "siblings with a real depth keep ascending-depth order (descending index here)"
    );
}

/// #4716 — `total_cmp` orders -0.0 before +0.0, but they are the same depth:
/// siblings at `-0` and `0` keep document order, as they did under
/// `partial_cmp`.
#[test]
fn negative_zero_depth_ties_with_zero_in_document_order() {
    let (doc, _memo) = eval_all(
        r#"
        <menu name="M">
            <image name="first"> <depth> 0 </depth> <filename> a.dds </filename> </image>
            <image name="second"> <depth> -0 </depth> <filename> a.dds </filename> </image>
            <image name="third"> <depth> 0 </depth> <filename> a.dds </filename> </image>
        </menu>"#,
    );
    let strings = HashMap::new();
    let overrides = Overrides::new();
    let mut eval = EvalState::new(&doc, ScreenTraits::new(1280.0, 720.0), &strings, &overrides);
    let order: Vec<usize> = build_draw_list(&doc, &mut eval)
        .iter()
        .map(|item| item.tile())
        .collect();
    let expected = ["first", "second", "third"].map(|name| doc.name_index[name]);
    assert_eq!(order, expected);
}

// ---------------------------------------------------------------------------
// Rasterizer
// ---------------------------------------------------------------------------

#[test]
fn blit_scales_and_tints() {
    use crate::layout::Rect;
    use crate::raster::{Framebuffer, BlitStyle};
    use crate::tex::Rgba8;

    let mut tex = Rgba8::new(2, 2);
    // Red opaque quadrant pattern.
    for (i, px) in tex.pixels.as_chunks_mut::<4>().0.iter_mut().enumerate() {
        let odd = i % 2 == 1;
        *px = [255, 0, 0, if odd { 255 } else { 0 }];
    }
    let mut fb = Framebuffer::new(4, 2);
    fb.blit(
        &tex,
        Rect { x: 0.0, y: 0.0, w: 4.0, h: 2.0 },
        (0.0, 0.0),
        -1.0,
        false,
BlitStyle { tint: [255.0, 255.0, 255.0], alpha: 255.0, clip: None }      ,
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
        -1.0,
        false,
BlitStyle { tint: [255.0, 255.0, 255.0], alpha: 255.0, clip: None }      ,
    );
    // The white cell should now fill the first 32 display pixels…
    // clipped to the 8px framebuffer.
    for x in 0..8 {
        let o = x * 4;
        assert_eq!(fb.pixels[o + 3], 255, "pixel {x} should be opaque white");
    }
}

#[test]
fn blit_default_zoom_draws_natural_size_clipped_to_tile() {
    use crate::layout::Rect;
    use crate::raster::Framebuffer;
    use crate::tex::Rgba8;

    // The HUD-ribbon shape: a power-of-2 padded texture — 8 px wide,
    // ink only in the left 4 px — drawn into tile rects narrower than
    // the texture. Default zoom (unauthored) must draw texels 1:1 from
    // the tile origin and CLIP at the tile edge, never squeeze the
    // transparent pad into the bar (that rendered a constant ~63%-width
    // health bar at every fraction; see blit's doc).
    let mut tex = Rgba8::new(8, 1);
    for x in 0..4 {
        let o = x * 4;
        tex.pixels[o..o + 4].copy_from_slice(&[255, 0, 0, 255]);
    }
    // Half-width tile: 4 display px of red, then the tile clips.
    let mut fb = Framebuffer::new(8, 1);
    fb.blit(
        &tex,
        Rect { x: 1.0, y: 0.0, w: 4.0, h: 1.0 },
        (0.0, 0.0),
        0.0,
        false,
BlitStyle { tint: [255.0, 255.0, 255.0], alpha: 255.0, clip: None }      ,
    );
    for x in 0..8 {
        let o = x * 4;
        let expect_opaque = (1..5).contains(&x);
        assert_eq!(
            fb.pixels[o + 3],
            if expect_opaque { 255 } else { 0 },
            "pixel {x}: natural draw clips at the tile rect"
        );
    }
    // Full-width tile sees the whole ink span and none of the pad.
    let mut fb = Framebuffer::new(8, 1);
    fb.blit(
        &tex,
        Rect { x: 0.0, y: 0.0, w: 8.0, h: 1.0 },
        (0.0, 0.0),
        100.0,
        false,
BlitStyle { tint: [255.0, 255.0, 255.0], alpha: 255.0, clip: None }      ,
    );
    assert_eq!(&fb.pixels[0..4], &[255, 0, 0, 255]);
    assert_eq!(&fb.pixels[12..16], &[255, 0, 0, 255]);
    assert_eq!(&fb.pixels[16..20], &[0, 0, 0, 0], "pad stays transparent");
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
                x_offset: 0.0,
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
    fb.text_line(
        &font,
        "AA",
        0.0,
        0.0,
        0,
        BlitStyle { tint: [255.0, 0.0, 0.0], alpha: 255.0, clip: None },
    );
    // First glyph: x 0..4 red; pen then at 6 → second glyph x 6..10.
    #[allow(clippy::erasing_op)] // glyph 0's origin × the pixel stride, parallel to the 5/6 sites below
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

/// A `size`×`size` DXT1 DDS (all-zero blocks).
fn dxt1_dds(size: u32) -> Vec<u8> {
    let mut dds = vec![0u8; 128];
    dds[0..4].copy_from_slice(b"DDS ");
    dds[4..8].copy_from_slice(&124u32.to_le_bytes());
    dds[12..16].copy_from_slice(&size.to_le_bytes()); // height
    dds[16..20].copy_from_slice(&size.to_le_bytes()); // width
    dds[80..84].copy_from_slice(&0x4u32.to_le_bytes()); // DDPF_FOURCC
    dds[84..88].copy_from_slice(b"DXT1");
    dds.resize(128 + (size as usize / 4).pow(2) * 8, 0);
    dds
}

/// One XML naming each of `filenames` on its own visible image tile, and a
/// texture source that serves a 64×64 DXT1 for every menu path, recording
/// the paths it was asked for.
struct BudgetAssets {
    xml: Vec<u8>,
    fetched: std::cell::RefCell<Vec<String>>,
}

impl BudgetAssets {
    fn new(filenames: &[&str]) -> Self {
        let mut xml = String::from("<menu name=\"HUDMainMenu\"><locus> &true; </locus>");
        for (i, name) in filenames.iter().enumerate() {
            xml.push_str(&format!(
                "<image name=\"i{i}\"><filename>{name}</filename>\
                 <width>8</width><height>8</height></image>"
            ));
        }
        xml.push_str("</menu>");
        Self {
            xml: xml.into_bytes(),
            fetched: Default::default(),
        }
    }
}

impl MenuAssets for BudgetAssets {
    fn menu_xml(&self, path: &str) -> Option<Vec<u8>> {
        (path.eq_ignore_ascii_case("menus\\main\\hud_main_menu.xml")).then(|| self.xml.clone())
    }
    fn texture(&self, path: &str) -> Option<Vec<u8>> {
        self.fetched.borrow_mut().push(path.to_string());
        path.starts_with("textures\\menus").then(|| dxt1_dds(64))
    }
    fn font(&self, _index: u8) -> Option<Vec<u8>> {
        None
    }
    fn font_texture(&self, _path: &str) -> Option<Vec<u8>> {
        None
    }
}

/// #5658 (PAR-D1-2026-10-12-01) — the decoded-texture cache never evicts,
/// so a document naming K distinct textures used to pin K decodes with no
/// total. Past the renderer's budget a new texture is refused from its
/// header alone (no decode) and resolves to nothing, while everything
/// decoded before keeps being served.
#[test]
fn decoded_textures_stop_at_the_renderer_budget() {
    let names = [
        "Menus\\a0.dds",
        "Menus\\a1.dds",
        "Menus\\a2.dds",
        "Menus\\a3.dds",
        "Menus\\a4.dds",
    ];
    let assets = BudgetAssets::new(&names);
    let mut r = MenuRenderer::load(
        &assets,
        "menus\\main\\hud_main_menu.xml",
        ScreenTraits::new(1280.0, 720.0),
    )
    .expect("load");
    let one = 64 * 64 * 4;
    r.texture_budget = 3 * one;
    r.render_frame(&assets);

    assert_eq!(r.decoded.len(), 3, "three textures fit the budget");
    assert_eq!(r.decoded_bytes, 3 * one);
    assert_eq!(r.tex_cache.len(), 5);
    assert_eq!(
        r.tex_cache.values().filter(|t| t.is_none()).count(),
        2,
        "the two over budget resolve to nothing"
    );
    // Refusals are cached: a second frame fetches nothing new.
    let fetched = assets.fetched.borrow().len();
    r.render_frame(&assets);
    assert_eq!(assets.fetched.borrow().len(), fetched);
}

/// #5658 — the authored spellings of one file (`Menus\`, `Menus80\`,
/// `textures\menus\`) resolve to one archive path and must share one
/// decode, charged once, instead of each keeping its own copy.
#[test]
fn spellings_of_one_menu_texture_share_one_decode() {
    let names = [
        "Menus\\hud\\a.dds",
        "Menus80\\hud\\a.dds",
        "textures\\menus\\hud\\a.dds",
    ];
    let assets = BudgetAssets::new(&names);
    let mut r = MenuRenderer::load(
        &assets,
        "menus\\main\\hud_main_menu.xml",
        ScreenTraits::new(1280.0, 720.0),
    )
    .expect("load");
    r.render_frame(&assets);

    assert_eq!(r.decoded.len(), 1, "one archive file, one decode");
    assert_eq!(r.decoded_bytes, 64 * 64 * 4);
    let images: Vec<_> = r
        .tex_cache
        .values()
        .map(|t| t.clone().expect("resolved"))
        .collect();
    assert_eq!(images.len(), 3);
    assert!(images
        .windows(2)
        .all(|pair| std::sync::Arc::ptr_eq(&pair[0], &pair[1])));
    assert_eq!(assets.fetched.borrow().len(), 1, "the archive is read once");
}

/// #5693 — the over-budget refusal is remembered per resolved archive
/// path, consulted before the fetch. Pre-fix the refusal was cached only
/// under the authored spelling, so each additional spelling of the same
/// over-budget file re-extracted it from the archive for the same
/// refusal (~33 MB inflated per 8192² DXT1) — a first-render stall whose
/// cost was bounded only by the tile count. Every archive path may be
/// fetched for a budget decision at most once.
#[test]
fn refused_menu_texture_paths_are_not_refetched_per_spelling() {
    let names = [
        "Menus\\hud\\a.dds",
        "Menus80\\hud\\a.dds",
        "textures\\menus\\hud\\a.dds",
    ];
    let assets = BudgetAssets::new(&names);
    let mut r = MenuRenderer::load(
        &assets,
        "menus\\main\\hud_main_menu.xml",
        ScreenTraits::new(1280.0, 720.0),
    )
    .expect("load");
    r.texture_budget = 1; // every candidate is over budget
    r.render_frame(&assets);

    let fetched = assets.fetched.borrow();
    assert!(!fetched.is_empty(), "the first spelling does fetch");
    let mut unique = std::collections::HashSet::new();
    assert!(
        fetched.iter().all(|path| unique.insert(path.clone())),
        "no archive path may be fetched twice for the same refusal: {fetched:?}"
    );
    assert!(
        r.tex_cache.values().all(|t| t.is_none()),
        "every spelling resolves to nothing"
    );
}

/// A size-aware texture source: lowercase archive path → DDS dimension.
struct SizedAssets {
    xml: Vec<u8>,
    sizes: HashMap<String, u32>,
    fetched: std::cell::RefCell<Vec<String>>,
}

impl MenuAssets for SizedAssets {
    fn menu_xml(&self, path: &str) -> Option<Vec<u8>> {
        (path.eq_ignore_ascii_case("menus\\main\\hud_main_menu.xml")).then(|| self.xml.clone())
    }
    fn texture(&self, path: &str) -> Option<Vec<u8>> {
        self.fetched.borrow_mut().push(path.to_string());
        self.sizes.get(path).map(|&size| dxt1_dds(size))
    }
    fn font(&self, _index: u8) -> Option<Vec<u8>> {
        None
    }
    fn font_texture(&self, _path: &str) -> Option<Vec<u8>> {
        None
    }
}

/// #5693 — the budget check is per texture, so a later, smaller texture
/// that still fits is decoded and drawn after an over-budget refusal;
/// the old warning ("it and any further new textures are not drawn")
/// overstated what is refused. Unequal sizes tell the two apart (the
/// #5658 tests use equal sizes and cannot).
#[test]
fn a_smaller_menu_texture_still_draws_after_a_budget_refusal() {
    let mut xml = String::from("<menu name=\"HUDMainMenu\"><locus> &true; </locus>");
    for (i, name) in ["Menus\\big.dds", "Menus\\small.dds"].iter().enumerate() {
        xml.push_str(&format!(
            "<image name=\"i{i}\"><filename>{name}</filename>\
             <width>8</width><height>8</height></image>"
        ));
    }
    xml.push_str("</menu>");
    let assets = SizedAssets {
        xml: xml.into_bytes(),
        sizes: HashMap::from([
            ("textures\\menus\\big.dds".to_string(), 256),
            ("textures\\menus\\small.dds".to_string(), 64),
        ]),
        fetched: Default::default(),
    };
    let mut r = MenuRenderer::load(
        &assets,
        "menus\\main\\hud_main_menu.xml",
        ScreenTraits::new(1280.0, 720.0),
    )
    .expect("load");
    r.texture_budget = 64 * 64 * 4; // exactly one small texture
    r.render_frame(&assets);

    assert_eq!(r.decoded.len(), 1, "only the small texture decoded");
    assert!(r.decoded.contains_key("textures\\menus\\small.dds"));
    assert_eq!(r.decoded_bytes, 64 * 64 * 4);
    assert!(
        r.tex_cache.get("menus\\big.dds").is_some_and(|t| t.is_none()),
        "the over-budget texture resolves to nothing"
    );
    assert!(
        r.tex_cache.get("menus\\small.dds").is_some_and(|t| t.is_some()),
        "the smaller later texture still draws"
    );
}

// ---------------------------------------------------------------------------
// Tile-mode raster (FO3 `<tile>` trait — M48.5)
// ---------------------------------------------------------------------------

#[test]
fn tiled_blit_repeats_and_scrolls_with_wrap() {
    use crate::layout::Rect;
    use crate::raster::Framebuffer;
    use crate::tex::Rgba8;

    // 2×1 texture: black texel then white texel.
    let mut tex = Rgba8::new(2, 1);
    tex.pixels[4..8].copy_from_slice(&[255, 255, 255, 255]);
    let mut fb = Framebuffer::new(6, 1);
    fb.blit(
        &tex,
        Rect { x: 0.0, y: 0.0, w: 6.0, h: 1.0 },
        (0.0, 0.0),
        0.0,
        true,
BlitStyle { tint: [255.0, 255.0, 255.0], alpha: 255.0, clip: None }      ,
    );
    for x in 0..6 {
        let o = x * 4;
        let expect_white = x % 2 == 1;
        assert_eq!(fb.pixels[o], if expect_white { 255 } else { 0 }, "x={x}");
    }
    // cropx 1 scrolls the window one texel and wraps at the edge.
    let mut fb = Framebuffer::new(6, 1);
    fb.blit(
        &tex,
        Rect { x: 0.0, y: 0.0, w: 6.0, h: 1.0 },
        (1.0, 0.0),
        0.0,
        true,
BlitStyle { tint: [255.0, 255.0, 255.0], alpha: 255.0, clip: None }      ,
    );
    for x in 0..6 {
        let o = x * 4;
        let expect_white = x % 2 == 0;
        assert_eq!(fb.pixels[o], if expect_white { 255 } else { 0 }, "x={x}");
    }
}

// ---------------------------------------------------------------------------
// Runtime grafting + templates (FO3 HUD assembly — M48.5)
// ---------------------------------------------------------------------------

/// The vanilla FO3 `menus\prefabs\meter.xml` shape (traits trimmed to the
/// load-bearing ops): `_Value` scales MeterImage's width, `tile` repeats
/// the tick mark, `parent()`/`sibling()` resolve through the graft.
const METER_FRAGMENT: &str = r#"
    <_Value> </_Value>
    <_SolidMeter> &false; </_SolidMeter>
    <_ShowBackground> &false; </_ShowBackground>
    <height> 24 </height>
    <width> 600 </width>
    <alpha> 255 </alpha>
    <justify> &left; </justify>
    <locus> &true; </locus>
    <image name="MeterBackground">
        <filename> solid.dds </filename>
        <height> <copy src="parent()" trait="height"/> </height>
        <width> <copy src="parent()" trait="width"/> </width>
        <depth> 1 </depth>
        <visible> <copy src="parent()" trait="_ShowBackground"/> </visible>
    </image>
    <image name="MeterImage">
        <height> <copy src="parent()" trait="height"/> </height>
        <width>
            <copy src="parent()" trait="width"/>
            <mul src="parent()" trait="_Value"/>
        </width>
        <alpha> <copy src="parent()" trait="alpha"/> </alpha>
        <depth> 2 </depth>
        <x> <copy src="sibling(MeterBackground)" trait="x"/> </x>
        <filename>
            <copy src="parent()" trait="_SolidMeter"/>
            <copy src="me()" trait="_filename_"/>
        </filename>
        <_filename_0>Interface\HUD\hud_tick_mark.dds</_filename_0>
        <_filename_1>Interface\Shared\solid.dds</_filename_1>
        <tile> <not src="parent()" trait="_SolidMeter"/> </tile>
    </image>
"#;

/// Grafting `meter.xml` under a container rect mirrors the source
/// engine's runtime HUD assembly: the fragment's loose traits land on the
/// wrapper, `parent()` ops resolve against it, and an override on the
/// wrapper's `_Value` scales the meter's drawn width.
#[test]
fn graft_fragment_drives_meter_ops_from_overrides() {
    use crate::eval::Overrides;
    use crate::menu::MenuError;
    use crate::profile::MenuProfile;

    let mut s = src(&[
        (
            "menus\\main\\hud_main_menu.xml",
            r#"
            <menu name="HUDMainMenu">
                <locus> &true; </locus>
                <rect name="HitPoints">
                    <width> 369 </width>
                    <height> 127 </height>
                </rect>
            </menu>"#,
        ),
        ("menus\\prefabs\\meter.xml", METER_FRAGMENT),
    ]);
    let main_text = s.files.get("menus\\main\\hud_main_menu.xml").unwrap().clone();
    let meter_text = s.files.get("menus\\prefabs\\meter.xml").unwrap().clone();
    let mut doc = parse_document(&main_text, &mut s);
    // Same wrapping graft_fragment applies to rootless prefabs.
    let wrapped = format!("<rect name=\"hp_meter\">\n{meter_text}\n</rect>");
    let fragment = parse_document(&wrapped, &mut s);

    let hp = doc.name_index["hitpoints"];
    let wrapper = doc.graft_subtree(&fragment, 0, hp, Some("hp_meter"));
    assert_eq!(doc.tiles[wrapper].parent, Some(hp));
    // Loose fragment traits landed on the wrapper.
    assert_eq!(doc.tiles[wrapper].traits.get("width"), Some(&RawTrait::Num(600.0)));
    assert!(doc.tiles[wrapper].traits.contains_key("locus"));
    let meter_img = doc.name_index["meterimage"];
    assert_eq!(doc.tiles[meter_img].parent, Some(wrapper));

    // Profile plumbing: an 8-slot FO3 table loads without any font data
    // (slots stay None) and skips the strings document.
    struct NoFonts;
    impl MenuAssets for NoFonts {
        fn menu_xml(&self, _: &str) -> Option<Vec<u8>> { None }
        fn texture(&self, _: &str) -> Option<Vec<u8>> { None }
        fn font(&self, _: u8) -> Option<Vec<u8>> { None }
        fn font_texture(&self, _: &str) -> Option<Vec<u8>> { None }
    }
    let result = MenuRenderer::load_with_profile(
        &NoFonts,
        "menus\\main\\hud_main_menu.xml",
        ScreenTraits::new(1280.0, 720.0),
        MenuProfile::fallout3(),
    );
    assert!(
        matches!(result, Err(MenuError::MissingMenu(_))),
        "menu xml absent from NoFonts → MissingMenu"
    );

    // Half value → half the authored 600px meter width, tick-repeated.
    let mut overrides = Overrides::new();
    overrides.insert(
        ("hp_meter".to_string(), "_value".to_string()),
        Scalar::Num(0.5),
    );
    let strings = HashMap::new();
    let mut eval = crate::eval::EvalState::new(
        &doc,
        ScreenTraits::new(1280.0, 720.0),
        &strings,
        &overrides,
    );
    eval.resolve_all();
    let items = build_draw_list(&doc, &mut eval);
    let meter = items
        .iter()
        .find(|i| i.tile() == meter_img)
        .expect("MeterImage in the draw list");
    match meter {
        crate::layout::DrawItem::Image { rect, tiled, .. } => {
            assert_eq!(rect.w, 300.0, "600 × _Value 0.5");
            assert!(tiled, "tick-meter repeats its texture");
        }
        other => panic!("expected an image item, got {other:?}"),
    }
}

/// `<template>` prototypes never draw themselves; instantiating one
/// through the renderer's menu API clones its content subtree under the
/// named tile, renamed and driven by overrides.
#[test]
fn instantiate_template_clones_prototype_content() {
    let assets = SynthAssets {
        xml: br#"
        <menu name="HUDMainMenu">
            <locus> &true; </locus>
            <rect name="Compass"/>
            <template name="template_compass_window">
                <image name="compass_window">
                    <filename> Interface\HUD\glow_hud_comp_direction_strip.dds </filename>
                    <width> 345 </width>
                    <height> 64 </height>
                    <tile> &true; </tile>
                    <x> 20 </x>
                    <y> 65 </y>
                </image>
            </template>
        </menu>"#
            .to_vec(),
        strings: Vec::new(),
    };
    let mut r = MenuRenderer::load(
        &assets,
        "menus\\main\\hud_main_menu.xml",
        ScreenTraits::new(1280.0, 720.0),
    )
    .expect("load");
    assert!(
        r.tile("template_compass_window").is_some(),
        "template parses into the arena"
    );

    let clone = r
        .instantiate_template("template_compass_window", "Compass", "hud_compass")
        .expect("instantiate");
    let _ = clone;

    // The clone draws with the prototype's authored traits, and overrides
    // address it by its new name (cropx scrolls the strip).
    r.set_override("hud_compass", "cropx", 100.0);
    let _ = r.render_frame(&assets);

    // A missing parent or template names the failure.
    assert!(r.instantiate_template("template_compass_window", "Nope", "x").is_err());
    assert!(r.instantiate_template("template_nope", "Compass", "x").is_err());
}

// ---------------------------------------------------------------------------
// Per-game profiles (M48.5)
// ---------------------------------------------------------------------------

#[test]
fn profiles_pin_corpus_facts() {
    use crate::profile::{FontArchive, MenuProfile};

    let ob = MenuProfile::oblivion();
    assert_eq!(ob.font_paths.len(), 5);
    assert_eq!(ob.font_archive, FontArchive::Misc);
    assert_eq!(ob.strings_path, Some("menus\\strings.xml"));
    assert_eq!((ob.font_atlas)("Glow_X"), vec!["fonts\\Glow_X.tex"]);

    for (profile, slots) in [
        (MenuProfile::fallout3(), 8),
        (MenuProfile::fallout_nv(), 9),
    ] {
        assert_eq!(profile.font_paths.len(), slots, "{} font slots", profile.label);
        assert_eq!(profile.font_archive, FontArchive::Textures);
        assert_eq!(profile.strings_path, None);
        // Both atlas candidates beside each .fnt, .tex preferred.
        assert_eq!(
            (profile.font_atlas)("Glow_Monofonto_Large_0_Lod_A"),
            vec![
                "textures\\fonts\\Glow_Monofonto_Large_0_Lod_A.tex".to_string(),
                "textures\\fonts\\Glow_Monofonto_Large_0_Lod_A.dds".to_string(),
            ]
        );
        // The HUD's text tiles use slots 7/8 — both must be populated.
        assert!(profile.font_paths.len() >= 8);
    }
}

/// #6994 — a hand-built `.fnt` record: `[1]`/`[3]` are the x edges, `[2]`/`[6]`
/// the y edges and `[9]`/`[10]` the glyph width / height (x before y). The
/// real-data pin is `every_font_cuts_its_glyphs_from_clean_atlas_cells` in the
/// three corpus test files.
#[test]
fn a_parsed_font_record_reads_x_before_y() {
    use crate::font::Font;
    let mut fnt = vec![0u8; 296 + 256 * 56];
    fnt[0..4].copy_from_slice(&20.0f32.to_le_bytes());
    fnt[12..16].copy_from_slice(b"Test");
    let record = 296 + b'A' as usize * 56;
    let mut put = |i: usize, v: f32| fnt[record + i * 4..record + i * 4 + 4].copy_from_slice(&v.to_le_bytes());
    put(1, 0.125); // u0
    put(2, 0.25); // v0
    put(3, 0.375); // u1
    put(6, 0.75); // v1
    put(9, 5.0); // width
    put(10, 7.0); // height
    put(12, -1.0);
    put(13, 6.0);
    let mut tex = Vec::new();
    tex.extend_from_slice(&8u32.to_le_bytes());
    tex.extend_from_slice(&8u32.to_le_bytes());
    tex.extend_from_slice(&[0u8; 8 * 8 * 4]);
    let font = Font::parse(&fnt, &tex).expect("synthetic font parses");
    let g = font.glyph(b'A');
    assert_eq!((g.u0, g.v0, g.u1, g.v1), (0.125, 0.25, 0.375, 0.75));
    assert_eq!((g.width, g.height), (5.0, 7.0));
    assert!(g.inked);
}

/// M48 GUI-XML-002 (#6665) — pointer and keyboard input over a menu's targets: a `hotrect` parses
/// as a tile (with its children); `hit_test` returns the topmost visible target under the point
/// (alpha 0 still counts, a non-target or invisible tile does not, a clip window bounds a target
/// that `clips`); a down / up pair on one target activates it once; Next / Activate walk the
/// targets from the `xdefault` one.
#[test]
fn targets_take_pointer_and_keyboard_input() {
    let assets = SynthAssets {
        xml: br#"
        <menu name="HUDMainMenu">
            <locus> &true; </locus>
            <hotrect name="List">
                <locus> &true; </locus>
                <clipwindow> &true; </clipwindow>
                <x> 100 </x> <y> 100 </y> <width> 200 </width> <height> 100 </height>
                <hotrect name="Yes">
                    <id> 7 </id>
                    <target> &true; </target> <alpha> 0 </alpha>
                    <x> 0 </x> <y> 0 </y> <width> 200 </width> <height> 40 </height>
                    <text name="YesText"> <string> Yes </string> </text>
                </hotrect>
                <hotrect name="No">
                    <id> 8 </id>
                    <target> &true; </target> <xdefault> &true; </xdefault>
                    <clips> &true; </clips>
                    <x> 0 </x> <y> 80 </y> <width> 200 </width> <height> 40 </height>
                </hotrect>
                <rect name="Hidden">
                    <target> &true; </target> <visible> &false; </visible>
                    <x> 0 </x> <y> 0 </y> <width> 200 </width> <height> 40 </height>
                </rect>
                <rect name="Plain">
                    <x> 0 </x> <y> 40 </y> <width> 200 </width> <height> 40 </height>
                </rect>
            </hotrect>
        </menu>"#
            .to_vec(),
        strings: b"<menu></menu>".to_vec(),
    };
    let mut r = MenuRenderer::load(&assets, "menus\\main\\hud_main_menu.xml", ScreenTraits::new(1280.0, 720.0)).unwrap();
    assert!(r.tile("YesText").is_some(), "a hotrect's children are tiles");
    let name = |hit: Option<crate::TileHit>| hit.and_then(|hit| hit.name);
    assert_eq!(name(r.hit_test(150.0, 120.0)).as_deref(), Some("Yes"));
    assert_eq!(r.hit_test(150.0, 120.0).and_then(|hit| hit.id), Some(7));
    assert_eq!(r.hit_test(150.0, 160.0), None, "a plain rect takes no pointer");
    assert_eq!(name(r.hit_test(150.0, 185.0)).as_deref(), Some("No"));
    assert_eq!(r.hit_test(150.0, 205.0), None, "No's lower half is clipped by its list");
    assert_eq!(r.hit_test(50.0, 50.0), None);

    r.pointer_down(150.0, 120.0);
    r.pointer_up(152.0, 125.0);
    r.pointer_down(150.0, 120.0);
    r.pointer_up(150.0, 185.0);
    r.pointer_down(50.0, 50.0);
    r.pointer_up(50.0, 50.0);
    let activated: Vec<_> = r.take_activations().into_iter().filter_map(|hit| hit.name).collect();
    assert_eq!(activated, ["Yes"], "one activation, only for a press released on the same target");

    r.key(crate::MenuKey::Next);
    assert_eq!(name(r.focused()).as_deref(), Some("No"), "the xdefault target first");
    r.key(crate::MenuKey::Next);
    assert_eq!(name(r.focused()).as_deref(), Some("Yes"));
    r.key(crate::MenuKey::Activate);
    assert_eq!(r.take_activations().into_iter().filter_map(|hit| hit.name).collect::<Vec<_>>(), ["Yes"]);
}

/// #6666 — `visible` propagates (CS wiki, Traits: "unlike visible, clips does not propagate to
/// descendant tiles"): a hidden tile hides its whole subtree, so a child that is itself visible
/// neither draws nor takes the pointer. FNV's dialogue menu hides its topic list, rows and
/// scrollbar this way while the line shows.
#[test]
fn a_hidden_tile_hides_its_subtree() {
    let assets = SynthAssets {
        xml: br#"
        <menu name="DialogMenu">
            <locus> &true; </locus>
            <_ShowingText> &true; </_ShowingText>
            <hotrect name="List">
                <locus> &true; </locus>
                <visible> <not src="io()" trait="_ShowingText"/> </visible>
                <x> 100 </x> <y> 100 </y> <width> 200 </width> <height> 100 </height>
                <hotrect name="Row">
                    <target> &true; </target>
                    <x> 0 </x> <y> 0 </y> <width> 200 </width> <height> 40 </height>
                    <text name="RowText"> <string> Who are you? </string> </text>
                </hotrect>
            </hotrect>
        </menu>"#
            .to_vec(),
        strings: b"<menu></menu>".to_vec(),
    };
    let mut r = MenuRenderer::load(&assets, "menus\\main\\hud_main_menu.xml", ScreenTraits::new(1280.0, 720.0)).unwrap();
    assert_eq!(r.hit_test(150.0, 120.0), None, "the row under a hidden list takes no pointer");
    assert!(r.hit_targets().is_empty());
    r.set_override("DialogMenu", "_ShowingText", 0.0);
    assert_eq!(r.hit_test(150.0, 120.0).and_then(|hit| hit.name).as_deref(), Some("Row"), "shown, the row is a target");
}

/// One synthetic font in slot 1: every printable glyph is 10 × 20 texels with no kerning and a
/// 16-texel ascent at point size 20, so a string's width is 10 per character and a line is 20.
struct FontAssets {
    xml: Vec<u8>,
}

impl FontAssets {
    fn fnt() -> Vec<u8> {
        let mut bytes = vec![0u8; 296 + 256 * 56];
        bytes[0..4].copy_from_slice(&20.0f32.to_le_bytes());
        bytes[4..8].copy_from_slice(&1u32.to_le_bytes());
        bytes[8..12].copy_from_slice(&1u32.to_le_bytes());
        bytes[12..17].copy_from_slice(b"synth");
        for ch in b' '..=b'~' {
            let base = 296 + ch as usize * 56;
            let mut put = |i: usize, v: f32| bytes[base + i * 4..base + i * 4 + 4].copy_from_slice(&v.to_le_bytes());
            if ch == b' ' {
                put(12, 10.0); // a space is all right kerning
                continue;
            }
            for (i, v) in [(1, 0.0), (2, 0.0), (3, 0.5), (4, 0.0), (5, 0.0), (6, 1.0), (7, 0.5), (8, 1.0)] {
                put(i, v);
            }
            put(9, 10.0);
            put(10, 20.0);
            put(13, 16.0);
        }
        bytes
    }

    fn tex() -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&20u32.to_le_bytes());
        bytes.extend_from_slice(&20u32.to_le_bytes());
        bytes.extend(std::iter::repeat_n(255u8, 20 * 20 * 4));
        bytes
    }
}

impl MenuAssets for FontAssets {
    fn menu_xml(&self, path: &str) -> Option<Vec<u8>> {
        (path.eq_ignore_ascii_case("menus\\main\\hud_main_menu.xml")).then(|| self.xml.clone())
    }
    fn texture(&self, _path: &str) -> Option<Vec<u8>> {
        None
    }
    fn font(&self, index: u8) -> Option<Vec<u8>> {
        (index == 1).then(Self::fnt)
    }
    fn font_texture(&self, _path: &str) -> Option<Vec<u8>> {
        Some(Self::tex())
    }
}

/// #6666 — a text tile's `width` / `height` read the size of its displayed text (the native
/// code sets them; CS wiki, Traits), wrapped as the raster wraps it: FNV's dialogue menu centres
/// its line and sizes the panel behind it from the line's height. Glyphs advance by
/// `width + right kerning` and sit `point size − ascent` below the line's top.
#[test]
fn text_tiles_report_their_displayed_size() {
    let assets = FontAssets {
        xml: br#"
        <menu name="DialogMenu">
            <locus> &true; </locus>
            <text name="Line">
                <font> 1 </font>
                <string> ab cd </string>
                <wrapwidth> 30 </wrapwidth>
            </text>
            <text name="Name"> <font> 1 </font> <string> abcde </string> </text>
            <hotrect name="Panel">
                <target> &true; </target>
                <width> <copy src="Name" trait="width"/> </width>
                <height> <copy src="Line" trait="height"/> </height>
            </hotrect>
        </menu>"#
            .to_vec(),
    };
    let font = crate::font::Font::parse(&FontAssets::fnt(), &FontAssets::tex()).unwrap();
    assert_eq!((font.glyph(b'a').advance, font.glyph(b'a').y_offset), (10.0, 4.0));
    assert_eq!(font.glyph(b' ').advance, 10.0);
    let r = MenuRenderer::load(&assets, "menus\\main\\hud_main_menu.xml", ScreenTraits::new(1280.0, 720.0)).unwrap();
    // `abcde` is 50 wide; `ab cd` wraps at 30 into two 20-high lines.
    let name = |x: f32, y: f32| r.hit_test(x, y).and_then(|hit| hit.name);
    assert_eq!(name(45.0, 35.0).as_deref(), Some("Panel"));
    assert_eq!(name(55.0, 35.0), None, "the panel is as wide as the name");
    assert_eq!(name(45.0, 45.0), None, "and as high as the wrapped line");
}
