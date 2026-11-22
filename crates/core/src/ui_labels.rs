//! M48 GUI-PAGE-005 (#6666) — the words the native pages draw, in the game's own vocabulary.
//!
//! Where a native page stays the surface (PRD Q1, S3), every label it shows must come from
//! the game's data, a record's name or a `GMST` string, unless the Phase 0 inventory
//! (`docs/data/game-ui/native-page-strings.tsv`) lists it as an engine string with its reason.
//! [`UiLabels`] is that table: each [`LabelDef`] names a word by its engine text (the key) and
//! the game settings that carry it, tried in order. The load order's winning `GMST` value is
//! drawn when one is authored (Skyrim authors `sContinue`, `sBack`, `sWeight`, `sTakeAll` …,
//! through its string tables); without one the key itself is drawn, and the inventory lists it.
//! New Vegas authors none of these settings: its defaults live in the executable, which is
//! packed (GUI-FONT-005's FNV source is open), so on FNV the keys are drawn unless a plugin
//! authors the setting.
//!
//! A setting's value may be a printf-style template (`sNotEnoughVendorGold` = "Transaction
//! value: %d gold.  Vendor only has %d gold."); [`UiLabels::format`] fills it from the call's
//! arguments, in order. A key carries no placeholders, so the arguments only reach the drawn
//! text through a game's template.

use std::collections::HashMap;
use std::fmt::Display;
use std::sync::Arc;

use crate::ecs::Resource;

/// One word a native page draws: its engine text, and the game settings that carry the game's
/// own word for it, in lookup order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LabelDef {
    pub key: &'static str,
    pub settings: &'static [&'static str],
}

