//! Session-global FormIDs in saves, written as stable identities (PRD legacy-mod-compatibility LMC-SAV-002..004).
//!
//! A global FormID's top byte (or light / medium sub-index) is a load-order slot, so a `u32` saved under one load
//! order names a different form, or none, under another. Saved state that holds one marks the field with this
//! module's serde adapters (`#[serde(with = "stable_form_id")]`, [`option`], [`vec`], [`map`]). While a
//! [`FormIdTranslator`] is installed around a save ([`with_translator`]), the adapter writes the plugin-and-object
//! [`FormIdPair`] in place of the number; around a load it reads the pair back through the *current* load order. A
//! pair whose plugin the current load order lacks reads as the null form (`0`) for a single field, and is dropped from
//! a list or a map; each such plugin is counted in the [`TranslationReport`] the scope returns, so the load can report
//! what it lost instead of carrying a dangling FormID.
//!
//! A FormID the translator cannot name (FormID 0, an engine-made reference with no owning plugin) is written as the
//! number itself, and a number reads back unchanged, so a save without a translator (a unit test's round trip) is the
//! plain integer form it always was. A pair read with no translator installed (a typed preflight of the columns before
//! the reload) reads as the null form and counts nothing.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::form_id::{FormIdPair, PluginId};

/// Converts between the session's global FormIDs and stable identities: the active load order.
pub trait FormIdTranslator {
    /// The stable identity of `global`, or `None` when no plugin of this load order owns its slot.
    fn pair(&self, global: u32) -> Option<FormIdPair>;
    /// The global FormID `pair` maps to in this load order, or `None` when its plugin is not loaded.
    fn global(&self, pair: FormIdPair) -> Option<u32>;
}

/// What a translation scope did.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct TranslationReport {
    /// FormIDs written or read as stable identities.
    pub translated: usize,
    /// Per plugin the current load order lacks, how many distinct saved forms of it read as absent (a form read by
    /// two decode passes, a typed check and the restore, counts once).
    pub absent: BTreeMap<PluginId, usize>,
}

struct Scope {
    translator: Rc<dyn FormIdTranslator>,
    report: TranslationReport,
    absent_seen: std::collections::HashSet<FormIdPair>,
}

thread_local! {
    static SCOPE: RefCell<Option<Scope>> = const { RefCell::new(None) };
}

/// Run `f` with `translator` converting every marked FormID it serializes or deserializes on this thread, and return
/// its result with what the conversion did. Scopes nest: an inner one is restored to the outer on return.
pub fn with_translator<R>(translator: Rc<dyn FormIdTranslator>, f: impl FnOnce() -> R) -> (R, TranslationReport) {
    let outer = SCOPE.with(|scope| {
        scope.replace(Some(Scope { translator, report: TranslationReport::default(), absent_seen: Default::default() }))
    });
    let result = f();
    let report = SCOPE.with(|scope| scope.replace(outer)).map(|scope| scope.report).unwrap_or_default();
    (result, report)
}

/// One saved FormID: a stable identity, or the number itself when it has none.
#[derive(Serialize, Deserialize)]
#[serde(untagged)]
enum Repr {
    Raw(u32),
    Pair(FormIdPair),
}

fn to_repr(global: u32) -> Repr {
    if global == 0 {
        return Repr::Raw(0);
    }
    SCOPE.with(|scope| {
        let mut scope = scope.borrow_mut();
        match scope.as_mut() {
            Some(scope) => match scope.translator.pair(global) {
                Some(pair) => {
                    scope.report.translated += 1;
                    Repr::Pair(pair)
                }
                None => Repr::Raw(global),
            },
            None => Repr::Raw(global),
        }
    })
}

/// The global FormID `repr` names in the current load order; `None` when its plugin is absent.
fn from_repr(repr: Repr) -> Option<u32> {
    match repr {
        Repr::Raw(global) => Some(global),
        Repr::Pair(pair) => SCOPE.with(|scope| {
            let mut scope = scope.borrow_mut();
            let scope = scope.as_mut()?;
            match scope.translator.global(pair) {
                Some(global) => {
                    scope.report.translated += 1;
                    Some(global)
                }
                None => {
                    if scope.absent_seen.insert(pair) {
                        *scope.report.absent.entry(pair.plugin).or_default() += 1;
                    }
                    None
                }
            }
        }),
    }
}

/// `#[serde(with = "stable_form_id")]` on a `u32` FormID. An absent plugin reads as `0`, the null form.
pub fn serialize<S: Serializer>(global: &u32, ser: S) -> Result<S::Ok, S::Error> {
    to_repr(*global).serialize(ser)
}

/// See [`serialize`].
pub fn deserialize<'de, D: Deserializer<'de>>(de: D) -> Result<u32, D::Error> {
    Ok(from_repr(Repr::deserialize(de)?).unwrap_or(0))
}

/// `#[serde(with = "stable_form_id::option")]` on an `Option<u32>` FormID. An absent plugin reads as `None`.
pub mod option {
    use super::*;

    pub fn serialize<S: Serializer>(global: &Option<u32>, ser: S) -> Result<S::Ok, S::Error> {
        global.map(to_repr).serialize(ser)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(de: D) -> Result<Option<u32>, D::Error> {
        Ok(Option::<Repr>::deserialize(de)?.and_then(from_repr))
    }
}

/// `#[serde(with = "stable_form_id::vec")]` on a `Vec<u32>` of FormIDs. An absent plugin's entries are dropped.
pub mod vec {
    use super::*;

    pub fn serialize<S: Serializer>(globals: &[u32], ser: S) -> Result<S::Ok, S::Error> {
        globals.iter().map(|&global| to_repr(global)).collect::<Vec<_>>().serialize(ser)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(de: D) -> Result<Vec<u32>, D::Error> {
        Ok(Vec::<Repr>::deserialize(de)?.into_iter().filter_map(from_repr).collect())
    }
}

/// `#[serde(with = "stable_form_id::map")]` on a map keyed by FormID (`HashMap<u32, V>`, `BTreeMap<u32, V>`), written
/// as a list of `[key, value]` entries because a stable identity is not a JSON object key. An absent plugin's entries
/// are dropped.
pub mod map {
    use super::*;

    pub fn serialize<'a, M, V, S>(entries: &'a M, ser: S) -> Result<S::Ok, S::Error>
    where
        &'a M: IntoIterator<Item = (&'a u32, &'a V)>,
        V: Serialize + 'a,
        S: Serializer,
    {
        entries.into_iter().map(|(&key, value)| (to_repr(key), value)).collect::<Vec<_>>().serialize(ser)
    }

    pub fn deserialize<'de, M, V, D>(de: D) -> Result<M, D::Error>
    where
        M: FromIterator<(u32, V)>,
        V: Deserialize<'de>,
        D: Deserializer<'de>,
    {
        Ok(Vec::<(Repr, V)>::deserialize(de)?
            .into_iter()
            .filter_map(|(key, value)| from_repr(key).map(|key| (key, value)))
            .collect())
    }
}

#[cfg(test)]
#[path = "stable_form_id_tests.rs"]
mod tests;
