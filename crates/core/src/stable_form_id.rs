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
#[derive(Serialize, Deserialize, Clone, Copy)]
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

/// One saved FormID in its wire form: the stable identity, or the number itself when it has none. For a wire struct
/// a saved type converts through (`#[serde(from, into)]`) when its FormIDs sit where no adapter reaches.
#[derive(Serialize, Deserialize)]
#[serde(transparent)]
pub struct SavedForm(Repr);

impl SavedForm {
    /// `global` as the current translation scope writes it.
    pub fn save(global: u32) -> Self {
        Self(to_repr(global))
    }

    /// The global FormID this names in the current load order; `None` when its plugin is absent (counted).
    pub fn load(self) -> Option<u32> {
        from_repr(self.0)
    }
}

/// A value made of FormIDs (and plain parts), saved position by position: a key, a set element, a map value. Loading
/// it is `None` when one of its FormIDs belongs to an absent plugin, so the entry holding it can be dropped. A plain
/// `u32` is one FormID; a tuple of two `u32` is two (every such pair the saved state keys by is two FormIDs: a
/// reference and its keyword, two factions, a speaker and its line); `(u32, i16)` is a quest and an alias index;
/// `(u16, u32)` is a command code and the FormID it was given.
pub trait StableForms: Sized {
    type Wire: Serialize + for<'de> Deserialize<'de>;
    fn to_wire(&self) -> Self::Wire;
    fn from_wire(wire: Self::Wire) -> Option<Self>;
}

impl StableForms for u32 {
    type Wire = SavedForm;
    fn to_wire(&self) -> SavedForm {
        SavedForm::save(*self)
    }
    fn from_wire(wire: SavedForm) -> Option<u32> {
        wire.load()
    }
}

impl StableForms for (u32, u32) {
    type Wire = (SavedForm, SavedForm);
    fn to_wire(&self) -> Self::Wire {
        (SavedForm::save(self.0), SavedForm::save(self.1))
    }
    fn from_wire((a, b): Self::Wire) -> Option<Self> {
        let (a, b) = (a.load(), b.load());
        Some((a?, b?))
    }
}

impl StableForms for (u32, i16) {
    type Wire = (SavedForm, i16);
    fn to_wire(&self) -> Self::Wire {
        (SavedForm::save(self.0), self.1)
    }
    fn from_wire((form, index): Self::Wire) -> Option<Self> {
        Some((form.load()?, index))
    }
}

impl StableForms for (u16, u32) {
    type Wire = (u16, SavedForm);
    fn to_wire(&self) -> Self::Wire {
        (self.0, SavedForm::save(self.1))
    }
    fn from_wire((code, form): Self::Wire) -> Option<Self> {
        Some((code, form.load()?))
    }
}

/// An optional FormID inside a collection: an absent plugin's reads as `None`, the entry kept.
impl<T: StableForms> StableForms for Option<T> {
    type Wire = Option<T::Wire>;
    fn to_wire(&self) -> Self::Wire {
        self.as_ref().map(T::to_wire)
    }
    fn from_wire(wire: Self::Wire) -> Option<Self> {
        Some(wire.and_then(T::from_wire))
    }
}

/// A list of FormIDs inside a collection: an absent plugin's elements are dropped, the list kept.
impl<T: StableForms> StableForms for Vec<T> {
    type Wire = Vec<T::Wire>;
    fn to_wire(&self) -> Self::Wire {
        self.iter().map(T::to_wire).collect()
    }
    fn from_wire(wire: Self::Wire) -> Option<Self> {
        Some(wire.into_iter().filter_map(T::from_wire).collect())
    }
}

/// `#[serde(with = "stable_form_id::forms")]` on one field of any [`StableForms`] shape (`Option<Vec<u32>>`,
/// `(u32, u32)`, ...). A value whose FormID names an absent plugin reads as its `Default`.
pub mod forms {
    use super::*;

    pub fn serialize<T: StableForms, S: Serializer>(value: &T, ser: S) -> Result<S::Ok, S::Error> {
        value.to_wire().serialize(ser)
    }

    pub fn deserialize<'de, T: StableForms + Default, D: Deserializer<'de>>(de: D) -> Result<T, D::Error> {
        Ok(T::from_wire(T::Wire::deserialize(de)?).unwrap_or_default())
    }
}

/// `#[serde(with = "stable_form_id::vec")]` on a collection of [`StableForms`] (`Vec<u32>`, `HashSet<u32>`,
/// `BTreeSet<(u32, u32)>`, ...). An element with an absent plugin's FormID is dropped.
pub mod vec {
    use super::*;

    pub fn serialize<'a, C, T, S>(collection: &'a C, ser: S) -> Result<S::Ok, S::Error>
    where
        &'a C: IntoIterator<Item = &'a T>,
        T: StableForms + 'a,
        S: Serializer,
    {
        collection.into_iter().map(T::to_wire).collect::<Vec<_>>().serialize(ser)
    }

    pub fn deserialize<'de, C, T, D>(de: D) -> Result<C, D::Error>
    where
        C: IntoIterator<Item = T> + FromIterator<T>,
        T: StableForms,
        D: Deserializer<'de>,
    {
        Ok(Vec::<T::Wire>::deserialize(de)?.into_iter().filter_map(T::from_wire).collect())
    }
}

/// `#[serde(with = "stable_form_id::map")]` on a map keyed by [`StableForms`] (`HashMap<u32, V>`,
/// `BTreeMap<(u32, u32), V>`, ...), written as a list of `[key, value]` entries because a stable identity is not a
/// JSON object key. An entry whose key names an absent plugin's form is dropped; the values are saved as they are.
pub mod map {
    use super::*;

