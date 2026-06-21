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
use std::path::Path;

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
pub(super) fn plugin_for_form_id(form_id: u32, load_order: &[String]) -> Option<&str> {
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

/// #1553 / SK-D4-02 — load a Localized-flagged plugin's companion
/// `.STRINGS` / `.DLSTRINGS` / `.ILSTRINGS` tables and install them into
/// the thread-local string table for the duration of the returned guard.
///
/// Returns `None` (identity behaviour — placeholders survive) for a
/// non-localized plugin or an unreadable header. The loader + RAII guard
/// already existed (`esm::strings_table`); this is the missing wiring
/// that turns `<lstring 0xNNNNNNNN>` placeholders into authored names.
/// All three table kinds are covered by `StringTableSet::load`. The
/// guard MUST be held by the caller across the record walk so
/// `resolve_lstring` sees the tables, then dropped before the next plugin.
fn install_strings_guard(
    plugin_path: &str,
    plugin_data: &[u8],
    language: &str,
) -> Option<esm::StringsTableGuard> {
    let mut reader = esm::reader::EsmReader::new(plugin_data);
    if !reader.read_file_header().ok()?.localized {
        return None;
    }
    let tables = esm::StringTableSet::load(Path::new(plugin_path), language);
    Some(esm::StringsTableGuard::new(tables))
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
    // #1553 — companion `.STRINGS` language. Vanilla ships `english`;
    // a localized install (french / german / …) can override it. Read
    // once outside the loop.
    let strings_language =
        std::env::var("BYRO_STRINGS_LANG").unwrap_or_else(|_| "english".to_string());

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
        // #1553 — install this plugin's companion string tables for the
        // record walk so localized FULL/DESC/etc. lstring indices resolve
        // to authored names instead of `<lstring 0xNNNNNNNN>`. RAII guard:
        // alive across the parse, dropped before the next plugin so each
        // plugin sees only its own tables.
        let _strings_guard = install_strings_guard(path, &bytes, &strings_language);
        let plugin_records = esm::records::parse_esm_with_load_order(&bytes, Some(remap))
            .unwrap_or_else(|e| {
                log::warn!("Record parse failed for '{}': {}", path, e);
                esm::records::EsmIndex::default()
            });
        merged.merge_from(plugin_records);
    }
    Ok((merged, load_order))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// FO3+/TES5 24-byte-header record: `type + size + flags + form_id +
    /// 8-byte trailer`, then `[subtype, u16 len, data]` sub-records.
    fn build_record(typ: &[u8; 4], form_id: u32, subs: &[(&[u8; 4], Vec<u8>)]) -> Vec<u8> {
        let mut sub_data = Vec::new();
        for (st, data) in subs {
            sub_data.extend_from_slice(*st);
            sub_data.extend_from_slice(&(data.len() as u16).to_le_bytes());
            sub_data.extend_from_slice(data);
        }
        let mut buf = Vec::new();
        buf.extend_from_slice(typ);
        buf.extend_from_slice(&(sub_data.len() as u32).to_le_bytes());
        buf.extend_from_slice(&0u32.to_le_bytes()); // flags
        buf.extend_from_slice(&form_id.to_le_bytes());
        buf.extend_from_slice(&[0u8; 8]); // trailer
        buf.extend_from_slice(&sub_data);
        buf
    }

    fn wrap_group(label: &[u8; 4], record: &[u8]) -> Vec<u8> {
        let total = 24 + record.len();
        let mut buf = Vec::new();
        buf.extend_from_slice(b"GRUP");
        buf.extend_from_slice(&(total as u32).to_le_bytes());
        buf.extend_from_slice(label);
        buf.extend_from_slice(&0u32.to_le_bytes()); // group_type = top
        buf.extend_from_slice(&[0u8; 8]);
        buf.extend_from_slice(record);
        buf
    }

    /// TES4 with the Localized flag (0x80) + a Skyrim HEDR version.
    fn build_localized_tes4() -> Vec<u8> {
        let mut hedr = Vec::new();
        hedr.extend_from_slice(b"HEDR");
        hedr.extend_from_slice(&12u16.to_le_bytes());
        hedr.extend_from_slice(&1.7f32.to_le_bytes()); // Skyrim
        hedr.extend_from_slice(&0u32.to_le_bytes());
        hedr.extend_from_slice(&0u32.to_le_bytes());
        let mut buf = Vec::new();
        buf.extend_from_slice(b"TES4");
        buf.extend_from_slice(&(hedr.len() as u32).to_le_bytes());
        buf.extend_from_slice(&0x80u32.to_le_bytes()); // Localized
        buf.extend_from_slice(&0u32.to_le_bytes());
        buf.extend_from_slice(&[0u8; 8]);
        buf.extend_from_slice(&hedr);
        buf
    }

    /// Synthetic `.STRINGS`: `[count][data_size][id,offset…][blob]` with
    /// bare null-terminated strings (no length prefix).
    fn build_strings_file(entries: &[(u32, &str)]) -> Vec<u8> {
        let mut blob = Vec::new();
        let mut offsets = Vec::new();
        for (_, s) in entries {
            offsets.push(blob.len() as u32);
            blob.extend_from_slice(s.as_bytes());
            blob.push(0);
        }
        let mut out = Vec::new();
        out.extend_from_slice(&(entries.len() as u32).to_le_bytes());
        out.extend_from_slice(&(blob.len() as u32).to_le_bytes());
        for (i, (id, _)) in entries.iter().enumerate() {
            out.extend_from_slice(&id.to_le_bytes());
            out.extend_from_slice(&offsets[i].to_le_bytes());
        }
        out.extend_from_slice(&blob);
        out
    }

    /// Localized WEAP (FULL = lstring id 0x0001) wrapped in a TES4-headed
    /// plugin written to `dir/<stem>.esm`. Returns the path.
    fn write_localized_weap_plugin(dir: &Path, stem: &str) -> std::path::PathBuf {
        let mut weap_subs = Vec::<(&[u8; 4], Vec<u8>)>::new();
        weap_subs.push((b"EDID", b"TestBlade\0".to_vec()));
        weap_subs.push((b"FULL", 0x0001u32.to_le_bytes().to_vec()));
        weap_subs.push((b"DATA", {
            let mut d = Vec::new();
            d.extend_from_slice(&100u32.to_le_bytes()); // value
            d.extend_from_slice(&0u32.to_le_bytes()); // health
            d.extend_from_slice(&1.5f32.to_le_bytes()); // weight
            d.extend_from_slice(&15u16.to_le_bytes()); // damage
            d.push(0);
            d.push(0);
            d
        }));
        let weap = build_record(b"WEAP", 0xBEEF, &weap_subs);
        let group = wrap_group(b"WEAP", &weap);
        let mut esm_bytes = build_localized_tes4();
        esm_bytes.extend_from_slice(&group);
        let path = dir.join(format!("{stem}.esm"));
        fs::write(&path, &esm_bytes).unwrap();
        path
    }

    /// #1553 / SK-D4-02 — end-to-end wiring: a Localized plugin on disk
    /// with a sibling `Strings/<stem>_english.STRINGS` must resolve its
    /// FULL lstring indices to authored names through
    /// `parse_record_indexes_in_load_order`. Pre-fix the loader + guard
    /// existed but were never wired, so every localized name stayed a
    /// `<lstring 0x…>` placeholder.
    #[test]
    fn localized_plugin_resolves_names_through_load_order() {
        let dir = tempfile::tempdir().unwrap();
        let stem = "TestPlugin";
        let esm_path = write_localized_weap_plugin(dir.path(), stem);

        let strings_dir = dir.path().join("Strings");
        fs::create_dir(&strings_dir).unwrap();
        fs::write(
            strings_dir.join(format!("{stem}_english.STRINGS")),
            build_strings_file(&[(0x0001, "Iron Sword")]),
        )
        .unwrap();

        let path_str = esm_path.to_str().unwrap();
        let (index, _order) = parse_record_indexes_in_load_order(&[path_str]).unwrap();
        let item = index.items.get(&0xBEEF).expect("WEAP indexed");
        assert_eq!(
            item.common.full_name, "Iron Sword",
            "the load-order wiring must install the .STRINGS guard so the \
             FULL lstring resolves (not the <lstring 0x…> placeholder)"
        );
    }

    /// Control: the SAME Localized plugin WITHOUT the companion file keeps
    /// the placeholder — proving the resolution above came from the wired
    /// guard reading the on-disk table, not some other path.
    #[test]
    fn localized_plugin_without_strings_keeps_placeholder() {
        let dir = tempfile::tempdir().unwrap();
        let esm_path = write_localized_weap_plugin(dir.path(), "NoStrings");

        let path_str = esm_path.to_str().unwrap();
        let (index, _order) = parse_record_indexes_in_load_order(&[path_str]).unwrap();
        let item = index.items.get(&0xBEEF).expect("WEAP indexed");
        assert_eq!(item.common.full_name, "<lstring 0x00000001>");
    }
}
