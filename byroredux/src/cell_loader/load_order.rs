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

use crate::asset_provider::Archive;
use byroredux_plugin::esm;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Lowercase basename of a plugin path. Used as the global load-order
/// key (case-insensitive on Bethesda content).
pub(super) fn plugin_basename_lc(path: &str) -> String {
    std::path::Path::new(path)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(path)
        .to_ascii_lowercase()
}

/// Load-order plugin basenames paired with the global slot each was assigned.
///
/// The two are *not* interchangeable (#3366). `allocate_global_slot` draws from
/// two independent counters: regular plugins take `0x00..=0xFD` from
/// `next_regular`, light masters take a 12-bit sub-index in the `0xFE` space
/// from `next_light`. So a plugin's load-order **position** equals its slot byte
/// only while no ESL precedes a regular plugin. Both vectors are indexed by
/// load-order position and stay parallel; `slots[pos]` is that plugin's slot.
///
/// Derefs to the name slice so the many pass-through call sites that only need
/// `&[String]` are unaffected.
pub(crate) struct LoadOrder {
    names: Vec<String>,
    slots: Vec<esm::reader::GlobalSlot>,
}

impl LoadOrder {
    pub(crate) fn new(names: Vec<String>, slots: Vec<esm::reader::GlobalSlot>) -> Self {
        debug_assert_eq!(
            names.len(),
            slots.len(),
            "load-order names and slots must stay parallel"
        );
        Self { names, slots }
    }

    /// Build an order in which every plugin is a regular master taking a
    /// sequential slot — the case where load-order position and slot byte
    /// coincide. Test convenience; the real order comes from
    /// [`parse_record_indexes_in_load_order`].
    #[cfg(test)]
    pub(crate) fn all_regular(names: Vec<String>) -> Self {
        let slots = (0..names.len())
            .map(|i| esm::reader::GlobalSlot::Regular(i as u8))
            .collect();
        Self::new(names, slots)
    }
}

impl Default for LoadOrder {
    /// Empty order — used by test fixtures and by the synthetic
    /// `ExteriorWorldContext`s that never resolve a FormID to a plugin.
    fn default() -> Self {
        Self {
            names: Vec::new(),
            slots: Vec::new(),
        }
    }
}

impl std::ops::Deref for LoadOrder {
    type Target = [String];
    fn deref(&self) -> &Self::Target {
        &self.names
    }
}

/// Resolve a global FormID to the owning plugin's basename.
/// Used by the loud-fail diagnostic when a REFR's `base_form_id` is
/// unresolved — the audit's #561 completeness item: "name the missing
/// master" instead of silently rendering empty.
///
/// #3366 — this used to index `load_order` by the FormID's top byte, treating
/// it as a load-order *position*. Those coincide only when no ESL precedes a
/// regular plugin: an ESL anywhere but last shifts every later regular plugin's
/// position past its slot byte, so the diagnostic named the wrong plugin, and
/// an ESL-owned form (top byte `0xFE` = 254) fell off the end of the list and
/// reported `None` — rendered by callers as `"???"` / `"Engine.esm"`. Measured
/// on a legal 5-plugin order with `_ResourcePack.esl` third, every
/// Dragonborn-owned `DLC2*` static was attributed to the ESL.
///
/// Decode the FormID into a [`GlobalSlot`] first — the exact inverse of
/// [`GlobalSlot::compose`] — then find the plugin holding that slot. The
/// remap that actually places geometry was never affected: it looks masters up
/// by position and reads `slots[pos]`, keeping the two in step.
pub(super) fn plugin_for_form_id(form_id: u32, load_order: &LoadOrder) -> Option<&str> {
    let slot = global_slot_of(form_id);
    let position = load_order.slots.iter().position(|s| *s == slot)?;
    load_order.names.get(position).map(|s| s.as_str())
}

/// Inverse of [`esm::reader::GlobalSlot::compose`]: which slot owns this global
/// FormID. `0xFE` is the light-master space, where the owner is the 12 bits
/// below the top byte; anything else is a full-byte regular slot.
fn global_slot_of(form_id: u32) -> esm::reader::GlobalSlot {
    const LIGHT_MASTER_BYTE: u32 = 0xFE;
    if (form_id >> 24) == LIGHT_MASTER_BYTE {
        esm::reader::GlobalSlot::Light(((form_id >> 12) & 0x0FFF) as u16)
    } else {
        esm::reader::GlobalSlot::Regular((form_id >> 24) as u8)
    }
}

