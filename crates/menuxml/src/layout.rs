//! Layout: absolute positioning + depth-ordered draw-list construction.
//!
//! Two passes over the resolved trait values:
//!
//! 1. **Locus walk** — a tile's `x`/`y` are "relative to the left/top
//!    edge of the tile's locus ancestor" (CS Wiki trait docs): the
//!    nearest ancestor-or-self with `<locus> &true;`. Every non-locus
//!    tile passes its parent's origin through unchanged.
//! 2. **Draw-list walk** — depth-sorted within each parent (ascending
//!    `depth`, document order as tiebreak), skipping `visible` false and
//!    `alpha` 0 tiles, intersecting `clipwindow` ancestor rects.

use crate::eval::EvalState;
use crate::parse::{Document, Scalar, TileKind};

/// Axis-aligned rect in UI pixel space.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

/// One drawable produced by the layout walk.
#[derive(Debug, Clone)]
pub enum DrawItem {
    Image {
        tile: usize,
        rect: Rect,
        /// Authored texture path (`Menus\HUD\hud_back.dds`), resolved to
        /// an archive path at raster time.
        filename: String,
        /// Source-crop in display pixels (applied post-zoom per the wiki).
        crop: (f32, f32),
        /// Authored zoom; −1 (the default, `&scale;`) stretches to the
        /// tile rect.
        zoom: f32,
        /// `<tile> &true;` — repeat the texture at 1:1 texel scale
        /// across the tile rect (FO3-era trait; the tick-mark meters
        /// and scrolling compass strip depend on it).
        tiled: bool,
        tint: [f32; 3],
        alpha: f32,
        clip: Option<Rect>,
    },
    Text {
        tile: usize,
        /// Draw origin (top-left of the first line's ink box, before
        /// justify shifting).
        x: f32,
        y: f32,
        string: String,
        /// `<font>` trait value (1-based font table index).
        font: u8,
        /// 0 left, 1 center, 2 right.
        justify: u8,
        tint: [f32; 3],
        alpha: f32,
        wrap_width: f32,
        wrap_lines: u32,
        clip: Option<Rect>,
    },
    /// Solid fill (rect tiles with visible color+alpha; rare — vanilla
    /// rect tiles are almost always alpha-0 hit targets).
    Fill {
        tile: usize,
        rect: Rect,
        tint: [f32; 3],
        alpha: f32,
        clip: Option<Rect>,
    },
}

/// Clamp a 0–255-sourced float colour channel into u8 at draw time.
fn u8_color(v: f32) -> u8 {
    v.clamp(0.0, 255.0) as u8
}

/// Numeric trait read with default 0 — the layout hot path.
fn num(eval: &mut EvalState, tile: usize, name: &str) -> f32 {
    eval.trait_value(tile, name).as_num()
}

impl DrawItem {
    /// Arena index of the emitting tile.
    pub fn tile(&self) -> usize {
        match self {
            DrawItem::Image { tile, .. }
            | DrawItem::Text { tile, .. }
            | DrawItem::Fill { tile, .. } => *tile,
        }
    }
}

/// Build the depth-ordered draw list for a document.
pub fn build_draw_list(doc: &Document, eval: &mut EvalState) -> Vec<DrawItem> {
    let mut out = Vec::new();
    // Root position: the menu's own x/y (usually 0,0).
    let root_x = num(eval, 0, "x");
    let root_y = num(eval, 0, "y");
    let root_rect = Rect {
        x: root_x,
        y: root_y,
        w: num(eval, 0, "width").max(0.0),
        h: num(eval, 0, "height").max(0.0),
    };
    walk_children(doc, eval, 0, (root_x, root_y), None, &mut out, root_rect);
    out
}

