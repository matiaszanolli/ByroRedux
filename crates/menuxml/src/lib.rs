//! `byroredux-menuxml` — the Oblivion / FO3 / FNV legacy UI track.
//!
//! These games drive their HUD and menus from XML documents
//! (`menus\*.xml` in `Oblivion - Misc.bsa` / `Fallout - Textures*.bsa`)
//! interpreted by Gamebryo's tile system: a tree of `rect` / `image` /
//! `text` / `nif` tiles whose traits are computed every frame by a
//! fold-style operator expression language (CS Wiki "Oblivion XML
//! Reference"). Skyrim and later moved to Scaleform SWF, which is the
//! separate `byroredux-ui` (Ruffle) track — ROADMAP M48's legacy-UI
//! sub-track starts here.
//!
//! Pipeline:
//!
//! ```text
//! BSA bytes ──parse──▶ Document (tile arena + operator chains)
//!             eval    ▶ per-frame trait values (fold + selectors)
//!             layout  ▶ absolute rects, depth-ordered draw list
//!             raster  ▶ RGBA frame for the engine's UI overlay upload
//! ```
//!
//! The renderer is CPU-side by design: menu pixel budgets are tiny (a few
//! dozen quads), and routing through the same RGBA upload the Scaleform
//! overlay uses keeps one compositing path for every game's UI.

pub mod eval;
pub mod font;
pub mod layout;
pub mod menu;
pub mod parse;
pub mod profile;
pub mod raster;
pub mod tex;

#[cfg(test)]
mod tests;

pub use eval::{Overrides, ScreenTraits};
pub use font::{Font, FontError, Glyph};
pub use layout::{DrawItem, Rect};
pub use menu::{MenuAssets, MenuError, MenuRenderer, TextureSet};
pub use parse::{Document, Op, OpArg, OpKind, RawTrait, Scalar, Tile, TileKind};
pub use profile::{FontArchive, MenuProfile};
pub use raster::Framebuffer;
pub use tex::{Rgba8, TexError};