/// Build the [`FormIdRemap`] that turns this plugin's local FormIDs
/// (top byte = mod-index in its own MASTERS list) into globally
/// load-order-resolved FormIDs.
///
/// `plugin_slot` is this plugin's already-assigned global slot;
/// `slots` holds every plugin's slot indexed by load-order position, so
/// each master resolves to its own [`GlobalSlot`] — regular or ESL
/// (#1554). Masters always precede their dependents, so their slots are
/// assigned before this call.
///
/// Returns `Err` when the plugin declares a master that isn't in the
/// global load order (or loads after it) — a load-order misconfiguration
/// the caller must fix (every declared master must be present and earlier).
pub(super) fn build_remap_for_plugin(
    plugin_path: &str,
    header: &esm::reader::FileHeader,
    plugin_slot: esm::reader::GlobalSlot,
    load_order: &[String],
    slots: &[esm::reader::GlobalSlot],
) -> anyhow::Result<esm::reader::FormIdRemap> {
    let master_slots: Vec<esm::reader::GlobalSlot> = header
        .master_files
        .iter()
        .map(|m| {
            let m_lc = m.to_ascii_lowercase();
            let pos = load_order
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
                })?;
            // Masters load before their dependents, so the slot is already
            // assigned. A master listed AFTER this plugin is a misordered
            // load order — fail loudly rather than index unassigned slots.
            slots.get(pos).copied().ok_or_else(|| {
                anyhow::anyhow!(
                    "Plugin '{}' declares master '{}' which loads after it — \
                     masters must come first in the load order",
                    plugin_path,
                    m,
                )
            })
        })
        .collect::<anyhow::Result<Vec<_>>>()?;

    Ok(esm::reader::FormIdRemap {
        plugin_slot,
        master_slots,
    })
}

/// #1553 / SK-D4-02 — load a Localized plugin's companion `.STRINGS` /
/// `.DLSTRINGS` / `.ILSTRINGS` tables and install them into the
/// thread-local string table for the duration of the returned guard.
///
/// `localized` is the caller's already-read TES4 `0x80` flag. Returns
/// `None` (identity behaviour — placeholders survive) for a non-localized
/// plugin. The loader + RAII guard already existed (`esm::strings_table`);
/// this is the missing wiring that turns `<lstring 0xNNNNNNNN>`
/// placeholders into authored names. All three table kinds are covered by
/// `StringTableSet::load`. The guard MUST be held by the caller across the
/// record walk so `resolve_lstring` sees the tables, then dropped before
/// the next plugin.
fn install_strings_guard<F>(
    localized: bool,
    plugin_path: &str,
    language: &str,
    read_archive: &mut F,
) -> Option<esm::StringsTableGuard>
where
    F: FnMut(&Path, &str) -> Option<Vec<u8>>,
{
    if !localized {
        return None;
    }
    let plugin_path = Path::new(plugin_path);
    let tables = esm::StringTableSet::load_with_archive(plugin_path, language, |relative_path| {
        read_archive(plugin_path, relative_path)
    });
    Some(esm::StringsTableGuard::new(tables))
}

/// Lazily opened archive set used only for localized companion strings.
/// The plugin crate remains archive-agnostic; this engine boundary owns
/// BSA/BA2 discovery and extraction.
#[derive(Default)]
struct ArchiveStringSource {
    by_plugin: HashMap<PathBuf, Vec<Archive>>,
}

impl ArchiveStringSource {
    fn read(&mut self, plugin_path: &Path, relative_path: &str) -> Option<Vec<u8>> {
        let archives = self
            .by_plugin
            .entry(plugin_path.to_path_buf())
            .or_insert_with(|| Self::discover(plugin_path));
        archives
            .iter()
            .find_map(|archive| archive.extract(relative_path).ok())
    }

