//! Menu runtime: owns a parsed menu document plus its assets and renders
//! frames driven by engine-side trait overrides.
//!
//! This is the crate's public entry point. The engine:
//!
//! 1. implements [`MenuAssets`] (BSA-backed in `byroredux`);
//! 2. loads a menu via [`MenuRenderer::load`] (`menus\main\hud_main_menu.xml`);
//! 3. pushes gameplay state through [`MenuRenderer::set_override`]
//!    (health/magicka/fatigue fractions, compass heading);
//! 4. calls [`MenuRenderer::render_frame`] each frame and uploads the
//!    returned RGBA through the same path the Scaleform overlay uses.

use std::collections::HashMap;
use std::collections::HashSet;
use std::sync::Arc;

use crate::eval::{EvalState, Overrides, ScreenTraits};
use crate::font::Font;
use crate::layout::{build_draw_list, DrawItem};
use crate::parse::{parse_document, Document, MenuFileSource, Scalar};
use crate::profile::MenuProfile;
use crate::raster::Framebuffer;
use crate::tex::Rgba8;

/// Asset access for one game's UI corpus.
pub trait MenuAssets {
    /// Menu XML bytes by archive path (`menus\main\hud_main_menu.xml`,
    /// `menus\prefabs\button_long.xml`, `menus\strings.xml`).
    fn menu_xml(&self, path: &str) -> Option<Vec<u8>>;
    /// Texture bytes by canonical archive path
    /// (`textures\menus\hud\hud_back.dds`).
    fn texture(&self, path: &str) -> Option<Vec<u8>>;
    /// `.fnt` bytes by 1-based font table index (the `<font>` trait
    /// value; Oblivion.ini `[Fonts]` SFontFile_1..5 order).
    fn font(&self, index: u8) -> Option<Vec<u8>>;
    /// `.tex` glyph-atlas bytes by archive path
    /// (`fonts\Kingthings_Regular_0_Lod_A.tex`).
    fn font_texture(&self, path: &str) -> Option<Vec<u8>>;
}

/// Menu-texture resolution set: Oblivion ships menu art at three
/// resolution classes (`textures\Menus`, `\Menus80`, `\Menus50`) and the
/// authored `<filename>` paths name the base set. Selected by UI width —
/// the source engine picked the class whose design width nearest-matched
/// the screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextureSet {
    Base,
    Menus80,
    Menus50,
}

impl TextureSet {
    pub fn for_width(ui_width: f32) -> Self {
        if ui_width <= 640.0 {
            Self::Menus50
        } else if ui_width <= 800.0 {
            Self::Menus80
        } else {
            Self::Base
        }
    }

    fn segment(&self) -> &'static str {
        match self {
            Self::Base => "menus",
            Self::Menus80 => "menus80",
            Self::Menus50 => "menus50",
        }
    }
}

/// Errors from menu loading.
#[derive(Debug, thiserror::Error)]
pub enum MenuError {
    #[error("menu XML '{0}' not found")]
    MissingMenu(String),
    #[error("menu XML '{0}' is not valid UTF-8")]
    BadUtf8(String),
    #[error("tile '{0}' not found")]
    MissingTile(String),
    #[error("font {0} unavailable: {1}")]
    FontUnavailable(u8, String),
    #[error("strings.xml missing — strings() selections read as 0")]
    MissingStrings,
}

/// Ceiling on the decoded RGBA bytes one [`MenuRenderer`] keeps (#5658).
///
/// Each image is already capped at 8192² (256 MiB) by the decoder, but the
/// cache never evicts, so without a total a mod menu naming K distinct
/// textures pinned K × 256 MiB for the HUD's lifetime (about 48 KB of
/// archive per 256 MiB; a few MB of archive aborted a 32 GiB machine).
/// Sized from the vanilla census (real decoder, every archive in the
/// install, 2026-10-12): decoding *every* menu texture a game ships at
/// once totals 979.9 MiB for Oblivion (6,111 files, all three resolution
/// sets), 753.8 MiB for FNV and 425.9 MiB for FO3, largest single image
/// 16 MiB. A renderer only ever decodes the handful its document names,
/// so 1 GiB cannot refuse vanilla art in any supported game.
const MAX_DECODED_TEXTURE_BYTES: usize = 1 << 30;