/// Walk one tile's children, sorting by depth (stable — document order
/// tiebreak). `origin` is the position the *children* of `parent` are
/// relative to (the parent's absolute position when the parent is a
/// locus tile).
fn walk_children(
    doc: &Document,
    eval: &mut EvalState,
    parent: usize,
    origin: (f32, f32),
    clip: Option<Rect>,
    out: &mut Vec<DrawItem>,
    parent_rect: Rect,
) {
    // Depth is read for every child first so the sort does not interleave
    // with evaluation (memoised reads make the second pass free).
    let mut depth: Vec<(usize, f32)> = doc.tiles[parent]
        .children
        .iter()
        .map(|&c| (c, num(eval, c, "depth")))
        .collect();
    depth.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

    for (child, _) in depth {
        if doc.tiles[child].kind == TileKind::Template {
            continue;
        }
        let visible = eval.trait_value(child, "visible").truthy();
        let alpha = num(eval, child, "alpha").clamp(0.0, 255.0);
        let x = origin.0 + num(eval, child, "x");
        let y = origin.1 + num(eval, child, "y");
        let w = num(eval, child, "width").max(0.0);
        let h = num(eval, child, "height").max(0.0);
        let rect = Rect { x, y, w, h };
        let locus = eval.trait_value(child, "locus").truthy();
        let child_origin = if locus { (x, y) } else { origin };

        // Clip windows: a `<clipwindow> &true;` tile clips its
        // descendants (that opt in via `clips`, plus every deeper
        // descendant of the window — the compass uses both forms).
        let mut child_clip = clip;
        if eval.trait_value(child, "clipwindow").truthy() {
            let own = Some(rect);
            child_clip = match clip {
                Some(existing) => intersect(existing, rect).or(Some(existing_zeroed(existing))),
                None => own,
            };
        }

        if visible && alpha > 0.5 {
            let tint = [
                num(eval, child, "red"),
                num(eval, child, "green"),
                num(eval, child, "blue"),
            ];
            match doc.tiles[child].kind {
                TileKind::Image => {
                    let filename = match eval.trait_value(child, "filename") {
                        Scalar::Str(s) => s,
                        Scalar::Num(_) => String::new(),
                    };
                    if !filename.is_empty() {
                        // A zero width/height falls back to the texture's
                        // natural size at raster time (TiImage behaviour).
                        // `clips` opts this tile into the nearest
                        // clipwindow ancestor's rect.
                        let clip_here = if eval.trait_value(child, "clips").truthy() {
                            child_clip
                        } else {
                            clip
                        };
                        out.push(DrawItem::Image {
                            tile: child,
                            rect,
                            filename,
                            crop: (num(eval, child, "cropx"), num(eval, child, "cropy")),
                            zoom: num(eval, child, "zoom"),
                            tiled: eval.trait_value(child, "tile").truthy(),
                            tint,
                            alpha,
                            clip: clip_here,
                        });
                    }
                }
                TileKind::Text => {
                    let string = match eval.trait_value(child, "string") {
                        Scalar::Str(s) => s,
                        Scalar::Num(n) => format_number(n),
                    };
                    if !string.is_empty() {
                        let clip_here = if eval.trait_value(child, "clips").truthy() {
                            child_clip
                        } else {
                            clip
                        };
                        out.push(DrawItem::Text {
                            tile: child,
                            x,
                            y,
                            string,
                            font: num(eval, child, "font").clamp(0.0, 255.0) as u8,
                            justify: num(eval, child, "justify").clamp(0.0, 2.0) as u8,
                            tint,
                            alpha,
                            wrap_width: num(eval, child, "wrapwidth").max(0.0),
                            wrap_lines: num(eval, child, "wraplines").max(0.0) as u32,
                            clip: clip_here,
                        });
                    }
                }
                TileKind::Rect => {
                    // Gamebryo rect tiles are hit targets and layout
                    // containers, never painted (the CS Wiki flags the
                    // solid-fill path as broken; vanilla's screen-sized
                    // grab zones at alpha 128 must stay invisible).
                }
                // NIF tiles are 3D models rendered by the scene system;
                // not part of the 2D composite. Logged once at load, not
                // per frame.
                TileKind::Nif | TileKind::Menu | TileKind::Template => {}
            }
        }

        walk_children(doc, eval, child, child_origin, child_clip, out, rect);
        let _ = parent_rect;
    }
}

/// Gamebryo's number→string formatting for `<string>` traits fed from
/// numeric copies (ammo counters, timers): integral values lose the
/// `.0`.
fn format_number(n: f32) -> String {
    if n.fract() == 0.0 && n.abs() < 1e9 {
        format!("{}", n as i64)
    } else {
        format!("{n}")
    }
}

fn intersect(a: Rect, b: Rect) -> Option<Rect> {
    let x = a.x.max(b.x);
    let y = a.y.max(b.y);
    let right = (a.x + a.w).min(b.x + b.w);
    let bottom = (a.y + a.h).min(b.y + b.h);
    if right > x && bottom > y {
        Some(Rect {
            x,
            y,
            w: right - x,
            h: bottom - y,
        })
    } else {
        None
    }
}

/// A fully-clipped-out window: keep a zero-area rect at the intersection
/// point so descendants draw nothing (rather than everything).
fn existing_zeroed(existing: Rect) -> Rect {
    Rect {
        x: existing.x,
        y: existing.y,
        w: 0.0,
        h: 0.0,
    }
}

/// Colour helpers shared with the rasterizer.
pub fn tint_u8(tint: [f32; 3]) -> [u8; 3] {
    [u8_color(tint[0]), u8_color(tint[1]), u8_color(tint[2])]
}
