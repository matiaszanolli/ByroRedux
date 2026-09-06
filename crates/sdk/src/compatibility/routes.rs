//! Layer 1 — the SDK route constants shared across compat services.
//!
//! Each service that owns its whole four-layer stack (notably
//! `storage_util`) keeps its own routes beside the code that uses them.

/// Largest plugin count representable by the classic OBSE/xNVSE load order.
/// Indices `0..=254` are valid and `255` is reserved as the missing sentinel.
pub const LEGACY_OBSCRIPT_PLUGIN_LIMIT: usize = 255;

pub const LEGACY_OBSCRIPT_MISSING_MOD_INDEX: i32 = 255;

/// Engine routes backing SKSE's `Game` compatibility aliases.
pub const PAPYRUS_GAME_GET_MOD_COUNT_ROUTE: &str = "byro.content.catalog.get-mod-count";

pub const PAPYRUS_GAME_GET_PLAYER_ROUTE: &str = "byro.world.compat.get-player";

pub const PAPYRUS_GAME_GET_MOD_BY_NAME_ROUTE: &str = "byro.content.catalog.get-mod-by-name";

pub const PAPYRUS_GAME_GET_FORM_FROM_FILE_ROUTE: &str = "byro.content.catalog.get-form-from-file";

pub const PAPYRUS_GAME_GET_MOD_NAME_ROUTE: &str = "byro.content.catalog.get-mod-name";

pub const PAPYRUS_GAME_GET_MOD_DEPENDENCY_COUNT_ROUTE: &str =
    "byro.content.catalog.get-mod-dependency-count";

pub const PAPYRUS_GAME_IS_PLUGIN_INSTALLED_ROUTE: &str = "byro.content.catalog.is-plugin-installed";

pub const PAPYRUS_GAME_GET_LIGHT_MOD_COUNT_ROUTE: &str = "byro.content.catalog.get-light-mod-count";

pub const PAPYRUS_GAME_GET_LIGHT_MOD_BY_NAME_ROUTE: &str =
    "byro.content.catalog.get-light-mod-by-name";

pub const PAPYRUS_GAME_GET_LIGHT_MOD_NAME_ROUTE: &str = "byro.content.catalog.get-light-mod-name";

pub const PAPYRUS_GAME_GET_LIGHT_MOD_DEPENDENCY_COUNT_ROUTE: &str =
    "byro.content.catalog.get-light-mod-dependency-count";

pub const PAPYRUS_GAME_GET_NTH_LIGHT_MOD_DEPENDENCY_ROUTE: &str =
    "byro.content.catalog.get-nth-light-mod-dependency";

pub const PAPYRUS_GAME_LIGHT_MOD_OFFSET: i32 = 0x100;

pub const PAPYRUS_GAME_MISSING_LIGHT_MOD_INDEX: i32 = 0xffff;
