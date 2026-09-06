//! Layer 4 — runtime adapters for the game/content-catalog surface
//! (plugin counts, indices, names, dependencies, load order).

use super::*;

pub fn adapt_papyrus_game_get_mod_count(catalog: &ContentCatalog) -> i32 {
    plugin_count(catalog, PluginKind::Regular)
}

/// Execute SKSE's `Game.GetModByName` against the immutable engine catalog.
pub fn adapt_papyrus_game_get_mod_by_name(catalog: &ContentCatalog, plugin: &str) -> i32 {
    let Some((kind, index)) = plugin_index(catalog, plugin) else {
        return LEGACY_OBSCRIPT_MISSING_MOD_INDEX;
    };
    match kind {
        PluginKind::Regular => index,
        PluginKind::Light => PAPYRUS_GAME_LIGHT_MOD_OFFSET.saturating_add(index),
    }
}

/// Execute Papyrus `Game.GetFormFromFile` against the immutable content catalog.
pub fn adapt_papyrus_game_get_form_from_file(
    catalog: &ContentCatalog,
    form_id: i64,
    plugin: &str,
) -> Option<FormRef> {
    let local = u32::try_from(form_id).ok()?;
    catalog.qualify_form(plugin, local)
}

pub fn adapt_papyrus_game_get_mod_name(catalog: &ContentCatalog, index: i64) -> String {
    if index < 0 || index > i64::from(i32::MAX) {
        return String::new();
    }
    let index = index as i32;
    if index > LEGACY_OBSCRIPT_MISSING_MOD_INDEX {
        plugin_name(
            catalog,
            PluginKind::Light,
            index - PAPYRUS_GAME_LIGHT_MOD_OFFSET,
        )
    } else {
        plugin_name(catalog, PluginKind::Regular, index)
    }
}

pub fn adapt_papyrus_game_get_mod_dependency_count(catalog: &ContentCatalog, index: i64) -> i32 {
    i32::try_from(
        plugin_at_mod_index(catalog, index).map_or(0, |plugin| plugin.dependencies().len()),
    )
    .expect("content catalog dependency count fits i32")
}

pub fn adapt_papyrus_game_is_plugin_installed(catalog: &ContentCatalog, plugin: &str) -> bool {
    catalog.find(plugin).is_some()
}

pub fn adapt_papyrus_game_get_light_mod_count(catalog: &ContentCatalog) -> i32 {
    plugin_count(catalog, PluginKind::Light)
}

pub fn adapt_papyrus_game_get_light_mod_by_name(catalog: &ContentCatalog, plugin: &str) -> i32 {
    match plugin_index(catalog, plugin) {
        Some((PluginKind::Light, index)) => index,
        _ => PAPYRUS_GAME_MISSING_LIGHT_MOD_INDEX,
    }
}

pub fn adapt_papyrus_game_get_light_mod_name(catalog: &ContentCatalog, index: i64) -> String {
    i32::try_from(index).ok().map_or_else(String::new, |index| {
        plugin_name(catalog, PluginKind::Light, index)
    })
}

pub fn adapt_papyrus_game_get_light_mod_dependency_count(
    catalog: &ContentCatalog,
    index: i64,
) -> i32 {
    let count = i32::try_from(index)
        .ok()
        .and_then(|index| plugin_at(catalog, PluginKind::Light, index))
        .map_or(0, |plugin| plugin.dependencies().len());
    i32::try_from(count).expect("content catalog dependency count fits i32")
}

pub fn adapt_papyrus_game_get_nth_light_mod_dependency(
    catalog: &ContentCatalog,
    mod_index: i64,
    dependency_index: i64,
) -> i32 {
    let Some(plugin) = i32::try_from(mod_index)
        .ok()
        .and_then(|index| plugin_at(catalog, PluginKind::Light, index))
    else {
        return 0;
    };
    let Some(dependency_ordinal) = usize::try_from(dependency_index)
        .ok()
        .and_then(|index| plugin.dependencies().get(index))
        .copied()
    else {
        return 0;
    };
    let Some(dependency) = catalog.plugin(dependency_ordinal) else {
        return 0;
    };
    if dependency.kind() != PluginKind::Regular {
        return 0;
    }
    i32::try_from(
        catalog
            .iter()
            .take(dependency_ordinal as usize)
            .filter(|plugin| plugin.kind() == PluginKind::Regular)
            .count(),
    )
    .expect("content catalog regular index fits i32")
}

fn plugin_count(catalog: &ContentCatalog, kind: PluginKind) -> i32 {
    i32::try_from(
        catalog
            .iter()
            .filter(|plugin| plugin.kind() == kind)
            .count(),
    )
    .expect("content catalog count fits i32")
}