/// A target tile the pointer or the keyboard reached (GUI-XML-002): its arena index, its
/// document name and its `<id>`, the handle the engine's menu code keys a control by.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TileHit {
    pub tile: usize,
    pub name: Option<String>,
    pub id: Option<i32>,
}

/// A navigation key, as the input adapter maps the platform's keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuKey {
    /// Focus the next target in draw order (Down / Tab).
    Next,
    /// Focus the previous target (Up / Shift+Tab).
    Previous,
    /// Activate the focused target (Enter / Space).
    Activate,
}

/// Activations held for a drain at most; the oldest goes past it.
const MAX_PENDING_ACTIVATIONS: usize = 64;

/// A flat document's traits (`strings.xml`, `globals.xml`: one `<rect name="Strings">` of `_name`
/// traits), literal values only — an operator-built trait in one reads as absent (0).
fn flat_document(assets: &dyn MenuAssets, path: &str) -> Option<HashMap<String, Scalar>> {
    let text = String::from_utf8(assets.menu_xml(path)?).ok()?;
    let mut src = AssetsAsFileSource { assets };
    let doc = parse_document(&text, &mut src);
    let mut map = HashMap::new();
    for (k, v) in &doc.tiles[0].traits {
        match v {
            crate::parse::RawTrait::Str(s) => {
                map.insert(k.clone(), Scalar::Str(s.clone()));
            }
            crate::parse::RawTrait::Num(n) => {
                map.insert(k.clone(), Scalar::Num(*n));
            }
            crate::parse::RawTrait::Ops(_) => {}
        }
    }
    Some(map)
}

/// A loaded, renderable menu.
pub struct MenuRenderer {
    doc: Document,
    /// `strings.xml` traits (`_name` → string).
    strings: HashMap<String, Scalar>,
    /// `globals.xml` traits, the `globals()` source.
    globals: HashMap<String, Scalar>,
    /// Engine-driven trait overrides, `(tile-name-lower, trait-lower)`.
    overrides: Overrides,
    screen: ScreenTraits,
    fonts: Vec<Option<Arc<Font>>>,
    default_font: u8,
    /// Authored `<filename>` (lowercased) → its resolved texture, or `None`
    /// once it is known missing, undecodable or over budget.
    pub(crate) tex_cache: HashMap<String, Option<Arc<Rgba8>>>,
    /// Decoded images by resolved archive path, so the `Menus\` /
    /// `Menus80\` / `textures\menus\` spellings of one file share a
    /// single decode and are charged against the budget once (#5658).
    pub(crate) decoded: HashMap<String, Arc<Rgba8>>,
    /// RGBA bytes held in `decoded`, against `texture_budget`.
    pub(crate) decoded_bytes: usize,
    /// [`MAX_DECODED_TEXTURE_BYTES`]; a field so tests can shrink it.
    pub(crate) texture_budget: usize,
    /// The over-budget warning fires once per renderer.
    budget_warned: bool,
    /// Archive paths already refused for budget (#5693). `tex_cache`
    /// remembers the refusal only under the authored spelling, so each
    /// additional spelling of the same over-budget file used to pay a
    /// full archive extract (~33 MB inflated per 8192² DXT1) for the
    /// same refusal. Consulted before the fetch, so a path is extracted
    /// for a budget decision at most once per renderer.
    refused_paths: HashSet<String>,
    /// Filenames already logged as missing (once each). Also dedupes the
    /// #5670 substitution notices, under a `substituted:`-prefixed key.
    missing: HashSet<String>,
    frame: Framebuffer,
    texture_set: TextureSet,
    /// nif tiles encountered (diagnostic, logged once at load).
    pub nif_tiles: usize,
    /// The target the pointer went down on, until it comes up.
    pressed: Option<usize>,
    /// The keyboard-focused target.
    focus: Option<usize>,
    /// Activations since the last [`Self::take_activations`].
    activations: Vec<TileHit>,
}

struct AssetsAsFileSource<'a> {
    assets: &'a dyn MenuAssets,
}

impl MenuFileSource for AssetsAsFileSource<'_> {
    fn menu_xml(&self, path: &str) -> Option<Vec<u8>> {
        self.assets.menu_xml(path)
    }
}

