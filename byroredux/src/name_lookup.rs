//! Case-insensitive fallback for bone/skeleton-node name → value lookups.
//!
//! `StringPool`-backed `Name` components are ASCII-lowercased at intern
//! time to match Gamebryo's `GlobalStringTable` behavior, so ordinary
//! `Name` comparisons (and animation channel binding) are
//! case-insensitive. The skin-bone and ragdoll bone-name maps
//! (`node_by_name` / `external_skeleton` in `scene::nif_loader`,
//! `skel_map` / `rest_pose_by_name` in `ragdoll::template_from_imported`)
//! bypass that pool entirely and are keyed on the raw, case-preserved
//! NIF node name — two different normalisation regimes for the same
//! conceptual identifier. See #2458.
//!
//! Bethesda's own tooling is case-insensitive, so third-party/modded
//! skeletons and outfits have no incentive to be byte-exact, and a
//! case-only divergence between (say) an outfit's skin bone list and
//! `skeleton.nif`'s node names silently unresolves that bone.
//!
//! This is the "cheaper interim" fix from #2458: fall back to a
//! case-insensitive scan on an exact-match miss, with a rate-limited
//! `log::warn!` measuring real-corpus incidence, ahead of the fuller fix
//! (re-key these maps through `StringPool`/`FixedString` so every
//! bone-name comparison shares one normalisation).

use std::collections::HashMap;
use std::hash::Hash;
use std::sync::atomic::{AtomicU32, Ordering};

static CASE_FALLBACK_WARNINGS: AtomicU32 = AtomicU32::new(0);
const MAX_CASE_FALLBACK_WARNINGS: u32 = 20;

/// Look up `name` in `map`, trying an exact match first and falling back
/// to a case-insensitive scan on miss. Returns `None` if neither finds a
/// match. Logs a rate-limited warning when the case-insensitive fallback
/// is what resolves the name, so real-corpus incidence can be measured
/// before committing to the fuller `StringPool`-backed re-key (#2458).
pub(crate) fn get_case_insensitive<'a, K, V>(map: &'a HashMap<K, V>, name: &str) -> Option<&'a V>
where
    K: std::borrow::Borrow<str> + Eq + Hash + std::fmt::Display,
{
    if let Some(v) = map.get(name) {
        return Some(v);
    }
    let (key, value) = map
        .iter()
        .find(|(k, _)| k.borrow().eq_ignore_ascii_case(name))?;
    let count = CASE_FALLBACK_WARNINGS.fetch_add(1, Ordering::Relaxed);
    if count < MAX_CASE_FALLBACK_WARNINGS {
        log::warn!(
            "bone/node name '{name}' resolved only via case-insensitive fallback (matched \
             '{key}') — #2458: skin/ragdoll bone-name binding is case-sensitive while every \
             other Name comparison is case-insensitive (StringPool lowercases)."
        );
        if count + 1 == MAX_CASE_FALLBACK_WARNINGS {
            log::warn!(
                "further case-insensitive bone-name fallback warnings suppressed for this \
                 process (#2458)"
            );
        }
    }
    Some(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn map(pairs: &[(&str, u32)]) -> HashMap<Arc<str>, u32> {
        pairs.iter().map(|&(k, v)| (Arc::from(k), v)).collect()
    }

    #[test]
    fn exact_match_hits_without_scanning() {
        let m = map(&[("Bip01 Spine", 1), ("Bip01 Head", 2)]);
        assert_eq!(get_case_insensitive(&m, "Bip01 Spine"), Some(&1));
    }

    #[test]
    fn case_mismatch_resolves_via_fallback() {
        let m = map(&[("Bip01 Spine", 1)]);
        assert_eq!(get_case_insensitive(&m, "bip01 spine"), Some(&1));
    }

    #[test]
    fn unrelated_name_stays_unresolved() {
        let m = map(&[("Bip01 Spine", 1)]);
        assert_eq!(get_case_insensitive(&m, "Bip01 Pelvis"), None);
    }

    #[test]
    fn empty_map_returns_none() {
        let m: HashMap<Arc<str>, u32> = HashMap::new();
        assert_eq!(get_case_insensitive(&m, "anything"), None);
    }
}
