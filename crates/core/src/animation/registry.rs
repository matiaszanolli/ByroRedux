//! Animation clip registry (shared ECS resource).

use std::collections::HashMap;

use crate::ecs::resource::Resource;

use super::types::AnimationClip;

/// Shared registry of loaded animation clips, indexed by handle.
///
/// Slots never alias stale data: a `clip_handle: u32` issued by
/// [`Self::add`] always resolves to the same slot for the process
/// lifetime. The slot's *contents* can be cleared via
/// [`Self::release`] (called from the cell-loader's LRU eviction
/// path — see #863) to drop the keyframe arrays without needing to
/// invalidate live `AnimationPlayer` / `AnimationLayer` consumers.
/// A released slot reads as an empty clip; sampling produces
/// identity transforms, identical to a never-loaded clip — so live
/// handles stay resolvable and just stop animating.
///
/// Path-keyed memoisation via [`Self::get_or_insert_by_path`] is the
/// dedup mechanism for caller paths that load the same `.kf` repeatedly
/// (e.g. NPC spawn re-using `idle.kf` across every loaded cell). See
/// #790. `release` removes any path-binding pointing at the released
/// handle so the next `get_or_insert_by_path` for the same key
/// rebuilds rather than returning the empty stub.
pub struct AnimationClipRegistry {
    clips: Vec<AnimationClip>,
    /// Path-keyed memoisation. Populated by
    /// [`Self::get_or_insert_by_path`] and read by callers that want
    /// the cheap early-out before paying the parse cost.
    ///
    /// Keys are stored ASCII-lowercased — both
    /// [`Self::get_or_insert_by_path`] and [`Self::get_by_path`]
    /// case-fold the caller-supplied key before hash lookup, so
    /// `"Meshes\\IDLE.KF"` and `"meshes\\idle.kf"` collapse onto the
    /// same handle. This matches the pool-wide case-insensitive
    /// convention (see #895) and removes a foot-gun for future
    /// IDLE-record / Papyrus-routed callers that hand in
    /// user-authored paths without explicit normalisation (#866).
    ///
    /// Without this map, repeated registration of the same KF clip
    /// (one per cell load with NPCs) grew the registry unboundedly —
    /// the `idle.kf` keyframe arrays leaked across an entire walking
    /// session (#790).
    clip_handles_by_path: HashMap<String, u32>,
}

impl Resource for AnimationClipRegistry {}

impl AnimationClipRegistry {
    pub fn new() -> Self {
        Self {
            clips: Vec::new(),
            clip_handles_by_path: HashMap::new(),
        }
    }

    pub fn add(&mut self, clip: AnimationClip) -> u32 {
        let handle = self.clips.len() as u32;
        self.clips.push(clip);
        handle
    }

    /// Look up a previously-memoised handle for `key`. Returns `None`
    /// when the key has never been seen (or was registered via plain
    /// [`Self::add`] without a key).
    ///
    /// Case-insensitive: the key is ASCII-lowercased before hash
    /// lookup so callers can pass any case (#866). Allocation-free
    /// when `key` is already lowercase — the common case for
    /// statically-authored paths.
    pub fn get_by_path(&self, key: &str) -> Option<u32> {
        match canonicalise(key) {
            CanonKey::Borrowed(k) => self.clip_handles_by_path.get(k).copied(),
            CanonKey::Owned(k) => self.clip_handles_by_path.get(&k).copied(),
        }
    }

    /// Path-keyed memoising insert. Returns the existing handle if `key`
    /// was already registered; otherwise calls `build_clip()` to
    /// produce the clip, registers it, and memoises the handle for
    /// future lookups.
    ///
    /// `build_clip` is a closure (rather than a pre-built `AnimationClip`)
    /// so callers can short-circuit the BSA extract + NIF parse on a
    /// hit. Without that, every cell load would re-parse the same KF
    /// just to throw it away on dedup. See #790.
    ///
    /// Case-insensitive: `key` is ASCII-lowercased internally before
    /// hash lookup / insert, so future callers handing in
    /// user-authored paths from IDLE records or Papyrus
    /// `Debug.SendAnimationEvent` re-routes don't accidentally split
    /// the dedup map across case variants (#866).
    pub fn get_or_insert_by_path<F>(&mut self, key: String, build_clip: F) -> u32
    where
        F: FnOnce() -> AnimationClip,
    {
        // Avoid the lowercase allocation when the caller already
        // passed a canonical key (the production hot path —
        // `npc_spawn::ensure_idle_clip` hands in a static lowercase
        // literal). When upper-case bytes are present, fold in place
        // on the owned `String` rather than allocating a second copy.
        let key = if key.bytes().any(|b| b.is_ascii_uppercase()) {
            let mut k = key;
            k.make_ascii_lowercase();
            k
        } else {
            key
        };
        if let Some(&handle) = self.clip_handles_by_path.get(&key) {
            return handle;
        }
        let handle = self.add(build_clip());
        self.clip_handles_by_path.insert(key, handle);
        handle
    }