    fn discover(plugin_path: &Path) -> Vec<Archive> {
        let directory = plugin_path.parent().unwrap_or(Path::new("."));
        let plugin_stem = plugin_path
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .to_ascii_lowercase();
        let mut candidates: Vec<(u8, PathBuf)> = std::fs::read_dir(directory)
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|entry| {
                let path = entry.path();
                let extension = path.extension()?.to_string_lossy();
                if !extension.eq_ignore_ascii_case("bsa") && !extension.eq_ignore_ascii_case("ba2")
                {
                    return None;
                }
                let stem = path.file_stem()?.to_string_lossy().to_ascii_lowercase();
                let plugin_archive = stem == plugin_stem
                    || stem.strip_prefix(&plugin_stem).is_some_and(|suffix| {
                        suffix.starts_with(" - main")
                            || suffix.starts_with(" - interface")
                            || suffix.starts_with(" - localization")
                            || suffix.starts_with(" - strings")
                    });
                let shared_archive = stem.ends_with(" - interface")
                    || stem.ends_with(" - localization")
                    || stem.ends_with(" - strings");
                (plugin_archive || shared_archive)
                    .then_some((if plugin_archive { 0 } else { 1 }, path))
            })
            .collect();
        candidates.sort_by(|(priority_a, path_a), (priority_b, path_b)| {
            priority_a.cmp(priority_b).then_with(|| path_a.cmp(path_b))
        });
        candidates
            .into_iter()
            .filter_map(|(_, path)| match Archive::open(&path.to_string_lossy()) {
                Ok(archive) => Some(archive),
                Err(error) => {
                    log::warn!("failed to open localized-strings archive: {error}");
                    None
                }
            })
            .collect()
    }
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
pub(crate) fn parse_record_indexes_in_load_order(
    plugin_paths: &[&str],
) -> anyhow::Result<(esm::records::EsmIndex, LoadOrder)> {
    let mut archive_source = ArchiveStringSource::default();
    parse_record_indexes_in_load_order_with_archive(plugin_paths, |plugin_path, relative_path| {
        archive_source.read(plugin_path, relative_path)
    })
}

fn parse_record_indexes_in_load_order_with_archive<F>(
    plugin_paths: &[&str],
    mut read_archive: F,
) -> anyhow::Result<(esm::records::EsmIndex, LoadOrder)>
where
    F: FnMut(&Path, &str) -> Option<Vec<u8>>,
{
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
    // #1554 — global-slot assignment. Regular plugins consume a full
    // top-byte slot (0x00–0xFD); ESL / light-master plugins (TES4 0x0200)
    // share the 0xFE byte via a 12-bit sub-index. Masters precede their
    // dependents, so a single forward pass assigns every slot before it's
    // referenced.
    let mut slots: Vec<esm::reader::GlobalSlot> = Vec::with_capacity(plugin_paths.len());
    let mut next_regular: u16 = 0;
    let mut next_light: u16 = 0;

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

        // Read the TES4 header once: masters (for the remap), the ESL
        // flag (slot assignment, #1554), and the Localized flag (the
        // .STRINGS guard, #1553).
        let header = {
            let mut reader = esm::reader::EsmReader::new(&bytes);
            reader
                .read_file_header()
                .map_err(|e| anyhow::anyhow!("Failed to read TES4 header for '{}': {}", path, e))?
        };

        let plugin_slot =
            allocate_global_slot(header.light_master, &mut next_regular, &mut next_light)?;
        slots.push(plugin_slot);

        let remap = build_remap_for_plugin(path, &header, plugin_slot, &load_order, &slots)?;
        // #1553 — install this plugin's companion string tables for the
        // record walk so localized FULL/DESC/etc. lstring indices resolve
        // to authored names instead of `<lstring 0xNNNNNNNN>`. RAII guard:
        // alive across the parse, dropped before the next plugin so each
        // plugin sees only its own tables.
        let _strings_guard =
            install_strings_guard(header.localized, path, &strings_language, &mut read_archive);
        let plugin_records = esm::records::parse_esm_with_load_order(&bytes, Some(remap))
            .unwrap_or_else(|e| {
                log::warn!("Record parse failed for '{}': {}", path, e);
                esm::records::EsmIndex::default()
            });
        merged.merge_from(plugin_records);
    }
    Ok((merged, LoadOrder::new(load_order, slots)))
}

