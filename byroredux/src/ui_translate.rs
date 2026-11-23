//! The game's `$`-key text for the native pages (catalog U4): the Scaleform table of the active
//! language, merged with the enabled plugins' translation files, read for surfaces that have no movie
//! of their own to install it (the Mod Configuration page, a script message box on the native page).

/// Install the table once for this session. A game with no interface archive (Oblivion, Fallout 3,
/// New Vegas) has no such table, and nothing is installed.
fn ensure_installed() {
    static DONE: std::sync::Once = std::sync::Once::new();
    DONE.call_once(|| {
        let args = crate::cli_args::effective_args();
        let Ok(Some((_, archive_path))) = crate::scaleform_hud::interface_archive(&args) else {
            return;
        };
        match crate::asset_provider::LayeredSources::interface(&args, &archive_path) {
            Ok(sources) => byroredux_ui::load_translations(&sources),
            Err(error) => log::warn!("the `$`-key translation table could not be read: {error}"),
        }
    });
}

/// `text` with a `$` key replaced by the game's text, else as written (an unknown key is drawn as the
/// game draws it).
pub(crate) fn translate(text: &str) -> String {
    if !text.contains('$') {
        return text.to_owned();
    }
    ensure_installed();
    byroredux_ui::translate_text(text)
}