    pub fn get(&self, handle: u32) -> Option<&AnimationClip> {
        self.clips.get(handle as usize)
    }

    pub fn len(&self) -> usize {
        self.clips.len()
    }

    pub fn is_empty(&self) -> bool {
        self.clips.is_empty()
    }

    /// Drop the keyframe arrays of `handle`'s slot so the registry
    /// stops holding references to evicted cell-load NIF clips.
    /// Called from the cell-loader's LRU eviction path (#863) when a
    /// cached `NifImportRegistry` entry's memoised clip handle is
    /// retired.
    ///
    /// **Slot semantics**: the slot stays occupied at the same index
    /// — the contained `AnimationClip` is replaced with an empty
    /// stub (zero duration, no channels, no text keys). Live
    /// `AnimationPlayer.clip_handle` / `AnimationLayer.clip_handle`
    /// consumers still resolve via [`Self::get`] but read an empty
    /// clip; sampling produces identity transforms, identical to a
    /// never-loaded clip. This keeps the no-stale-handle invariant
    /// the rest of the engine assumes (no `clip_handle: u32` ever
    /// aliases a different clip after release) without needing to
    /// switch every holder to a generational handle scheme.
    ///
    /// Returns `true` when a populated slot was cleared, `false`
    /// when the handle was out-of-range or the slot was already
    /// empty (idempotent).
    ///
    /// Also drops any [`Self::clip_handles_by_path`] reverse-map
    /// entry pointing at this handle so a subsequent
    /// `get_or_insert_by_path` for the same key rebuilds the clip
    /// instead of returning the empty stub.
    pub fn release(&mut self, handle: u32) -> bool {
        let idx = handle as usize;
        let Some(slot) = self.clips.get_mut(idx) else {
            return false;
        };
        // Idempotency check — already-empty slot returns false so the
        // caller's release-counter telemetry doesn't double-count
        // releases against the same handle.
        let was_populated = !slot.channels.is_empty()
            || !slot.float_channels.is_empty()
            || !slot.color_channels.is_empty()
            || !slot.bool_channels.is_empty()
            || !slot.texture_flip_channels.is_empty()
            || !slot.text_keys.is_empty()
            || slot.duration > 0.0;
        if !was_populated {
            return false;
        }
        // Drain heavy collections via .clear() — the Vec/HashMap
        // capacities deallocate on the next allocator turn; the slot
        // headers stay so the slot stays addressable.
        slot.name = String::new();
        slot.duration = 0.0;
        slot.weight = 1.0;
        slot.accum_root_name = None;
        slot.channels.clear();
        slot.float_channels.clear();
        slot.color_channels.clear();
        slot.bool_channels.clear();
        slot.texture_flip_channels.clear();
        slot.text_keys.clear();
        // Path-memo cleanup: drop reverse-map entries pointing here.
        // O(N) on path-map size; called rarely (LRU eviction freq).
        self.clip_handles_by_path.retain(|_, h| *h != handle);
        true
    }
}

impl Default for AnimationClipRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Lookup-key canonicalisation result for [`AnimationClipRegistry::get_by_path`].
/// Borrowed when the caller's key is already ASCII-lowercase (zero
/// allocation — common case for static literals); owned when an
/// upper-case byte forced the case-fold copy.
enum CanonKey<'a> {
    Borrowed(&'a str),
    Owned(String),
}

