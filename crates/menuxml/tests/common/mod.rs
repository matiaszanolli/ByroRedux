//! Real-data resolution shared by the vanilla menu-corpus tests.
//!
//! #4660 — each corpus file used to resolve its own install: an override was
//! read but never checked, and every archive went through
//! `BsaArchive::open(..).ok()?`, so a broken archive reader read as "data not
//! found; skipping". These follow the #3850 contract every other real-data
//! harness uses: a set override is binding, an absent install or archive fails
//! under `BYROREDUX_REQUIRE_GAME_DATA`, and a present archive that fails to
//! open is always a failure.

use byroredux_bsa::BsaArchive;
use std::path::{Path, PathBuf};

fn strict() -> bool {
    std::env::var("BYROREDUX_REQUIRE_GAME_DATA").is_ok_and(|v| v != "0")
}

/// `env_var` if set (and it must name a directory), else `default` when it
/// exists; `None` means skip, and is never returned under the strict lane.
#[track_caller]
pub fn data_dir(env_var: &str, default: &str) -> Option<PathBuf> {
    if let Some(v) = std::env::var(env_var).ok().filter(|s| !s.is_empty()) {
        let p = PathBuf::from(&v);
        assert!(p.is_dir(), "{env_var} points to {v:?}, which is not a directory");
        return Some(p);
    }
    let p = PathBuf::from(default);
    if p.is_dir() {
        return Some(p);
    }
    if strict() {
        panic!(
            "BYROREDUX_REQUIRE_GAME_DATA is set, but no game data was found: \
             {env_var} is unset and the default {p:?} is not a directory"
        );
    }
    eprintln!("skipping: {env_var} unset and {p:?} missing");
    None
}

/// Open `dir/name`; `None` (skip) only when the file is absent outside the
/// strict lane.
#[track_caller]
pub fn open_archive(dir: &Path, name: &str) -> Option<BsaArchive> {
    let path = dir.join(name);
    if !path.is_file() {
        if strict() {
            panic!("BYROREDUX_REQUIRE_GAME_DATA is set, but {path:?} is not a file");
        }
        eprintln!("skipping: {path:?} not found");
        return None;
    }
    Some(BsaArchive::open(&path).unwrap_or_else(|e| panic!("failed to open {path:?}: {e}")))
}

/// #6994 — how many of `font`'s printable ASCII glyphs (`!`..`~`) cut a clean
/// rect out of the atlas, as `(clean, inked)`. Vanilla atlases pad every glyph
/// cell, so a rect read at the right place has no ink (alpha > 64) on its edge
/// pixels; a rect read with its axes or extents crossed cuts through
/// neighbouring glyphs and carries ink on its edges.
#[allow(dead_code)] // each corpus file is its own test crate; not every one calls every helper
pub fn clean_glyph_rects(font: &byroredux_menuxml::font::Font) -> (usize, usize) {
    let (w, h) = (font.atlas.width as f32, font.atlas.height as f32);
    let alpha = |x: i64, y: i64| -> u8 {
        if x < 0 || y < 0 || x >= font.atlas.width as i64 || y >= font.atlas.height as i64 {
            return 0;
        }
        font.atlas.pixels[(y as usize * font.atlas.width as usize + x as usize) * 4 + 3]
    };
    let (mut clean, mut inked) = (0, 0);
    for ch in b'!'..=b'~' {
        let g = font.glyph(ch);
        if !g.inked {
            continue;
        }
        inked += 1;
        let (x0, y0) = ((g.u0 * w).round() as i64, (g.v0 * h).round() as i64);
        let (x1, y1) = ((g.u1 * w).round() as i64 - 1, (g.v1 * h).round() as i64 - 1);
        if x1 < x0 || y1 < y0 {
            continue;
        }
        let edge_ink = (x0..=x1).any(|x| alpha(x, y0) > 64 || alpha(x, y1) > 64)
            || (y0..=y1).any(|y| alpha(x0, y) > 64 || alpha(x1, y) > 64);
        if !edge_ink {
            clean += 1;
        }
    }
    (clean, inked)
}

/// #6994 — inked printable glyphs whose metric width / height differ by more
/// than one texel from the extent of the atlas rect they are drawn from. A
/// correct reading of the `.fnt` record makes the two agree.
#[allow(dead_code)] // each corpus file is its own test crate; not every one calls every helper
pub fn glyph_extent_mismatches(font: &byroredux_menuxml::font::Font) -> usize {
    let (w, h) = (font.atlas.width as f32, font.atlas.height as f32);
    (b'!'..=b'~')
        .map(|ch| font.glyph(ch))
        .filter(|g| g.inked)
        .filter(|g| ((g.u1 - g.u0) * w - g.width).abs() > 1.0 || ((g.v1 - g.v0) * h - g.height).abs() > 1.0)
        .count()
}