fn plugin_index(catalog: &ContentCatalog, name: &str) -> Option<(PluginKind, i32)> {
    let target = catalog.find(name)?.1;
    let kind = target.kind();
    let index = catalog
        .iter()
        .filter(|plugin| plugin.kind() == kind)
        .position(|plugin| std::ptr::eq(plugin, target))?;
    Some((
        kind,
        i32::try_from(index).expect("content catalog index fits i32"),
    ))
}

fn plugin_name(catalog: &ContentCatalog, kind: PluginKind, index: i32) -> String {
    let Ok(index) = usize::try_from(index) else {
        return String::new();
    };
    catalog
        .iter()
        .filter(|plugin| plugin.kind() == kind)
        .nth(index)
        .map_or_else(String::new, |plugin| plugin.name().to_owned())
}

fn plugin_at_mod_index(
    catalog: &ContentCatalog,
    index: i64,
) -> Option<&crate::content::PluginInfo> {
    let index = i32::try_from(index).ok()?;
    if index > LEGACY_OBSCRIPT_MISSING_MOD_INDEX {
        plugin_at(
            catalog,
            PluginKind::Light,
            index.checked_sub(PAPYRUS_GAME_LIGHT_MOD_OFFSET)?,
        )
    } else {
        plugin_at(catalog, PluginKind::Regular, index)
    }
}

fn plugin_at(
    catalog: &ContentCatalog,
    kind: PluginKind,
    index: i32,
) -> Option<&crate::content::PluginInfo> {
    let index = usize::try_from(index).ok()?;
    catalog
        .iter()
        .filter(|plugin| plugin.kind() == kind)
        .nth(index)
}

/// Typed load-order operation recovered from extender-era ObScript.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LegacyObscriptLoadOrderCall {
    IsModLoaded { plugin: String },
    GetModIndex { plugin: String },
    GetNumLoadedMods,
    GetNumLoadedPlugins,
    GetNthModName { index: i32 },
}

/// ObScript-visible scalar produced by a load-order compatibility call.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LegacyObscriptLoadOrderResult {
    Bool(bool),
    Integer(i32),
    String(String),
}

/// Failure to represent the active catalog through the classic 8-bit ABI.
#[derive(Clone, Debug, Eq, thiserror::Error, PartialEq)]
pub enum LegacyObscriptLoadOrderError {
    #[error(
        "active catalog has {actual} plugins, exceeding the classic ObScript limit of {maximum}"
    )]
    PluginBudgetExceeded { actual: usize, maximum: usize },
}

/// Execute an OBSE/xNVSE load-order query against the immutable engine
/// content snapshot without loading an external script extender.
///
/// This deliberately preserves the classic `0xff` missing-index sentinel and
/// empty-string nth-name behavior. Catalog ordinals remain callback-local and
/// must not be persisted as authored identity.
pub fn adapt_legacy_obscript_load_order(
    catalog: &ContentCatalog,
    call: LegacyObscriptLoadOrderCall,
) -> Result<LegacyObscriptLoadOrderResult, LegacyObscriptLoadOrderError> {
    if catalog.len() > LEGACY_OBSCRIPT_PLUGIN_LIMIT {
        return Err(LegacyObscriptLoadOrderError::PluginBudgetExceeded {
            actual: catalog.len(),
            maximum: LEGACY_OBSCRIPT_PLUGIN_LIMIT,
        });
    }

    let result = match call {
        LegacyObscriptLoadOrderCall::IsModLoaded { plugin } => {
            LegacyObscriptLoadOrderResult::Bool(catalog.find(&plugin).is_some())
        }
        LegacyObscriptLoadOrderCall::GetModIndex { plugin } => {
            let index = catalog
                .find(&plugin)
                .map_or(LEGACY_OBSCRIPT_MISSING_MOD_INDEX, |(index, _)| {
                    i32::try_from(index).expect("classic content catalog index fits i32")
                });
            LegacyObscriptLoadOrderResult::Integer(index)
        }
        LegacyObscriptLoadOrderCall::GetNumLoadedMods
        | LegacyObscriptLoadOrderCall::GetNumLoadedPlugins => {
            LegacyObscriptLoadOrderResult::Integer(
                i32::try_from(catalog.len()).expect("classic content catalog length fits i32"),
            )
        }
        LegacyObscriptLoadOrderCall::GetNthModName { index } => {
            let name = u32::try_from(index)
                .ok()
                .and_then(|index| catalog.plugin(index))
                .map_or_else(String::new, |plugin| plugin.name().to_owned());
            LegacyObscriptLoadOrderResult::String(name)
        }
    };
    Ok(result)
}
