//! Headless cell / worldspace catalogue (`--list-cells [FILTER]`).
//!
//! Answers "what can I actually pass to `--cell` / `--wrld` for this
//! game?" without launching a window. Before this existed the only
//! ways to find a loadable location were the three-entry
//! `sample_cells` list in `assets/debug_profiles.toml`, the
//! `probe_cells` plugin example (which needs a candidate name up
//! front — it validates, it doesn't enumerate), or guessing an EDID
//! and reading the loader's "close matches" error.
//!
//! Composes with the rest of the CLI: `--game <key>` expansion
//! supplies `--esm` / archives, and repeatable `--master <path>` is
//! honoured so DLC interiors show up under the same load order the
//! cell loader would build for them.
//!
//! ```text
//! byroredux --game skyrim_se --list-cells dragonsreach
//! byroredux --game fnv --list-cells            # everything
//! byroredux --esm Skyrim.esm --master Update.esm --list-cells whiterun
//! ```
//!
//! Interiors print `EDID`, REFR count, form ID and the localized
//! display name (FULL); worldspaces print `EDID`, form ID, the
//! streamed exterior cell count and the usable grid bounds, which is
//! the rectangle `--grid x,y` has to land inside.

use anyhow::Result;

/// Print every interior cell and worldspace across `plugin_paths`
/// (masters first, main ESM last), optionally narrowed to entries
/// whose editor ID or display name contains `filter`.
///
/// Never touches the World or the GPU: parses, prints to stdout and
/// returns, so `run()` can exit before any window exists.
pub fn run(plugin_paths: &[&str], filter: Option<&str>) -> Result<()> {
    let (index, load_order) = crate::cell_loader::parse_record_indexes_in_load_order(plugin_paths)?;

    let needle = filter.map(|f| f.to_ascii_lowercase());
    let matches = |edid: &str, display: Option<&str>| -> bool {
        match &needle {
            None => true,
            Some(n) => {
                edid.to_ascii_lowercase().contains(n.as_str())
                    || display.is_some_and(|d| d.to_ascii_lowercase().contains(n.as_str()))
            }
        }
    };

    println!("Load order: {}", load_order.join(" → "));

    // ---- Interior cells (`--cell <EDID>`) --------------------------
    let mut interiors: Vec<_> = index
        .cells
        .cells
        .values()
        .filter(|c| matches(&c.editor_id, c.display_name.as_deref()))
        .collect();
    interiors.sort_by(|a, b| {
        a.editor_id
            .to_ascii_lowercase()
            .cmp(&b.editor_id.to_ascii_lowercase())
    });

    println!(
        "\nInterior cells — pass to `--cell` ({} shown of {} total)",
        interiors.len(),
        index.cells.cells.len(),
    );
    for cell in &interiors {
        let display = cell
            .display_name
            .as_deref()
            .filter(|d| !d.is_empty() && !is_unresolved_lstring(d))
            .map(|d| format!("  \"{d}\""))
            .unwrap_or_default();
        println!(
            "  {:<40} {:>6} REFRs  form {:08X}{}",
            cell.editor_id,
            cell.references.len(),
            cell.form_id,
            display,
        );
    }

    // ---- Worldspaces (`--wrld <EDID> --grid x,y`) ------------------
    let mut worlds: Vec<_> = index
        .cells
        .worldspaces
        .values()
        .filter(|w| matches(&w.editor_id, None))
        .collect();
    worlds.sort_by(|a, b| {
        a.editor_id
            .to_ascii_lowercase()
            .cmp(&b.editor_id.to_ascii_lowercase())
    });

    println!(
        "\nWorldspaces — pass to `--wrld` with `--grid x,y` ({} shown of {} total)",
        worlds.len(),
        index.cells.worldspaces.len(),
    );
    for wrld in &worlds {
        // Exterior cell tables are keyed by lowercased worldspace EDID.
        let cell_count = index
            .cells
            .exterior_cells
            .get(&wrld.editor_id.to_ascii_lowercase())
            .map_or(0, |cells| cells.len());
        let bounds = match wrld.usable_cell_bounds() {
            Some(((min_x, min_y), (max_x, max_y))) => {
                format!("  grid {min_x},{min_y}..{max_x},{max_y}")
            }
            None => String::new(),
        };
        println!(
            "  {:<40} {:>6} cells  form {:08X}{}",
            wrld.editor_id, cell_count, wrld.form_id, bounds,
        );
    }

    if interiors.is_empty() && worlds.is_empty() {
        if let Some(f) = filter {
            println!("\nNothing matched '{f}'. Re-run without a filter for the full list.");
        }
    }
    Ok(())
}

/// A localized plugin's FULL sub-record holds a string-table ID, not
/// text; when the companion table can't be found the resolver hands back a
/// `<lstring 0xNNNNNNNN>` placeholder. Printing that would read as a name,
/// so drop it and leave the editor ID to identify the cell.
///
/// #3413 — this used to claim "Skyrim SE hits this for every cell", which
/// stopped being true when the archive fallback landed (#1553). `run` goes
/// through `parse_record_indexes_in_load_order`, which installs an
/// `ArchiveStringSource` and calls `StringTableSet::load_with_archive`, not
/// `::load`. That source matches `Skyrim - Interface.bsa` twice over — as
/// the `skyrim` stem's own plugin archive, and as a shared `" - interface"`
/// archive covering `Update.esm` / `Dawnguard.esm` / `HearthFires.esm` /
/// `Dragonborn.esm`, whose 138 `strings\…` entries all live there. The
/// real-data test `real_skyrim_load_order_preserves_categories_and_resolves_archive_strings`
/// pins it.
///
/// The placeholder is still reachable, so this helper still earns its keep:
/// a non-localized plugin, or a localized one whose table is genuinely
/// absent, produces it.
fn is_unresolved_lstring(display: &str) -> bool {
    display.starts_with("<lstring ") && display.ends_with('>')
}

#[cfg(test)]
mod tests {
    use super::is_unresolved_lstring;

    #[test]
    fn unresolved_lstring_placeholder_is_detected() {
        assert!(is_unresolved_lstring("<lstring 0x0000073A>"));
    }

    #[test]
    fn real_display_names_are_kept() {
        assert!(!is_unresolved_lstring("Goodsprings Schoolhouse"));
        assert!(!is_unresolved_lstring(""));
        // A name that merely mentions the marker isn't the placeholder.
        assert!(!is_unresolved_lstring("<lstring 0x1> Hall"));
    }
}