const fn def(key: &'static str, settings: &'static [&'static str]) -> LabelDef {
    LabelDef { key, settings }
}

/// Every label that has a game equivalent. Setting names are from the Phase 0 census (the
/// FO3 executable's built-in table and the FO3 / FNV / Skyrim masters); a name a game does not
/// author is simply skipped.
pub const LABELS: &[LabelDef] = &[
    // The pause page.
    def("Paused", &["sPauseText"]),
    def("Continue", &["sContinue"]),
    def("Settings", &["sSettings"]),
    def("Inventory", &["sContainerItemsTitle"]),
    def("Quicksave", &["sMenuDisplayQuicksaveName"]),
    def("Quit to desktop", &["sQuit"]),
    // Shared navigation.
    def("Back", &["sBack"]),
    def("Close", &["sClose", "sExit"]),
    def("Ok", &["sOk"]),
    // The settings page: section tabs and the per-setting reset.
    def("Gameplay", &["sGameplay"]),
    def("Controls", &["sControls"]),
    def("Audio", &["sAudio"]),
    def("Reset", &["sReset"]),
    // The inventory page.
    def("All", &["sAllItems"]),
    def("Count", &["sQuantity"]),
    def("Weight", &["sWeight"]),
    def("Value", &["sValue"]),
    def("Drop", &["sInventoryDrop"]),
    def("Use", &["sInventoryUse", "sUse"]),
    def("Equip", &["sInventoryEquip"]),
    def("Unequip", &["sInventoryUnequip"]),
    // Inventory categories (the tab names).
    def("Armor", &["sArmor"]),
    def("Weapon", &["sInventoryWeapons", "sWeapons"]),
    def("Ammo", &["sInventoryAmmoTab"]),
    def("Aid", &["sInventoryAid"]),
    def("Misc", &["sInventoryMisc", "sMisc"]),
    def("Junk", &["sJunk"]),
    def("Note", &["sNotesTabText"]),
    def("Book", &["sBooks"]),
    def("Scroll", &["sScrolls"]),
    def("Ingredient", &["sIngredients"]),
    def("Key", &["sKeys"]),
    // Barter.
    def("Buy", &["sBuy"]),
    def("Sell", &["sSell"]),
    def("gold", &["sInventoryCaps", "sGold"]),
    def("You can't afford that.", &["sItemTooExpensive", "sNotEnoughCaps"]),
    def("The merchant can't afford that.", &["sNotEnoughVendorGold", "sMerchantNoGold"]),
    // Crafting.
    def("Craft", &["sCraft"]),
    // Character creation.
    def("Done", &["sDone"]),
    def("Male", &["sSexMale"]),
    def("Female", &["sSexFemale"]),
    // Oblivion rest and level-up fallback.
    def("Wait", &["sWait"]),
    def("Hours", &["sHours"]),
    def("Level", &["sLevel"]),
    def("Cancel", &["sCancel"]),
    def("Strength", &["sAttributeStrength"]),
    def("Intelligence", &["sAttributeIntelligence"]),
    def("Willpower", &["sAttributeWillpower"]),
    def("Agility", &["sAttributeAgility"]),
    def("Speed", &["sAttributeSpeed"]),
    def("Endurance", &["sAttributeEndurance"]),
    def("Personality", &["sAttributePersonality"]),
    def("Luck", &["sAttributeLuck"]),
    // Interaction prompt verbs.
    def("Activate", &["sActivate"]),
    def("Open", &["sOpen"]),
    def("Take all", &["sTakeAll"]),
    def("Take", &["sTake"]),
    def("Talk", &["sTalk"]),
    def("Sleep", &["sFurnitureSleep", "sSleep"]),
    def("Sit", &["sSit"]),
    // Vitals.
    def("Health", &["sHealth"]),
    def("HP", &["sHitPointsShort", "sStatsHP"]),
    def("AP", &["sActionPointsShort", "sStatsAP"]),
    def("RAD", &["sStatsRADAbbrev"]),
    // Player messages.
    def("XP", &["sStatsXP"]),
    def("Unlocked with key", &["sOpenWithKey"]),
    def("Quicksaving...", &["sQuickSaving"]),
    def("Quickloading...", &["sQuickLoading"]),
    def("You cannot save right now.", &["sCantSaveNow"]),
];

/// The load order's words for [`LABELS`]. A handle: cloning it (once per frame, into the
/// panel snapshot) is a refcount bump. `NOT_SAVED_BY_DESIGN`: resolved from the records on
/// every load. The default is the engine text for every key (no game settings).
#[derive(Debug, Clone)]
pub struct UiLabels(Arc<LabelTable>);

#[derive(Debug)]
struct LabelTable {
    /// Per [`LABELS`] entry: the drawn text (the game's when authored, else the key).
    texts: Vec<Arc<str>>,
    /// Per [`LABELS`] entry: the setting that supplied the text, if any.
    sources: Vec<Option<&'static str>>,
    index: HashMap<&'static str, usize>,
}

impl Resource for UiLabels {}

impl Default for UiLabels {
    fn default() -> Self {
        static ENGINE: std::sync::OnceLock<UiLabels> = std::sync::OnceLock::new();
        ENGINE.get_or_init(|| Self::resolve(|_| None)).clone()
    }
}

impl UiLabels {
    /// Resolve every label through `setting` (a string `GMST` by EditorID, the load order's
    /// winning value; `None` when unauthored).
    pub fn resolve(setting: impl Fn(&str) -> Option<String>) -> Self {
        let mut texts = Vec::with_capacity(LABELS.len());
        let mut sources = Vec::with_capacity(LABELS.len());
        for label in LABELS {
            let found = label
                .settings
                .iter()
                .find_map(|name| setting(name).filter(|value| !value.trim().is_empty()).map(|value| (*name, value)));
            match found {
                Some((name, value)) => {
                    texts.push(Arc::from(value));
                    sources.push(Some(name));
                }
                None => {
                    texts.push(Arc::from(label.key));
                    sources.push(None);
                }
            }
        }
        let index = LABELS.iter().enumerate().map(|(i, label)| (label.key, i)).collect();
        Self(Arc::new(LabelTable { texts, sources, index }))
    }

    /// The drawn text for `key`: the game's word when the load order authors it, else the key.
    /// A key with no [`LabelDef`] is engine text and is returned unchanged.
    pub fn text<'a>(&'a self, key: &'a str) -> &'a str {
        self.0.index.get(key).map_or(key, |&i| &self.0.texts[i])
    }

    /// [`Self::text`] as a shared string, for views rebuilt every frame (a refcount bump).
    pub fn shared(&self, key: &str) -> Arc<str> {
        self.0.index.get(key).map_or_else(|| Arc::from(key), |&i| Arc::clone(&self.0.texts[i]))
    }

    /// The setting `key` was resolved from, if the load order authors one.
    pub fn source(&self, key: &str) -> Option<&'static str> {
        self.0.index.get(key).and_then(|&i| self.0.sources[i])
    }

    /// How many labels resolved from the game's settings.
    pub fn resolved_count(&self) -> usize {
        self.0.sources.iter().filter(|source| source.is_some()).count()
    }

    /// [`Self::text`] with a game template's `%d` / `%s` / `%i` / `%f` (flags and precision
    /// allowed) filled from `args` in order, and `%%` as a literal percent. A missing argument
    /// leaves its placeholder empty; the key itself has no placeholders.
    pub fn format(&self, key: &str, args: &[&dyn Display]) -> String {
        fill_template(self.text(key), args)
    }
}

