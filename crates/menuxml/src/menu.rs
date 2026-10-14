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

/// A loaded, renderable menu.
pub struct MenuRenderer {
    doc: Document,
    /// `strings.xml` traits (`_name` → string).
    strings: HashMap<String, Scalar>,
    /// Engine-driven trait overrides, `(tile-name-lower, trait-lower)`.
    overrides: Overrides,
    screen: ScreenTraits,
    fonts: Vec<Option<Arc<Font>>>,
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

        let strings = profile.strings_path.and_then(|path| {
            assets.menu_xml(path).and_then(|b| {
                String::from_utf8(b).ok().map(|s| {
                    let mut src = AssetsAsFileSource { assets };
                    let doc = parse_document(&s, &mut src);
                    let mut map = HashMap::new();
                    // A strings document is a flat `<rect name="Strings">`
                    // of `_name` text traits.
                    for (k, v) in &doc.tiles[0].traits {
                        if let crate::parse::RawTrait::Str(s) = v {
                            map.insert(k.clone(), Scalar::Str(s.clone()));
                        } else if let crate::parse::RawTrait::Num(n) = v {
                            map.insert(k.clone(), Scalar::Num(*n));
                        }
                    }
                    map
                })
            })
        });
        if strings.is_none() {
            log::debug!(
                "menuxml: no strings document for the {} corpus — strings() reads 0",
                profile.label
            );
        }

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
            overrides: Overrides::new(),
            screen,
            fonts,
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

    pub fn clear_override(&mut self, tile: &str, trait_name: &str) {
        self.overrides
            .remove(&(tile.to_lowercase(), trait_name.to_lowercase()));
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

    /// The last `render_frame` raster, without re-evaluating — for callers
    /// that need the pixels after the `&mut self` render call has returned
    /// (#4608's HUD handoff).
    pub fn frame_pixels(&self) -> &[u8] {
        &self.frame.pixels
    }

    /// Evaluate, lay out, and rasterize one frame. Returns the RGBA
    /// pixels (`width * height * 4`, top-down).
    pub fn render_frame(&mut self, assets: &dyn MenuAssets) -> &[u8] {
        let mut eval = EvalState::new(&self.doc, self.screen, &self.strings, &self.overrides);
        eval.resolve_all();
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
                    if let Some(tex) = self.texture(assets, filename, *zoom) {
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
                    let font_ref = self.fonts.get(*font as usize).and_then(|f| f.clone());
                    if let Some(font) = font_ref {
                        self.frame.draw_text_item(item, &font);
                    } else if self.missing.insert(format!("font{font}")) {
                        log::warn!("menuxml: font {font} not loaded — text skipped");
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
    fn texture(&mut self, assets: &dyn MenuAssets, filename: &str, _zoom: f32) -> Option<Arc<Rgba8>> {
        let normalized = filename.trim().replace('/', "\\");
        let cache_key = normalized.to_lowercase();
        if let Some(cached) = self.tex_cache.get(&cache_key) {
            return cached.clone();
        }
        // Candidate archive paths: preferred set first, then the other
        // classes (not every texture ships in every set).
        let lowered = normalized.to_lowercase();
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
            if let Some(len) = Rgba8::decoded_len(&bytes) {
                if self.decoded_bytes.saturating_add(len) > self.texture_budget {
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