#[inline]
fn canonicalise(key: &str) -> CanonKey<'_> {
    if key.bytes().any(|b| b.is_ascii_uppercase()) {
        CanonKey::Owned(key.to_ascii_lowercase())
    } else {
        CanonKey::Borrowed(key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::animation::types::{AnimationClip, CycleType};
    use std::collections::HashMap;

    fn empty_clip() -> AnimationClip {
        AnimationClip {
            name: String::new(),
            duration: 0.0,
            cycle_type: CycleType::Loop,
            frequency: 1.0,
            weight: 1.0,
            accum_root_name: None,
            channels: HashMap::new(),
            float_channels: Vec::new(),
            color_channels: Vec::new(),
            bool_channels: Vec::new(),
            texture_flip_channels: Vec::new(),
            text_keys: Vec::new(),
        }
    }

    #[test]
    fn add_returns_monotonic_handle() {
        let mut reg = AnimationClipRegistry::new();
        assert_eq!(reg.add(empty_clip()), 0);
        assert_eq!(reg.add(empty_clip()), 1);
        assert_eq!(reg.add(empty_clip()), 2);
        assert_eq!(reg.len(), 3);
    }

    /// Regression for #790: repeated `get_or_insert_by_path` calls with
    /// the same key MUST return the same handle and MUST NOT grow the
    /// underlying `clips` Vec. Pre-fix, the `load_idle_clip` path called
    /// plain `add` on every cell load, leaking one full keyframe-array
    /// copy per cell crossing.
    #[test]
    fn get_or_insert_by_path_dedupes_repeated_calls() {
        let mut reg = AnimationClipRegistry::new();
        let key = "meshes\\characters\\_male\\idle.kf";

        let h1 = reg.get_or_insert_by_path(key.to_string(), empty_clip);
        let h2 = reg.get_or_insert_by_path(key.to_string(), || {
            panic!("build_clip must not be called on a memoised hit")
        });
        let h3 = reg.get_or_insert_by_path(key.to_string(), empty_clip);

        assert_eq!(h1, h2);
        assert_eq!(h2, h3);
        assert_eq!(reg.len(), 1, "registry must hold exactly one clip");
        assert_eq!(reg.get_by_path(key), Some(h1));
    }

    /// Distinct keys round-trip independently — the dedup map keys on
    /// the full path string, not on a hash collision.
    #[test]
    fn get_or_insert_by_path_keys_are_independent() {
        let mut reg = AnimationClipRegistry::new();
        let h_male = reg.get_or_insert_by_path("_male\\idle.kf".to_string(), empty_clip);
        let h_female = reg.get_or_insert_by_path("_female\\idle.kf".to_string(), empty_clip);

        assert_ne!(h_male, h_female);
        assert_eq!(reg.len(), 2);
        assert_eq!(reg.get_by_path("_male\\idle.kf"), Some(h_male));
        assert_eq!(reg.get_by_path("_female\\idle.kf"), Some(h_female));
        assert_eq!(reg.get_by_path("missing.kf"), None);
    }

    /// Regression for #866: keys differing only in case must collapse
    /// onto the same handle. Pre-fix the registry took the caller's
    /// key verbatim; a future IDLE-record or Papyrus-routed caller
    /// handing in a user-authored path without `.to_ascii_lowercase()`
    /// would have silently split the dedup map and resurrected the
    /// #790 leak.
    #[test]
    fn get_or_insert_by_path_is_case_insensitive() {
        let mut reg = AnimationClipRegistry::new();
        let h_lower =
            reg.get_or_insert_by_path("meshes\\idle.kf".to_string(), empty_clip);
        let h_upper = reg.get_or_insert_by_path("MESHES\\IDLE.KF".to_string(), || {
            panic!("build_clip must not be called — case variant should hit the memo")
        });
        let h_mixed = reg.get_or_insert_by_path("Meshes\\Idle.KF".to_string(), || {
            panic!("build_clip must not be called — case variant should hit the memo")
        });

        assert_eq!(h_lower, h_upper);
        assert_eq!(h_upper, h_mixed);
        assert_eq!(reg.len(), 1, "all case variants must share one slot");

        // get_by_path canonicalises too — any case round-trips.
        assert_eq!(reg.get_by_path("MESHES\\IDLE.KF"), Some(h_lower));
        assert_eq!(reg.get_by_path("Meshes\\Idle.KF"), Some(h_lower));
        assert_eq!(reg.get_by_path("meshes\\idle.kf"), Some(h_lower));
    }

    /// `add()` (the un-keyed path) doesn't populate the path map —
    /// callers that want dedup must opt in via `get_or_insert_by_path`.
    /// This keeps the existing un-keyed callers working unchanged.
    #[test]
    fn plain_add_does_not_populate_path_map() {
        let mut reg = AnimationClipRegistry::new();
        let _ = reg.add(empty_clip());
        assert_eq!(reg.get_by_path("anything"), None);
    }

    fn populated_clip() -> AnimationClip {
        use crate::animation::types::{TransformChannel, KeyType};
        let mut clip = empty_clip();
        clip.name = "evicted_clip".to_string();
        clip.duration = 1.5;
        clip.text_keys.push((0.5, crate::string::StringPool::new().intern("evt")));
        clip.channels.insert(
            crate::string::StringPool::new().intern("Bip01 Pelvis"),
            TransformChannel {
                translation_keys: Vec::new(),
                rotation_keys: Vec::new(),
                scale_keys: Vec::new(),
                translation_type: KeyType::Linear,
                rotation_type: KeyType::Linear,
                scale_type: KeyType::Linear,
                priority: 0,
            },
        );
        clip
    }

    /// Regression for #863: `release(handle)` drops the slot's
    /// keyframe arrays so the cell-loader LRU eviction path can stop
    /// the unbounded `AnimationClipRegistry` growth without
    /// invalidating live `clip_handle: u32` consumers. The slot stays
    /// addressable at the same index; sampling reads an empty clip
    /// (identical to a never-loaded one).
    #[test]
    fn release_clears_slot_keyframes_but_keeps_slot_addressable() {
        let mut reg = AnimationClipRegistry::new();
        let h = reg.add(populated_clip());

        // Sanity: the slot starts populated.
        let pre = reg.get(h).expect("slot must exist after add");
        assert_eq!(pre.duration, 1.5);
        assert_eq!(pre.channels.len(), 1);
        assert_eq!(pre.text_keys.len(), 1);

        let cleared = reg.release(h);
        assert!(cleared, "release must return true for a populated slot");

        // Slot still addressable — live handles still resolve.
        let post = reg.get(h).expect("slot must remain addressable after release");
        assert_eq!(post.duration, 0.0);
        assert!(post.channels.is_empty());
        assert!(post.text_keys.is_empty());
        assert!(post.float_channels.is_empty());
        // Length unchanged — slot count is monotonic.
        assert_eq!(reg.len(), 1);
    }

    #[test]
    fn release_is_idempotent_on_empty_slot() {
        let mut reg = AnimationClipRegistry::new();
        let h = reg.add(populated_clip());
        assert!(reg.release(h));
        // Second release sees an already-empty slot — returns false
        // so caller's telemetry doesn't double-count.
        assert!(!reg.release(h));
    }

    #[test]
    fn release_returns_false_for_out_of_range_handle() {
        let mut reg = AnimationClipRegistry::new();
        let _ = reg.add(empty_clip());
        assert!(!reg.release(99), "out-of-range handle must return false");
    }

    /// Path-memo cleanup: a released handle's path-binding is dropped
    /// so the next `get_or_insert_by_path` rebuilds rather than
    /// returning the empty stub.
    #[test]
    fn release_drops_path_binding_so_next_get_or_insert_rebuilds() {
        let mut reg = AnimationClipRegistry::new();
        let key = "meshes\\evicted.kf";
        let h1 = reg.get_or_insert_by_path(key.to_string(), populated_clip);
        assert_eq!(reg.get_by_path(key), Some(h1));

        reg.release(h1);

        // Path-map binding gone: the next get_or_insert_by_path with
        // the same key returns a NEW handle pointing at a freshly-
        // populated clip (not the empty h1 stub).
        assert_eq!(reg.get_by_path(key), None);
        let h2 = reg.get_or_insert_by_path(key.to_string(), populated_clip);
        assert_ne!(h1, h2, "post-release rebuild must allocate a fresh slot");
        // h1 stays empty (live handles get the no-op behaviour); h2
        // is the populated rebuild.
        assert_eq!(reg.get(h1).unwrap().duration, 0.0);
        assert_eq!(reg.get(h2).unwrap().duration, 1.5);
    }
}