/// Fill a printf-style template (see [`UiLabels::format`]).
pub fn fill_template(template: &str, args: &[&dyn Display]) -> String {
    let mut out = String::with_capacity(template.len());
    let mut args = args.iter();
    let mut chars = template.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '%' {
            out.push(c);
            continue;
        }
        if chars.peek() == Some(&'%') {
            chars.next();
            out.push('%');
            continue;
        }
        // Flags, width and precision, then the conversion.
        let mut spec = String::new();
        while let Some(&next) = chars.peek() {
            if next.is_ascii_digit() || matches!(next, '.' | '-' | '+' | ' ' | '#') {
                spec.push(next);
                chars.next();
            } else {
                break;
            }
        }
        match chars.peek() {
            Some('d' | 'i' | 'u' | 's' | 'f' | 'g') => {
                chars.next();
                if let Some(arg) = args.next() {
                    out.push_str(&arg.to_string());
                }
            }
            // Not a conversion: the text is literal.
            _ => {
                out.push('%');
                out.push_str(&spec);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_label_reads_the_first_authored_setting_else_its_key() {
        let labels = UiLabels::resolve(|name| match name {
            "sContinue" => Some("Continue".into()),
            "sUse" => Some("Use".into()),
            "sInventoryWeapons" => Some(String::new()),
            "sWeapons" => Some("WEAPONS".into()),
            _ => None,
        });
        assert_eq!(labels.text("Continue"), "Continue");
        assert_eq!(labels.source("Continue"), Some("sContinue"));
        assert_eq!(labels.source("Use"), Some("sUse"), "the second name when the first is unauthored");
        assert_eq!(labels.text("Weapon"), "WEAPONS", "an empty value counts as unauthored");
        assert_eq!(labels.text("Back"), "Back");
        assert_eq!(labels.source("Back"), None);
        assert_eq!(labels.text("BYROREDUX"), "BYROREDUX", "a key with no definition is engine text");
        assert_eq!(&*labels.shared("Take all"), "Take all");
        assert_eq!(labels.resolved_count(), 3);
    }

    #[test]
    fn a_game_template_takes_the_arguments_and_a_key_ignores_them() {
        let skyrim = UiLabels::resolve(|name| {
            (name == "sNotEnoughVendorGold").then(|| "Transaction value: %d gold.  Vendor only has %d gold.".into())
        });
        assert_eq!(
            skyrim.format("The merchant can't afford that.", &[&120, &75]),
            "Transaction value: 120 gold.  Vendor only has 75 gold."
        );
        assert_eq!(UiLabels::default().format("The merchant can't afford that.", &[&120, &75]), "The merchant can't afford that.");
        assert_eq!(fill_template("Unlocked with %s.", &[&"Doc's key"]), "Unlocked with Doc's key.");
        assert_eq!(fill_template("%.0f%% %d", &[&3]), "3% ");
        assert_eq!(fill_template("50% off", &[]), "50% off");
    }

    #[test]
    fn every_key_is_unique() {
        let mut keys: Vec<&str> = LABELS.iter().map(|label| label.key).collect();
        keys.sort_unstable();
        let before = keys.len();
        keys.dedup();
        assert_eq!(keys.len(), before);
    }
}
