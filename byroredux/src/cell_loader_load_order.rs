//! Multi-plugin load-order helpers.
//!
//! Plugin FormIDs use the top byte as an index into the *plugin's own*
//! `MASTERS` list. To resolve cross-plugin REFRs (a Dawnguard interior
//! placing a Skyrim.esm STAT) we need a global load order — the
//! `FormIdRemap` produced by [`build_remap_for_plugin`] rewrites every
//! local top-byte into its global load-order index before the
//! per-plugin record tables merge into a single [`esm::records::EsmIndex`].
//!
//! See M46.0 / #561 / #445 for the multi-plugin landing.

use byroredux_plugin::esm;

/// Lowercase basename of a plugin path. Used as the global load-order
/// key (case-insensitive on Bethesda content).
pub(super) fn plugin_basename_lc(path: &str) -> String {
    std::path::Path::new(path)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(path)
        .to_ascii_lowercase()
}

/// Resolve a FormID's mod-index byte to the owning plugin's basename.
/// Used by the loud-fail diagnostic when a REFR's `base_form_id` is
/// unresolved — the audit's #561 completeness item: "name the missing
/// master" instead of silently rendering empty.
pub(super) fn plugin_for_form_id<'a>(form_id: u32, load_order: &'a [String]) -> Option<&'a str> {
    let mod_index = (form_id >> 24) as usize;
    load_order.get(mod_index).map(|s| s.as_str())
}

/// Build the [`FormIdRemap`] that turns this plugin's local FormIDs
/// (top byte = mod-index in its own MASTERS list) into globally
/// load-order-indexed FormIDs (top byte = position in the load order
/// passed to the cell loader).
///
/// Returns `Err` when the plugin declares a master that isn't in the
/// global load order — that's a load-order misconfiguration the caller
/// must fix (every declared master must be present and earlier).
pub(super) fn build_remap_for_plugin(
    plugin_path: &str,
    plugin_data: &[u8],
    plugin_index: usize,
    load_order: &[String],
) -> anyhow::Result<esm::reader::FormIdRemap> {
    let mut reader = esm::reader::EsmReader::new(plugin_data);
    let header = reader
        .read_file_header()
        .map_err(|e| anyhow::anyhow!("Failed to read TES4 header for '{}': {}", plugin_path, e))?;

    let master_indices: Vec<u8> = header
        .master_files
        .iter()
        .map(|m| {
            let m_lc = m.to_ascii_lowercase();
            load_order
                .iter()
                .position(|name| name == &m_lc)
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "Plugin '{}' declares master '{}' which is not in the load order — \
                         pass `--master {}` before `--esm`",
                        plugin_path,
                        m,
                        m,
                    )
                })
                .map(|i| i as u8)
        })
        .collect::<anyhow::Result<Vec<u8>>>()?;

    Ok(esm::reader::FormIdRemap {
        plugin_index: plugin_index as u8,
        master_indices,
    })
}

/// Parse a sequence of plugins in load order (masters first, main
/// plugin last) and return a single merged [`esm::records::EsmIndex`]
/// plus the lowercased load-order list.
///
/// Uses the full `parse_esm_with_load_order` walker so the broader
/// `EsmIndex` (climates, weathers, items, NPCs, …) is available
/// alongside the cell tables. Exterior loads need this for the
/// `wrld → CLMT` and `CELL → WTHR` resolution paths the renderer's
/// day-night arc consumes.
///
/// The retired cell-only variant (`parse_cell_indexes_in_load_order`)
/// was removed in SK-D6-02 / #566 once interior cell loads switched to
/// the full record walker so the LGTM lighting-template fallback can
/// resolve through `EsmIndex.lighting_templates`.
pub(super) fn parse_record_indexes_in_load_order(
    plugin_paths: &[&str],
) -> anyhow::Result<(esm::records::EsmIndex, Vec<String>)> {
    let load_order: Vec<String> = plugin_paths.iter().map(|p| plugin_basename_lc(p)).collect();
    {
        let mut seen = std::collections::HashSet::with_capacity(load_order.len());
        for name in &load_order {
            if !seen.insert(name) {
                return Err(anyhow::anyhow!(
                    "Plugin '{}' appears twice in the load order — \
                     a plugin can only be passed once",
                    name
                ));
            }
        }
    }
    let mut merged = esm::records::EsmIndex::default();
    for (idx, path) in plugin_paths.iter().enumerate() {
        let bytes = std::fs::read(path)
            .map_err(|e| anyhow::anyhow!("Failed to read ESM '{}': {}", path, e))?;
        log::info!(
            "Parsing plugin {}/{} '{}' ({:.1} MB) at load-order index {}…",
            idx + 1,
            plugin_paths.len(),
            path,
            bytes.len() as f64 / 1_048_576.0,
            idx,
        );
        let remap = build_remap_for_plugin(path, &bytes, idx, &load_order)?;
        let plugin_records = esm::records::parse_esm_with_load_order(&bytes, Some(remap))
            .unwrap_or_else(|e| {
                log::warn!("Record parse failed for '{}': {}", path, e);
                esm::records::EsmIndex::default()
            });
        merged.merge_from(plugin_records);
    }
    Ok((merged, load_order))
}
