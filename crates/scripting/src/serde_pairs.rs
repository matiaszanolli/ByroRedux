//! Saved maps keyed by a tuple — `(faction, other faction)`, `(command id,
//! reference)`, `(location, keyword)` — written as a list of `[key, value]`
//! pairs (`#[serde(with = "crate::serde_pairs")]`).
//!
//! The save format is JSON, whose object keys must be strings, and a
//! tuple-keyed map derives as an object: `serde_json` refuses it ("key must
//! be a string") the moment it holds an entry, and the whole snapshot fails.
//! Found when the FNV route's first `SetAlly` (VCG02 stage 10) put one row in
//! `ObScriptLedger` and every later quicksave failed.
//!
//! A save written before this adapter holds such a map as an object, and an
//! object could only ever have been written empty, so reading accepts an
//! empty object as the empty map. Nothing was persisted that is now lost.

use std::marker::PhantomData;

use serde::de::{self, Deserialize, Deserializer, IgnoredAny, MapAccess, SeqAccess, Visitor};
use serde::{Serialize, Serializer};

pub fn serialize<'a, M, K, V, S>(map: &'a M, serializer: S) -> Result<S::Ok, S::Error>
where
    &'a M: IntoIterator<Item = (&'a K, &'a V)>,
    K: Serialize + 'a,
    V: Serialize + 'a,
    S: Serializer,
{
    serializer.collect_seq(map)
}

pub fn deserialize<'de, M, K, V, D>(deserializer: D) -> Result<M, D::Error>
where
    M: FromIterator<(K, V)>,
    K: Deserialize<'de>,
    V: Deserialize<'de>,
    D: Deserializer<'de>,
{
    struct Pairs<M, K, V>(PhantomData<(M, K, V)>);

    impl<'de, M, K, V> Visitor<'de> for Pairs<M, K, V>
    where
        M: FromIterator<(K, V)>,
        K: Deserialize<'de>,
        V: Deserialize<'de>,
    {
        type Value = M;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("a list of [key, value] pairs")
        }

        fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<M, A::Error> {
            let mut pairs = Vec::with_capacity(seq.size_hint().unwrap_or(0));
            while let Some(pair) = seq.next_element::<(K, V)>()? {
                pairs.push(pair);
            }
            Ok(pairs.into_iter().collect())
        }

        fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<M, A::Error> {
            if map.next_entry::<IgnoredAny, IgnoredAny>()?.is_some() {
                return Err(de::Error::custom(
                    "a tuple-keyed map saved as an object can only be empty",
                ));
            }
            Ok(std::iter::empty().collect())
        }
    }

    deserializer.deserialize_any(Pairs(PhantomData))
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, HashMap};

    #[derive(Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
    struct Saved {
        #[serde(with = "super")]
        ordered: BTreeMap<(u16, u32), f32>,
        #[serde(with = "super")]
        hashed: HashMap<(u32, u32), u32>,
    }

    /// The derive's object form fails on the first entry; the pair list
    /// round-trips.
    #[test]
    fn tuple_keyed_maps_round_trip_through_json() {
        let plain: BTreeMap<(u32, u32), f32> = BTreeMap::from([((0x0010_4C66, 0x14), 1.0)]);
        assert!(serde_json::to_value(&plain).is_err(), "the bug this adapter fixes");

        let saved = Saved {
            ordered: BTreeMap::from([((0x1159, 0x14), 3.0), ((0x1159, 0x0010_4E85), -1.5)]),
            hashed: HashMap::from([((0x0010_A214, 0x0002_C5F2), 7)]),
        };
        let json = serde_json::to_value(&saved).unwrap();
        assert_eq!(json["ordered"], serde_json::json!([[[4441, 20], 3.0], [[4441, 1068677], -1.5]]));
        assert_eq!(serde_json::from_value::<Saved>(json).unwrap(), saved);
    }

    /// A save from before the adapter wrote these maps as objects, which can
    /// only have been empty; it still loads.
    #[test]
    fn an_empty_object_from_an_older_save_reads_as_empty() {
        let old = serde_json::json!({ "ordered": {}, "hashed": {} });
        assert_eq!(serde_json::from_value::<Saved>(old).unwrap(), Saved::default());
        let impossible = serde_json::json!({ "ordered": { "x": 1.0 }, "hashed": {} });
        assert!(serde_json::from_value::<Saved>(impossible).is_err());
    }
}
