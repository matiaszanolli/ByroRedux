//! byroredux patch (M48 GUI-FONT-004, #6665): GFx's `$`-key text translation.
//!
//! A Scaleform text field whose text is a `$` key of the game's translation table
//! (`interface\translate_<language>.txt`) shows the key's text instead
//! (`SetText("$Quit to main menu?…")`, `Direction_tf.text = "$Compass_North"`). GFx applies it
//! to the text a field is created with and to every text a movie sets; Flash has no such pass, so
//! Ruffle does not either. The embedding engine installs the lookup ([`set_text_translator`]);
//! [`translate`] is called from `EditText`'s construction and its `text` / `htmlText` setters.
//!
//! Plain text is translated whole; in HTML, each run of text between tags is looked up on its
//! own, so `<p align="center"><b>$Compass_North</b></p>` keeps its markup.

use std::sync::{Arc, RwLock};

use ruffle_wstr::{WStr, WString};

/// The lookup: a key (with its `$`) to its text, `None` when the table has no such key.
pub type TextTranslator = dyn Fn(&str) -> Option<String> + Send + Sync;

static TRANSLATOR: RwLock<Option<Arc<TextTranslator>>> = RwLock::new(None);

/// Install (or, with `None`, remove) the process-wide translation lookup.
pub fn set_text_translator(translator: Option<Arc<TextTranslator>>) {
    if let Ok(mut slot) = TRANSLATOR.write() {
        *slot = translator;
    }
}

/// `text` with its `$` keys translated, or `None` when nothing changed.
pub fn translate(text: &WStr) -> Option<WString> {
    let source = text.to_utf8_lossy();
    if !source.contains('$') {
        return None;
    }
    let translator = TRANSLATOR.read().ok()?.clone()?;
    let mut out = String::with_capacity(source.len());
    let mut changed = false;
    let mut rest: &str = &source;
    loop {
        let run_end = rest.find('<').unwrap_or(rest.len());
        let run = &rest[..run_end];
        match run.starts_with('$').then(|| translator(run)).flatten() {
            Some(translated) => {
                out.push_str(&translated);
                changed = true;
            }
            None => out.push_str(run),
        }
        rest = &rest[run_end..];
        if rest.is_empty() {
            break;
        }
        // Copy the tag through its `>`; an unclosed tag is copied as it is.
        let tag_end = rest.find('>').map_or(rest.len(), |at| at + 1);
        out.push_str(&rest[..tag_end]);
        rest = &rest[tag_end..];
        if rest.is_empty() {
            break;
        }
    }
    changed.then(|| WString::from_utf8(&out))
}
