//! Animation clip registry (shared ECS resource).

use std::collections::HashMap;

use crate::ecs::resource::Resource;

use super::types::AnimationClip;

/// Shared registry of loaded animation clips, indexed by handle.
///
/// The registry is grow-only by design: handles never alias stale data
/// after a cell unload, so any held `clip_handle: u32` (in
/// `AnimationStack` layers, `AnimationController` catalogs, etc.) is
/// guaranteed to point at the same clip for the process lifetime.
///
/// Path-keyed memoisation via [`Self::get_or_insert_by_path`] is the
/// dedup mechanism for caller paths that load the same `.kf` repeatedly
/// (e.g. NPC spawn re-using `idle.kf` across every loaded cell). See
/// #790.
pub struct AnimationClipRegistry {
    clips: Vec<AnimationClip>,
    /// Path-keyed memoisation. Populated by
    /// [`Self::get_or_insert_by_path`] and read by callers that want
    /// the cheap early-out before paying the parse cost. Keys are
    /// caller-normalised (typically a lowercased archive path).
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
    pub fn get_by_path(&self, key: &str) -> Option<u32> {
        self.clip_handles_by_path.get(key).copied()
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
    pub fn get_or_insert_by_path<F>(&mut self, key: String, build_clip: F) -> u32
    where
        F: FnOnce() -> AnimationClip,
    {
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
}

impl Default for AnimationClipRegistry {
    fn default() -> Self {
        Self::new()
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

    /// `add()` (the un-keyed path) doesn't populate the path map —
    /// callers that want dedup must opt in via `get_or_insert_by_path`.
    /// This keeps the existing un-keyed callers working unchanged.
    #[test]
    fn plain_add_does_not_populate_path_map() {
        let mut reg = AnimationClipRegistry::new();
        let _ = reg.add(empty_clip());
        assert_eq!(reg.get_by_path("anything"), None);
    }
}