impl MenuRenderer {
    /// Load `menu_path` (e.g. `menus\main\hud_main_menu.xml`) with the
    /// Oblivion corpus profile. See [`load_with_profile`].
    pub fn load(
        assets: &dyn MenuAssets,
        menu_path: &str,
        screen: ScreenTraits,
    ) -> Result<Self, MenuError> {
        Self::load_with_profile(assets, menu_path, screen, MenuProfile::oblivion())
    }

    /// Load with an explicit per-game corpus profile: the font table
    /// (slot count from `profile.font_paths`; the paths themselves are
    /// the [`MenuAssets`] implementor's concern) and the strings
    /// document feeding `strings()` selections.
    pub fn load_with_profile(
        assets: &dyn MenuAssets,
        menu_path: &str,
        screen: ScreenTraits,
        profile: MenuProfile,
    ) -> Result<Self, MenuError> {
        let xml = assets
            .menu_xml(menu_path)
            .ok_or_else(|| MenuError::MissingMenu(menu_path.to_string()))?;
        let text = String::from_utf8(xml)
            .map_err(|_| MenuError::BadUtf8(menu_path.to_string()))?;

        let strings = profile.strings_path.and_then(|path| flat_document(assets, path));
        if strings.is_none() {
            log::debug!(
                "menuxml: no strings document for the {} corpus — strings() reads 0",
                profile.label
            );
        }
        let globals = profile.globals_path.and_then(|path| flat_document(assets, path));

        let mut src = AssetsAsFileSource { assets };
        let doc = parse_document(&text, &mut src);
        let nif_tiles = doc
            .tiles
            .iter()
            .filter(|t| t.kind == crate::parse::TileKind::Nif)
            .count();

        // Font table: 1-based slots per the game ini's [Fonts] order
        // (the profile's table length). Any slot that fails to load
        // stays `None` and text using it is skipped (with one warn)
        // rather than aborting the whole HUD.
        let mut fonts = Vec::new();
        fonts.push(None); // index 0 — unused; `<font> 0` is unauthored
        for index in 1u8..=profile.font_paths.len() as u8 {
            let loaded = assets.font(index).and_then(|fnt| {
                // The .fnt names its atlas; resolve through the profile's
                // candidate paths (Oblivion: `fonts\<name>.tex`; FO3 ships
                // `.tex` and `.dds` beside each `.fnt`).
                // #4652 (PAR-D2-2026-09-21-03) — `&fnt[12..]` panicked on
                // any .fnt shorter than 12 bytes, BEFORE Font::parse's own
                // TruncatedHeader check could route the file through the
                // warn-and-None degrade path. `get` yields an empty name
                // for a short font, which Font::parse then rejects.
                let name = String::from_utf8_lossy(fnt.get(12..).unwrap_or_default())
                    .split('\0')
                    .next()
                    .unwrap_or("")
                    .to_string();
                (profile.font_atlas)(&name)
                    .iter()
                    .find_map(|p| assets.font_texture(p))
                    .and_then(|tex| Font::parse(&fnt, &tex).ok())
                    .map(Arc::new)
                    .map(Some)
                    .unwrap_or_else(|| {
                        log::warn!("menuxml: font {index} ('{name}') failed to load");
                        None
                    })
            });
            fonts.push(loaded);
        }

        let texture_set = TextureSet::for_width(screen.width);
        Ok(Self {
            doc,
            strings: strings.unwrap_or_default(),
            globals: globals.unwrap_or_default(),
            overrides: Overrides::new(),
            screen,
            fonts,
            default_font: 0,
            tex_cache: HashMap::new(),
            decoded: HashMap::new(),
            decoded_bytes: 0,
            texture_budget: MAX_DECODED_TEXTURE_BYTES,
            budget_warned: false,
            refused_paths: HashSet::new(),
            missing: HashSet::new(),
            frame: Framebuffer::new(screen.width as u32, screen.height as u32),
            texture_set,
            nif_tiles,
            pressed: None,
            focus: None,
            activations: Vec::new(),
        })
    }

    /// Set an engine-side trait override on a named tile.
    ///
    /// The HUD contract (from vanilla XML comments):
    /// * `hudmain_health_full` / `hudmain_magic_full` /
    ///   `hudmain_fatigue_full` — `user0` = 0..1 fill fraction;
    /// * `hudmain_compass_window` — `user0` = heading degrees
    ///   (0/360 = north);
    /// * the menu root (`HUDMainMenu`) — `user3` = 1 explore / 0 menu
    ///   mode.
    pub fn set_override(&mut self, tile: &str, trait_name: &str, value: f32) {
        self.overrides
            .insert((tile.to_lowercase(), trait_name.to_lowercase()), Scalar::Num(value));
    }