    pub fn serialize<'a, M, K, V, S>(entries: &'a M, ser: S) -> Result<S::Ok, S::Error>
    where
        &'a M: IntoIterator<Item = (&'a K, &'a V)>,
        K: StableForms + 'a,
        V: Serialize + 'a,
        S: Serializer,
    {
        entries.into_iter().map(|(key, value)| (key.to_wire(), value)).collect::<Vec<_>>().serialize(ser)
    }

    pub fn deserialize<'de, M, K, V, D>(de: D) -> Result<M, D::Error>
    where
        M: IntoIterator<Item = (K, V)> + FromIterator<(K, V)>,
        K: StableForms,
        V: Deserialize<'de>,
        D: Deserializer<'de>,
    {
        Ok(Vec::<(K::Wire, V)>::deserialize(de)?
            .into_iter()
            .filter_map(|(key, value)| K::from_wire(key).map(|key| (key, value)))
            .collect())
    }
}

/// `#[serde(with = "stable_form_id::map_forms")]` on a map whose keys *and* values are [`StableForms`]
/// (`BTreeMap<u32, u32>` of a reference and its owner, `BTreeMap<u32, Vec<u32>>` of a form list and what was added
/// to it). An entry is dropped when its key, or a single-FormID value, names an absent plugin's form.
pub mod map_forms {
    use super::*;

    pub fn serialize<'a, M, K, V, S>(entries: &'a M, ser: S) -> Result<S::Ok, S::Error>
    where
        &'a M: IntoIterator<Item = (&'a K, &'a V)>,
        K: StableForms + 'a,
        V: StableForms + 'a,
        S: Serializer,
    {
        entries.into_iter().map(|(key, value)| (key.to_wire(), value.to_wire())).collect::<Vec<_>>().serialize(ser)
    }

    pub fn deserialize<'de, M, K, V, D>(de: D) -> Result<M, D::Error>
    where
        M: IntoIterator<Item = (K, V)> + FromIterator<(K, V)>,
        K: StableForms,
        V: StableForms,
        D: Deserializer<'de>,
    {
        Ok(Vec::<(K::Wire, V::Wire)>::deserialize(de)?
            .into_iter()
            .filter_map(|(key, value)| Some((K::from_wire(key)?, V::from_wire(value)?)))
            .collect())
    }
}

/// Round trips for the tests of saved types across crates: a fixed pair of load orders.
#[doc(hidden)]
pub mod test_support {
    use super::*;
    use crate::form_id::LocalFormId;

    /// Regular slots only: slot `i` is `self.0[i]`.
    pub struct Order(pub Vec<PluginId>);

    impl FormIdTranslator for Order {
        fn pair(&self, global: u32) -> Option<FormIdPair> {
            let plugin = *self.0.get((global >> 24) as usize)?;
            Some(FormIdPair { plugin, local: LocalFormId(global & 0x00FF_FFFF) })
        }
        fn global(&self, pair: FormIdPair) -> Option<u32> {
            let slot = self.0.iter().position(|plugin| *plugin == pair.plugin)?;
            Some(((slot as u32) << 24) | pair.local.0)
        }
    }

    /// The plugin `Skyrim.esm` at slot 0 in both orders.
    pub fn game() -> PluginId {
        PluginId::from_filename("Skyrim.esm")
    }
    /// `ModA.esp`, slot 1 when saved, absent when loaded.
    pub fn removed() -> PluginId {
        PluginId::from_filename("ModA.esp")
    }
    /// `ModB.esp`, slot 2 when saved, slot 1 when loaded.
    pub fn shifted() -> PluginId {
        PluginId::from_filename("ModB.esp")
    }

    /// `value` saved under `[game, ModA, ModB]` and loaded under `[game, ModB]`: a `0x02xxxxxx` FormID comes back as
    /// `0x01xxxxxx`, a `0x01xxxxxx` one is absent, a `0x00xxxxxx` one is unchanged. Also checks that the saved JSON
    /// holds no `0x01`/`0x02`-slot number, which would mean a FormID the adapters did not reach (pass the plain
    /// numbers the value legitimately holds, counts or indices, in `allowed_numbers`).
    pub fn shift<T: Serialize + for<'de> Deserialize<'de>>(value: &T, allowed_numbers: &[u64]) -> (T, TranslationReport) {
        let (json, _) = with_translator(Rc::new(Order(vec![game(), removed(), shifted()])), || {
            serde_json::to_value(value).expect("serialize")
        });
        let mut stray = Vec::new();
        find_slot_numbers(&json, allowed_numbers, &mut stray);
        assert!(stray.is_empty(), "FormIDs saved as numbers, which no adapter converted: {stray:x?} in {json}");
        with_translator(Rc::new(Order(vec![game(), shifted()])), || serde_json::from_value(json).expect("deserialize"))
    }

    fn find_slot_numbers(json: &serde_json::Value, allowed: &[u64], out: &mut Vec<u64>) {
        match json {
            serde_json::Value::Number(n) => {
                if let Some(n) = n.as_u64().filter(|n| matches!(n >> 24, 1 | 2) && !allowed.contains(n)) {
                    out.push(n);
                }
            }
            serde_json::Value::Array(items) => items.iter().for_each(|item| find_slot_numbers(item, allowed, out)),
            serde_json::Value::Object(map) => {
                for (key, value) in map {
                    if let Ok(n) = key.parse::<u64>()
                        && matches!(n >> 24, 1 | 2)
                        && !allowed.contains(&n)
                    {
                        out.push(n);
                    }
                    find_slot_numbers(value, allowed, out);
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
#[path = "stable_form_id_tests.rs"]
mod tests;