/// Allocate one global load-order slot without ever entering reserved or
/// truncated FormID space. Regular plugins own `0x00..=0xFD`; light masters
/// share `0xFE` through a 12-bit `0x000..=0xFFF` sub-index.
fn allocate_global_slot(
    light_master: bool,
    next_regular: &mut u16,
    next_light: &mut u16,
) -> anyhow::Result<esm::reader::GlobalSlot> {
    if light_master {
        const MAX_LIGHT_SLOT: u16 = 0x0FFF;
        if *next_light > MAX_LIGHT_SLOT {
            return Err(anyhow::anyhow!(
                "Load order exceeds the 4096 light-master slot limit"
            ));
        }
        let slot = esm::reader::GlobalSlot::Light(*next_light);
        *next_light = next_light
            .checked_add(1)
            .ok_or_else(|| anyhow::anyhow!("Light-master slot counter overflow"))?;
        Ok(slot)
    } else {
        const MAX_REGULAR_SLOT: u16 = 0x00FD;
        if *next_regular > MAX_REGULAR_SLOT {
            return Err(anyhow::anyhow!(
                "Load order exceeds the 254 regular-plugin slot limit"
            ));
        }
        let slot = esm::reader::GlobalSlot::Regular(*next_regular as u8);
        *next_regular = next_regular
            .checked_add(1)
            .ok_or_else(|| anyhow::anyhow!("Regular-plugin slot counter overflow"))?;
        Ok(slot)
    }
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

    /// TES4 with explicit record `flags` (0x80 = Localized, 0x0200 = ESL)
    /// + a Skyrim HEDR version.
    fn build_tes4(flags: u32) -> Vec<u8> {
        let mut hedr = Vec::new();
        hedr.extend_from_slice(b"HEDR");
        hedr.extend_from_slice(&12u16.to_le_bytes());
        hedr.extend_from_slice(&1.7f32.to_le_bytes()); // Skyrim
        hedr.extend_from_slice(&0u32.to_le_bytes());
        hedr.extend_from_slice(&0u32.to_le_bytes());
        let mut buf = Vec::new();
        buf.extend_from_slice(b"TES4");
        buf.extend_from_slice(&(hedr.len() as u32).to_le_bytes());
        buf.extend_from_slice(&flags.to_le_bytes());
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

    /// A single-WEAP plugin (FULL = lstring id 0x0001) with TES4 `flags`
    /// and the WEAP at raw `weap_form_id`, written to `dir/<stem>.esm`.
    /// Returns the path.
    fn write_weap_plugin(
        dir: &Path,
        stem: &str,
        flags: u32,
        weap_form_id: u32,
    ) -> std::path::PathBuf {
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
        let weap = build_record(b"WEAP", weap_form_id, &weap_subs);
        let group = wrap_group(b"WEAP", &weap);
        let mut esm_bytes = build_tes4(flags);
        esm_bytes.extend_from_slice(&group);
        let path = dir.join(format!("{stem}.esm"));
        fs::write(&path, &esm_bytes).unwrap();
        path
    }

    #[test]
    fn regular_slot_allocator_rejects_the_255th_plugin() {
        let mut next_regular = 0;
        let mut next_light = 0;
        for expected in 0u16..254 {
            assert_eq!(
                allocate_global_slot(false, &mut next_regular, &mut next_light).unwrap(),
                esm::reader::GlobalSlot::Regular(expected as u8),
            );
        }
        let err = allocate_global_slot(false, &mut next_regular, &mut next_light)
            .expect_err("0xFE is reserved for light masters");
        assert!(err.to_string().contains("254 regular-plugin"));
    }

    #[test]
    fn light_slot_allocator_rejects_the_4097th_plugin() {
        let mut next_regular = 0;
        let mut next_light = 0;
        for expected in 0u16..4096 {
            assert_eq!(
                allocate_global_slot(true, &mut next_regular, &mut next_light).unwrap(),
                esm::reader::GlobalSlot::Light(expected),
            );
        }
        let err = allocate_global_slot(true, &mut next_regular, &mut next_light)
            .expect_err("light sub-index is 12-bit");
        assert!(err.to_string().contains("4096 light-master"));
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
        let esm_path = write_weap_plugin(dir.path(), stem, 0x80, 0xBEEF);

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

    /// #2912 — shipping installs keep localized tables in BSA/BA2 archives.
    /// The load-order path must consult that source when no loose override is
    /// present, without teaching the plugin parser about archive formats.
    #[test]
    fn localized_plugin_resolves_names_from_archive_source() {
        let dir = tempfile::tempdir().unwrap();
        let stem = "PackedPlugin";
        let esm_path = write_weap_plugin(dir.path(), stem, 0x80, 0xBEEF);
        let packed = build_strings_file(&[(0x0001, "Packed Sword")]);
        let mut requested = Vec::new();

        let path_str = esm_path.to_str().unwrap();
        let (index, _order) = parse_record_indexes_in_load_order_with_archive(
            &[path_str],
            |_plugin_path, relative_path| {
                requested.push(relative_path.to_owned());
                (relative_path == r"strings\PackedPlugin_english.STRINGS").then(|| packed.clone())
            },
        )
        .unwrap();

        assert_eq!(index.items[&0xBEEF].common.full_name, "Packed Sword");
        assert!(requested.contains(&r"strings\PackedPlugin_english.STRINGS".to_owned()));
    }

    /// Control: the SAME Localized plugin WITHOUT the companion file keeps
    /// the placeholder — proving the resolution above came from the wired
    /// guard reading the on-disk table, not some other path.
    #[test]
    fn localized_plugin_without_strings_keeps_placeholder() {
        let dir = tempfile::tempdir().unwrap();
        let esm_path = write_weap_plugin(dir.path(), "NoStrings", 0x80, 0xBEEF);

        let path_str = esm_path.to_str().unwrap();
        let (index, _order) = parse_record_indexes_in_load_order(&[path_str]).unwrap();
        let item = index.items.get(&0xBEEF).expect("WEAP indexed");
        assert_eq!(item.common.full_name, "<lstring 0x00000001>");
    }

    /// #2907 + #2912 — validate both load-order folding and archive-backed
    /// localization against the shipped Skyrim master.
    #[test]
    #[ignore]
    fn real_skyrim_load_order_preserves_categories_and_resolves_archive_strings() {
        let data = std::env::var("BYROREDUX_SKYRIMSE_DATA")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|_| {
                std::path::PathBuf::from(
                    "/mnt/data/SteamLibrary/steamapps/common/Skyrim Special Edition/Data",
                )
            });
        if !data.is_dir() {
            eprintln!("[Skyrim load-order] skipping: game data unavailable");
            return;
        }
        let master = data.join("Skyrim.esm");
        let bytes = std::fs::read(&master).expect("read Skyrim.esm");
        let direct = esm::records::parse_esm(&bytes).expect("parse Skyrim.esm directly");
        let master_str = master.to_str().unwrap();
        let (merged, _) = parse_record_indexes_in_load_order(&[master_str]).unwrap();

        assert_eq!(merged.total(), direct.total());
        assert_eq!(merged.idle_animations.len(), direct.idle_animations.len());
        assert!(merged.idle_animations.len() > 3_000);

        let names: Vec<_> = merged
            .items
            .values()
            .map(|item| item.common.full_name.as_str())
            .filter(|name| !name.is_empty())
            .collect();
        assert!(names.len() > 1_000, "too few resolved Skyrim item names");
        assert!(
            names.iter().all(|name| !name.starts_with("<lstring ")),
            "archive-backed tables must resolve every non-empty item name"
        );
    }

    /// #1554 / SK-D4-03 — end-to-end: an ESL-flagged (TES4 0x0200) plugin's
    /// own forms must land in the 0xFE light space through the load-order
    /// path, NOT at a flat top-byte index. The single ESL with no masters
    /// gets light sub-index 0, so a self-ref WEAP at raw 0x0000_0800
    /// resolves to 0xFE00_0800 in the merged index. Pre-fix the 0x0200
    /// flag was never read and the form kept its raw 0x0000_0800 id.
    #[test]
    fn esl_plugin_own_forms_land_in_light_space() {
        let dir = tempfile::tempdir().unwrap();
        // Self-ref object id 0x800 (within the 12-bit ESL object range).
        let esm_path = write_weap_plugin(dir.path(), "EslMod", 0x0200, 0x0000_0800);

        let path_str = esm_path.to_str().unwrap();
        let (index, _order) = parse_record_indexes_in_load_order(&[path_str]).unwrap();
        assert!(
            index.items.contains_key(&0xFE00_0800),
            "ESL self-ref form must remap into the 0xFE light space (sub-index 0), \
             got keys: {:?}",
            index.items.keys().collect::<Vec<_>>()
        );
        assert!(
            !index.items.contains_key(&0x0000_0800),
            "the raw pre-remap id must not survive — that's the #1554 bug"
        );
    }

    // ── EX-09/17 item 8 (#2370) — load-order conformance fixtures ──────
    //
    // The synthetic fixtures above exercise a single plugin (or two
    // disjoint statics); `crates/plugin/src/esm/cell/tests/merge.rs`
    // exercises `EsmCellIndex::merge_from` directly with hand-built
    // `CellData`/`WorldspaceRecord` values — real coverage of the merge
    // *algorithm*, but never through the actual multi-plugin load-order
    // pipeline (`parse_record_indexes_in_load_order`: on-disk files, real
    // MAST-based FormID remap, per-plugin parse feeding the running merge).
    // These fixtures close that gap with a real base-game→DLC→mod chain,
    // three plugins deep so a load order longer than the merge tests'
    // two-plugin base/child pair is actually exercised.

    /// TES4 with explicit `flags` + a Skyrim HEDR + a MAST sub-record per
    /// declared master. Extends [`build_tes4`] (which has no master-list
    /// capability) — the parser only reads `MAST`'s null-terminated name,
    /// no companion `DATA` size placeholder, so this is the minimum wire
    /// form `read_file_header` needs.
    fn build_tes4_with_masters(flags: u32, masters: &[&str]) -> Vec<u8> {
        let mut hedr = Vec::new();
        hedr.extend_from_slice(b"HEDR");
        hedr.extend_from_slice(&12u16.to_le_bytes());
        hedr.extend_from_slice(&1.7f32.to_le_bytes()); // Skyrim
        hedr.extend_from_slice(&0u32.to_le_bytes());
        hedr.extend_from_slice(&0u32.to_le_bytes());
        let mut sub_data = hedr;
        for master in masters {
            sub_data.extend_from_slice(b"MAST");
            let name = format!("{master}\0");
            sub_data.extend_from_slice(&(name.len() as u16).to_le_bytes());
            sub_data.extend_from_slice(name.as_bytes());
        }
        let mut buf = Vec::new();
        buf.extend_from_slice(b"TES4");
        buf.extend_from_slice(&(sub_data.len() as u32).to_le_bytes());
        buf.extend_from_slice(&flags.to_le_bytes());
        buf.extend_from_slice(&0u32.to_le_bytes());
        buf.extend_from_slice(&[0u8; 8]);
        buf.extend_from_slice(&sub_data);
        buf
    }

    /// A REFR with just `NAME` (base object) + zeroed `DATA` (pos/rot) —
    /// the minimum `parse_refr_group` needs to register a placement — at
    /// raw local `form_id`, optionally carrying the Deleted flag (0x20).
    fn build_refr_record(form_id: u32, base_form_id: u32, deleted: bool) -> Vec<u8> {
        let mut sub_data = Vec::new();
        sub_data.extend_from_slice(b"NAME");
        sub_data.extend_from_slice(&4u16.to_le_bytes());
        sub_data.extend_from_slice(&base_form_id.to_le_bytes());
        sub_data.extend_from_slice(b"DATA");
        sub_data.extend_from_slice(&24u16.to_le_bytes());
        sub_data.extend_from_slice(&[0u8; 24]);
        let mut buf = Vec::new();
        buf.extend_from_slice(b"REFR");
        buf.extend_from_slice(&(sub_data.len() as u32).to_le_bytes());
        buf.extend_from_slice(&(if deleted { 0x0000_0020u32 } else { 0 }).to_le_bytes());
        buf.extend_from_slice(&form_id.to_le_bytes());
        buf.extend_from_slice(&[0u8; 8]);
        buf.extend_from_slice(&sub_data);
        buf
    }

    /// An interior CELL record at raw local `form_id` (empty EDID, so
    /// cross-plugin identity matches purely on FormID — same as
    /// `parse_interior_without_edid_uses_form_id_identity_and_keeps_children`
    /// in `crates/plugin`), followed by a persistent-children GRUP (type 8)
    /// wrapping `refrs`. An empty `refrs` still emits the children GRUP —
    /// real partial-override CELLs commonly carry an empty/tombstone-only
    /// children group.
    fn build_cell_with_children(form_id: u32, refrs: &[Vec<u8>]) -> Vec<u8> {
        let mut cell_subs = Vec::new();
        cell_subs.extend_from_slice(b"DATA");
        cell_subs.extend_from_slice(&1u16.to_le_bytes());
        cell_subs.push(0x01); // is_interior
        let mut cell = Vec::new();
        cell.extend_from_slice(b"CELL");
        cell.extend_from_slice(&(cell_subs.len() as u32).to_le_bytes());
        cell.extend_from_slice(&0u32.to_le_bytes());
        cell.extend_from_slice(&form_id.to_le_bytes());
        cell.extend_from_slice(&[0u8; 8]);
        cell.extend_from_slice(&cell_subs);

        let refr_payload: Vec<u8> = refrs.iter().flatten().copied().collect();
        let mut children = Vec::new();
        children.extend_from_slice(b"GRUP");
        children.extend_from_slice(&((24 + refr_payload.len()) as u32).to_le_bytes());
        children.extend_from_slice(&form_id.to_le_bytes()); // group label = owning CELL's raw form_id
        children.extend_from_slice(&8u32.to_le_bytes()); // group_type 8 = persistent children
        children.extend_from_slice(&[0u8; 8]);
        children.extend_from_slice(&refr_payload);
        cell.extend_from_slice(&children);
        cell
    }

    /// Write a TES4(masters) + top-level `CELL` GRUP plugin to
    /// `dir/<stem>.esm` and return its path.
    fn write_cell_plugin(
        dir: &Path,
        stem: &str,
        masters: &[&str],
        cell: &[u8],
    ) -> std::path::PathBuf {
        let mut esm_bytes = build_tes4_with_masters(0, masters);
        esm_bytes.extend_from_slice(&wrap_group(b"CELL", cell));
        let path = dir.join(format!("{stem}.esm"));
        fs::write(&path, &esm_bytes).unwrap();
        path
    }

    /// #2370 EX-09/17 item 8 — a real 3-plugin chain (base → DLC → mod)
    /// through the actual `parse_record_indexes_in_load_order` pipeline
    /// (on-disk files, MAST-based FormID remap), not just the merge
    /// algorithm exercised in isolation. Covers item 5 (partial-CELL
    /// override keeps untouched base REFRs — already working) and item 7
    /// (cross-plugin REFR delete — this session's fix) composing across
    /// more than two plugins: the DLC's deletion of `refr_a` must survive
    /// into the mod's own override round, and the mod's own re-placement
    /// of a REFR at `refr_a`'s old FormID must still win (a legitimate
    /// later un-delete, not blocked by the earlier deletion).
    #[test]
    fn three_plugin_chain_composes_refr_merge_and_cross_plugin_delete() {
        let dir = tempfile::tempdir().unwrap();

        // base.esm (0 masters): CELL 0x1000 with REFR-A (0x2001) and
        // REFR-B (0x2002), both self-ref (top byte 0 == 0 masters).
        let base_cell = build_cell_with_children(
            0x0000_1000,
            &[
                build_refr_record(0x0000_2001, 0xAAA1, false),
                build_refr_record(0x0000_2002, 0xAAA2, false),
            ],
        );
        let base_path = write_cell_plugin(dir.path(), "base", &[], &base_cell);

        // dlc.esm (master: base.esm): overrides CELL 0x1000 — deletes
        // REFR-A, changes REFR-B's base object. Both REFRs reference
        // base.esm (master index 0), same bottom-24 identity as base.
        let dlc_cell = build_cell_with_children(
            0x0000_1000,
            &[
                build_refr_record(0x0000_2001, 0, true), // deleted; base_form_id irrelevant
                build_refr_record(0x0000_2002, 0xBEEF, false),
            ],
        );
        let dlc_path = write_cell_plugin(dir.path(), "dlc", &["base.esm"], &dlc_cell);

        // mod.esp (master: base.esm — NOT dlc.esm; it references the
        // ORIGINAL base identity, matching how a real third-party override
        // only needs to master whichever plugin first defined the FormID):
        // re-places a REFR at REFR-A's old FormID (a legitimate un-delete)
        // and adds a brand-new REFR-C via a self-ref FormID.
        let mod_cell = build_cell_with_children(
            0x0000_1000,
            &[
                build_refr_record(0x0000_2001, 0xFEED, false), // un-delete
                build_refr_record(0x0100_3000, 0xCCC3, false), // self-ref new REFR (top byte == mod.esp's 1 master)
            ],
        );
        let mod_path = write_cell_plugin(dir.path(), "modone", &["base.esm"], &mod_cell);

        let paths = [
            base_path.to_str().unwrap(),
            dlc_path.to_str().unwrap(),
            mod_path.to_str().unwrap(),
        ];
        let (index, _order) = parse_record_indexes_in_load_order(&paths).unwrap();

        let cell = index
            .cells
            .cells
            .get("cell_00001000")
            .expect("the 3-plugin-overridden CELL must merge to one entry");
        let by_id: std::collections::HashMap<u32, u32> = cell
            .references
            .iter()
            .map(|r| (r.form_id, r.base_form_id))
            .collect();

        // REFR-A (0x2001): deleted by dlc, then re-placed by mod — mod's
        // copy must win.
        assert_eq!(
            by_id.get(&0x0000_2001),
            Some(&0xFEED),
            "mod's un-delete re-placement must win over the DLC's earlier deletion"
        );
        // REFR-B (0x2002): overridden by dlc, untouched by mod since.
        assert_eq!(
            by_id.get(&0x0000_2002),
            Some(&0xBEEF),
            "DLC's override of REFR-B must survive mod's unrelated round"
        );
        // REFR-C: mod's own new self-ref REFR, remapped to mod's plugin
        // slot (2, since mod.esp is the 3rd plugin in load order).
        assert_eq!(
            by_id.get(&0x0200_3000),
            Some(&0xCCC3),
            "mod's own new REFR must remap into its own global slot (index 2)"
        );
        assert_eq!(
            cell.references.len(),
            3,
            "exactly REFR-A, REFR-B, REFR-C — no leftover deleted copy"
        );
    }

    /// EX-09/17 item 6 (#2370) — pins the documented (not fixed) WRLD
    /// merge gap as a real load-order fixture: unlike CELL's partial-field
    /// inherit, an override WRLD wholly replaces the base record. A field
    /// the override omits does NOT fall back to the base's value; it goes
    /// to that field's wire-absent default instead. If this test starts
    /// failing because a future change gives WRLD the same partial-inherit
    /// treatment as CELL, that's the intentional fix promised in
    /// `EsmCellIndex::merge_from`'s doc comment — update this test to match,
    /// don't just relax the assertion.
    #[test]
    fn wrld_override_replaces_whole_record_not_partial_fields() {
        let dir = tempfile::tempdir().unwrap();

        fn build_wrld_record(form_id: u32, edid: &str, dnam_water_height: Option<f32>) -> Vec<u8> {
            let mut sub_data = Vec::new();
            let edid_z = format!("{edid}\0");
            sub_data.extend_from_slice(b"EDID");
            sub_data.extend_from_slice(&(edid_z.len() as u16).to_le_bytes());
            sub_data.extend_from_slice(edid_z.as_bytes());
            if let Some(h) = dnam_water_height {
                sub_data.extend_from_slice(b"DNAM");
                sub_data.extend_from_slice(&8u16.to_le_bytes());
                sub_data.extend_from_slice(&0.0f32.to_le_bytes()); // default_land_height (unused here)
                sub_data.extend_from_slice(&h.to_le_bytes());
            }
            let mut buf = Vec::new();
            buf.extend_from_slice(b"WRLD");
            buf.extend_from_slice(&(sub_data.len() as u32).to_le_bytes());
            buf.extend_from_slice(&0u32.to_le_bytes());
            buf.extend_from_slice(&form_id.to_le_bytes());
            buf.extend_from_slice(&[0u8; 8]);
            buf.extend_from_slice(&sub_data);
            buf
        }

        let base_wrld = build_wrld_record(0x0000_0100, "TestWorld", Some(5.0));
        let mut base_bytes = build_tes4_with_masters(0, &[]);
        base_bytes.extend_from_slice(&wrap_group(b"WRLD", &base_wrld));
        let base_path = dir.path().join("baseworld.esm");
        fs::write(&base_path, &base_bytes).unwrap();

        // DLC override: same WRLD FormID, re-authors EDID but omits DNAM
        // entirely — a partial-CELL-style override would inherit the
        // base's water height; today's whole-record WRLD merge does not.
        let over_wrld = build_wrld_record(0x0000_0100, "TestWorld", None);
        let mut over_bytes = build_tes4_with_masters(0, &["baseworld.esm"]);
        over_bytes.extend_from_slice(&wrap_group(b"WRLD", &over_wrld));
        let over_path = dir.path().join("overworld.esm");
        fs::write(&over_path, &over_bytes).unwrap();

        let paths = [base_path.to_str().unwrap(), over_path.to_str().unwrap()];
        let (index, _order) = parse_record_indexes_in_load_order(&paths).unwrap();

        let world = index
            .cells
            .worldspaces
            .get("testworld")
            .expect("worldspace must merge to one entry");
        assert_eq!(
            world.default_water_height, None,
            "the override's omitted DNAM must NOT inherit the base's water height — \
             pins today's whole-record WRLD replace (item 6, flagged not fixed)"
        );
    }
}