    pub fn set_override_str(&mut self, tile: &str, trait_name: &str, value: &str) {
        self.overrides.insert(
            (tile.to_lowercase(), trait_name.to_lowercase()),
            Scalar::Str(value.to_string()),
        );
    }

    /// Runtime menu-API: clone a `<template>` prototype's content subtree
    /// under the named tile, renaming the clone's root `new_name`.
    ///
    /// The source engine assembled its FO3 HUD this way —
    /// `hud_main_menu.xml` ships only empty container rects (`HitPoints`,
    /// `ActionPoints`, …) while the art lives in `<template>` prototypes
    /// (`menus\prefabs\hudtemplates.xml`) that engine code instantiated
    /// at runtime. Templates with several children clone all of them;
    /// `new_name` applies to the first.
    pub fn instantiate_template(
        &mut self,
        template_name: &str,
        parent_name: &str,
        new_name: &str,
    ) -> Result<usize, MenuError> {
        let template = self
            .doc
            .name_index
            .get(&template_name.to_lowercase())
            .copied()
            .ok_or_else(|| MenuError::MissingTile(template_name.to_string()))?;
        let parent = self
            .doc
            .name_index
            .get(&parent_name.to_lowercase())
            .copied()
            .ok_or_else(|| MenuError::MissingTile(parent_name.to_string()))?;
        let children = self.doc.tiles[template].children.clone();
        let first = children.first().copied().ok_or_else(|| {
            MenuError::MissingTile(format!("{template_name} has no content to instantiate"))
        })?;
        let idx = self.doc.deep_clone(first, parent, Some(new_name));
        for &child in &children[1..] {
            self.doc.deep_clone(child, parent, None);
        }
        Ok(idx)
    }

    /// Runtime menu-API: graft a parsed prefab fragment (loose traits +
    /// children — e.g. FO3's ops-driven `menus\prefabs\meter.xml`) as a
    /// new child rect named `new_name` under the named tile.
    ///
    /// The fragment's top-level traits land on the wrapper tile (the
    /// same splicing semantics `<include>` applies), so `src="parent()"`
    /// ops inside the fragment resolve against the wrapper — the driver
    /// then drives the wrapper (`_Value`, `x`, …) through overrides.
    pub fn graft_fragment(
        &mut self,
        assets: &dyn MenuAssets,
        fragment_path: &str,
        parent_name: &str,
        new_name: &str,
    ) -> Result<usize, MenuError> {
        let xml = assets
            .menu_xml(fragment_path)
            .ok_or_else(|| MenuError::MissingMenu(fragment_path.to_string()))?;
        let text = String::from_utf8(xml)
            .map_err(|_| MenuError::BadUtf8(fragment_path.to_string()))?;
        // Prefab fragments are rootless (loose traits + tiles); wrapping
        // them in an explicit rect makes the wrapper own those traits,
        // exactly the `<include>` splice semantics the graft wants.
        let wrapped = format!("<rect name=\"{new_name}\">\n{text}\n</rect>");
        let mut src = AssetsAsFileSource { assets };
        let fragment = parse_document(&wrapped, &mut src);
        let parent = self
            .doc
            .name_index
            .get(&parent_name.to_lowercase())
            .copied()
            .ok_or_else(|| MenuError::MissingTile(parent_name.to_string()))?;
        Ok(self.doc.graft_subtree(&fragment, 0, parent, Some(new_name)))
    }

    /// Evaluate every trait for this frame. A text tile's `width` / `height` are the size of its
    /// displayed text, which the native code sets and documents read (CS wiki, Traits: "these
    /// traits cannot be set on Text tiles; instead, the executable sets them to the size of the
    /// displayed text if the text is visible") — FNV's dialogue menu centres the line and sizes
    /// its panel from them (#6666). A first pass resolves each text's string, font and wrap; a
    /// second answers the measured sizes.
    fn evaluate(&self) -> EvalState<'_> {
        let fresh = || EvalState::new(&self.doc, self.screen, &self.strings, &self.overrides).with_globals(&self.globals);
        let mut first = fresh();
        first.resolve_all();
        let sizes = self.measure_texts(&mut first);
        if sizes.is_empty() {
            return first;
        }
        let mut eval = fresh().with_text_sizes(sizes);
        eval.resolve_all();
        eval
    }

    /// The displayed size of every visible text tile with a loaded font, wrapped as the raster
    /// wraps it.
    fn measure_texts(&self, eval: &mut EvalState) -> HashMap<usize, (f32, f32)> {
        let mut sizes = HashMap::new();
        for (tile, data) in self.doc.tiles.iter().enumerate() {
            if data.kind != crate::parse::TileKind::Text || !eval.trait_value(tile, "visible").truthy() {
                continue;
            }
            let string = match eval.trait_value(tile, "string") {
                Scalar::Str(s) => s,
                Scalar::Num(_) => continue,
            };
            let font_index = eval.trait_value(tile, "font").as_num().clamp(0.0, 255.0) as u8;
            let Some(font) = self.fonts.get(self.effective_font(font_index)).and_then(|f| f.as_deref()) else { continue };
            let wrap_width = eval.trait_value(tile, "wrapwidth").as_num().max(0.0);
            let wrap_lines = eval.trait_value(tile, "wraplines").as_num().max(0.0) as u32;
            let lines = crate::raster::wrap(&string, font, wrap_width, wrap_lines);
            let width = lines.iter().map(|line| font.measure_width(line)).fold(0.0, f32::max);
            sizes.insert(tile, (width, lines.len() as f32 * font.line_height()));
        }
        sizes
    }

    /// The tiles that take the pointer this frame, in draw order (later is on top).
    pub fn hit_targets(&self) -> Vec<crate::layout::HitTarget> {
        let mut eval = self.evaluate();
        crate::layout::build_layout(&self.doc, &mut eval).1
    }

    fn hit(&self, tile: usize) -> TileHit {
        let tile_data = &self.doc.tiles[tile];
        TileHit { tile, name: tile_data.name.clone(), id: tile_data.id }
    }

    /// The topmost target under `(x, y)` (UI pixels), if any (GUI-XML-002).
    pub fn hit_test(&self, x: f32, y: f32) -> Option<TileHit> {
        self.hit_targets().iter().rev().find(|target| target.contains(x, y)).map(|target| self.hit(target.tile))
    }

    /// The pointer went down at `(x, y)`: the target there is pressed (and focused).
    pub fn pointer_down(&mut self, x: f32, y: f32) {
        self.pressed = self.hit_test(x, y).map(|hit| hit.tile);
        if self.pressed.is_some() {
            self.focus = self.pressed;
        }
    }

    /// The pointer came up at `(x, y)`: a target pressed and released in place is activated once.
    pub fn pointer_up(&mut self, x: f32, y: f32) {
        let pressed = self.pressed.take();
        if let Some(hit) = self.hit_test(x, y).filter(|hit| Some(hit.tile) == pressed) {
            self.push_activation(hit);
        }
    }

    /// A navigation key: move the focus through the targets in draw order (starting from the one
    /// whose `xdefault` is set), or activate the focused one.
    pub fn key(&mut self, key: MenuKey) {
        let targets: Vec<usize> = self.hit_targets().iter().map(|target| target.tile).collect();
        if targets.is_empty() {
            self.focus = None;
            return;
        }
        let position = self.focus.and_then(|focus| targets.iter().position(|tile| *tile == focus));
        match key {
            MenuKey::Activate => {
                if let Some(tile) = position.map(|at| targets[at]) {
                    let hit = self.hit(tile);
                    self.push_activation(hit);
                }
            }
            MenuKey::Next | MenuKey::Previous => {
                let next = match (position, key) {
                    (None, _) => self.default_target(&targets).unwrap_or(0),
                    (Some(at), MenuKey::Next) => (at + 1) % targets.len(),
                    (Some(at), _) => (at + targets.len() - 1) % targets.len(),
                };
                self.focus = Some(targets[next]);
            }
        }
    }

    /// The first target whose `xdefault` trait is set (the list items' default focus).
    fn default_target(&self, targets: &[usize]) -> Option<usize> {
        let mut eval = self.evaluate();
        targets.iter().position(|tile| eval.trait_value(*tile, "xdefault").truthy())
    }

    /// The keyboard-focused target, if any.
    pub fn focused(&self) -> Option<TileHit> {
        self.focus.map(|tile| self.hit(tile))
    }

    /// The current pointer capture, if a target received a press.
    pub fn pressed(&self) -> Option<TileHit> {
        self.pressed.map(|tile| self.hit(tile))
    }

    /// Release pointer capture when the menu loses window focus.
    pub fn cancel_pointer(&mut self) {
        self.pressed = None;
    }

    fn push_activation(&mut self, hit: TileHit) {
        if self.activations.len() >= MAX_PENDING_ACTIVATIONS {
            self.activations.remove(0);
        }
        self.activations.push(hit);
    }

    /// The targets activated since the last call, in order.
    pub fn take_activations(&mut self) -> Vec<TileHit> {
        std::mem::take(&mut self.activations)
    }

    /// The evaluated value of `tile`'s `trait_name` this frame (overrides, `io()`, measured
    /// text sizes included), as native code reads a menu back to size it (a message box's width
    /// from its text, #6666). `None` when no tile has the name.
    pub fn value(&self, tile: &str, trait_name: &str) -> Option<Scalar> {
        let index = self.tile(tile)?;
        Some(self.evaluate().trait_value(index, trait_name))
    }

    /// Tile index by name, for drivers that want to probe traits.
    pub fn tile(&self, name: &str) -> Option<usize> {
        self.doc.name_index.get(&name.to_lowercase()).copied()
    }

    pub fn screen(&self) -> ScreenTraits {
        self.screen
    }

    pub fn set_screen(&mut self, screen: ScreenTraits) {
        if (screen.width - self.screen.width).abs() > 0.5
            || (screen.height - self.screen.height).abs() > 0.5
        {
            self.frame = Framebuffer::new(screen.width as u32, screen.height as u32);
            self.texture_set = TextureSet::for_width(screen.width);
        }
        self.screen = screen;
    }

    /// Use this slot for text for which the document leaves the font unauthored.
    pub fn set_default_font(&mut self, slot: u8) {
        self.default_font = slot;
    }

    fn effective_font(&self, authored: u8) -> usize {
        usize::from(if authored == 0 { self.default_font } else { authored })
    }

    /// The last `render_frame` raster, without re-evaluating — for callers
    /// that need the pixels after the `&mut self` render call has returned
    /// (#4608's HUD handoff).
    pub fn frame_pixels(&self) -> &[u8] {
        &self.frame.pixels
    }

    /// Evaluate, lay out, and rasterize one frame. Returns the RGBA
    /// pixels (`width * height * 4`, top-down).
    pub fn render_frame(&mut self, assets: &dyn MenuAssets) -> &[u8] {
        let mut eval = self.evaluate();
        let items = build_draw_list(&self.doc, &mut eval);
        log::debug!(
            "menuxml frame: {} tiles -> {} draw items ({} textures cached)",
            self.doc.tiles.len(),
            items.len(),
            self.tex_cache.len()
        );

        self.frame.clear([0, 0, 0, 0]);
        for item in &items {
            match item {
                DrawItem::Image {
                    rect,
                    filename,
                    crop,
                    zoom,
                    tint,
                    alpha,
                    tiled,
                    clip,
                    ..
                } => {
                    if let Some(tex) = self.texture(assets, filename) {
                        self.frame
                            .blit(
                                &tex,
                                *rect,
                                *crop,
                                *zoom,
                                *tiled,
                                crate::raster::BlitStyle {
                                    tint: *tint,
                                    alpha: *alpha,
                                    clip: *clip,
                                },
                            );
                    }
                }
                DrawItem::Text { font, .. } => {
                    let slot = self.effective_font(*font);
                    let font_ref = self.fonts.get(slot).and_then(|f| f.clone());
                    if let Some(font) = font_ref {
                        self.frame.draw_text_item(item, &font);
                    } else if self.missing.insert(format!("font{slot}")) {
                        log::warn!("menuxml: font {slot} not loaded — text skipped");
                    }
                }
                DrawItem::Fill {
                    rect,
                    tint,
                    alpha,
                    clip,
                    ..
                } => {
                    self.frame
                        .fill(*rect, *tint, *alpha, *clip);
                }
            }
        }
        &self.frame.pixels
    }

    pub fn frame_size(&self) -> (u32, u32) {
        (self.frame.width, self.frame.height)
    }

    /// Resolve an authored `<filename>` to a decoded texture through the
    /// resolution-class table (`Menus\…` → `textures\menus…\…`).
    fn texture(&mut self, assets: &dyn MenuAssets, filename: &str) -> Option<Arc<Rgba8>> {
        let normalized = filename.trim().replace('/', "\\");
        let cache_key = normalized.trim_start_matches('\\').to_lowercase();
        if let Some(cached) = self.tex_cache.get(&cache_key) {
            return cached.clone();
        }
        // Candidate archive paths: preferred set first, then the other
        // classes (not every texture ships in every set).
        let lowered = &cache_key;
        let candidates: Vec<String> = if let Some(rest) = lowered
            .strip_prefix("menus\\")
            .or_else(|| lowered.strip_prefix("menus80\\"))
            .or_else(|| lowered.strip_prefix("menus50\\"))
        {
            let mut v = vec![format!(
                "textures\\{}\\{}",
                self.texture_set.segment(),
                rest
            )];
            for set in [TextureSet::Base, TextureSet::Menus80, TextureSet::Menus50] {
                let p = format!("textures\\{}\\{}", set.segment(), rest);
                if !v.contains(&p) {
                    v.push(p);
                }
            }
            v
        } else if let Some(rest) = lowered.strip_prefix("textures\\") {
            vec![format!("textures\\{rest}")]
        } else {
            vec![format!("textures\\{lowered}")]
        };

        // #5400 — a present-but-undecodable candidate must not end the
        // search (a decodable sibling in another resolution set can still
        // serve the menu), and the warning must say which failure it was,
        // so an operator isn't sent hunting for a missing archive over a
        // decoder gap.
        let mut decoded: Option<Arc<Rgba8>> = None;
        let mut undecodable: Vec<&str> = Vec::new();
        let mut refused_any = false;
        for path in &candidates {
            if let Some(shared) = self.decoded.get(path) {
                decoded = Some(Arc::clone(shared));
                break;
            }
            // #5693 — a path already refused for budget is not fetched
            // again: the refusal is remembered per resolved archive
            // path, not just per authored spelling.
            if self.refused_paths.contains(path) {
                refused_any = true;
                continue;
            }
            let Some(bytes) = assets.texture(path) else { continue };
            // #5658 — charge the decode against the renderer's budget from
            // the header alone, before anything is allocated. Over budget,
            // this candidate resolves to nothing and is remembered; a
            // smaller sibling in another resolution set may still serve,
            // and already-decoded images keep being served.
            if let Some(len) = Rgba8::decoded_len(&bytes)
                && self.decoded_bytes.saturating_add(len) > self.texture_budget {
                    if !self.budget_warned {
                        self.budget_warned = true;
                        log::warn!(
                            "menuxml: menu texture budget ({} MiB) exhausted at '{filename}' \
                             ({path}, {} MiB decoded); this texture is not drawn — later \
                             textures that still fit the budget are",
                            self.texture_budget >> 20,
                            len >> 20,
                        );
                    }
                    self.refused_paths.insert(path.clone());
                    refused_any = true;
                    continue;
                }
            match Rgba8::decode_dds(&bytes) {
                Some(img) => {
                    let img = Arc::new(img);
                    self.decoded_bytes += img.pixels.len();
                    self.decoded.insert(path.clone(), Arc::clone(&img));
                    decoded = Some(img);
                    break;
                }
                None => undecodable.push(path),
            }
        }
        if decoded.is_none() && !refused_any && self.missing.insert(cache_key.clone()) {
            if undecodable.is_empty() {
                log::warn!("menuxml: menu texture '{filename}' not found in any resolution set");
            } else {
                log::warn!(
                    "menuxml: menu texture '{filename}' present but undecodable in: {}",
                    undecodable.join(", ")
                );
            }
        }
        // #5670 — a lower-priority set's copy silently standing in for an
        // undecodable preferred-set file: the substitution is the right
        // call (the menu draws; #5400 kept the search going), but the
        // degraded result deserves its one log line, once per spelling.
        // Deduped through `missing` under a prefixed key.
        if decoded.is_some()
            && !undecodable.is_empty()
            && self.missing.insert(format!("substituted:{cache_key}"))
        {
            log::warn!(
                "menuxml: menu texture '{filename}' undecodable in the preferred set ({}); \
                 serving another resolution set's copy",
                undecodable.join(", ")
            );
        }
        self.tex_cache.insert(cache_key, decoded.clone());
        decoded
    }
}
