//! Translate reservoir indices across the per-frame light priority sort.
use super::MAX_LIGHTS;
use crate::vulkan::sync::MAX_FRAMES_IN_FLIGHT;
use rustc_hash::FxHashMap;

const INVALID: u32 = u32::MAX;
type Identity = [u32; 4];

/// #5055 — the ReSTIR remap identities this module tracks are CPU-only.
/// They ride a `[[u32; 4]]` slice parallel to the `GpuLight` upload (never
/// inside the GPU struct): authored lights use `[entity_id, 1, 0, 0]`, the
/// scene key `[0, 2, 0, 0]`, and anything else `[0; 4]` = "no identity,
/// sample fresh only".
#[derive(Default)]
pub(super) struct LightHistory {
    ids: [Vec<Identity>; MAX_FRAMES_IN_FLIGHT],
    // Previous index/count and current index/count. Duplicate identities on
    // either side are ambiguous and must never silently select another lamp.
    // #4954 — Fx, not SipHash: this runs every frame from `upload_lights`
    // (the #2923 hot-path hashing rule).
    scratch: FxHashMap<Identity, (u32, u32, u32, u32)>,
}

impl LightHistory {
    pub(super) fn remap(
        &mut self,
        frame: usize,
        identities: &[Identity],
    ) -> [u32; MAX_LIGHTS + 1] {
        let previous = (frame + MAX_FRAMES_IN_FLIGHT - 1) % MAX_FRAMES_IN_FLIGHT;
        self.scratch.clear();
        for (index, &id) in self.ids[previous].iter().enumerate() {
            if id == [0; 4] {
                continue;
            }
            let entry = self
                .scratch
                .entry(id)
                .or_insert((index as u32, 0, INVALID, 0));
            entry.1 += 1;
        }
        for (index, &id) in identities.iter().take(MAX_LIGHTS).enumerate() {
            if let Some(entry) = self.scratch.get_mut(&id) {
                entry.2 = index as u32;
                entry.3 += 1;
            }
        }
        let mut result = [INVALID; MAX_LIGHTS + 1];
        for &(old_index, old_count, new_index, new_count) in self.scratch.values() {
            if old_count == 1 && new_count == 1 {
                result[old_index as usize] = new_index;
            }
        }
        result
    }

    pub(super) fn commit(&mut self, frame: usize, identities: &[Identity]) {
        self.ids[frame].clear();
        self.ids[frame].extend(identities.iter().take(MAX_LIGHTS).copied());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// #4954 — `remap` and `upload_lights`' dirty gate run every frame; the
    /// #2923 hot-path rule keeps both on Fx hashing.
    #[test]
    fn per_frame_light_upload_does_not_use_siphash() {
        // `upload.rs` interleaves test modules with production code, so it is
        // scanned whole (no test there names either type); this file drops its
        // own test module, which spells the banned names out.
        let own = crate::source_scan::production_text(include_str!("light_history.rs"));
        for (name, production) in [("light_history.rs", own), ("upload.rs", include_str!("upload.rs"))] {
            for banned in ["std::collections::HashMap", "DefaultHasher"] {
                assert!(
                    !production.contains(banned),
                    "{name} uses `{banned}` on the per-frame light upload path (#4954 / #2923)"
                );
            }
        }
    }

    fn id(n: u32) -> Identity {
        [n, 1, 0, 0]
    }

    #[test]
    fn follows_identity_through_sort_animation_and_motion() {
        let mut history = LightHistory::default();
        history.commit(0, &[id(10), id(20), id(30)]);
        assert_eq!(
            &history.remap(1, &[id(30), id(20), id(10)])[..3],
            &[2, 1, 0]
        );
    }

    #[test]
    fn rejects_missing_unknown_and_ambiguous_identities() {
        let mut history = LightHistory::default();
        history.commit(0, &[id(1), id(2), id(2), id(3), [0; 4]]);
        let map = history.remap(1, &[id(2), id(3), id(3), [0; 4]]);
        assert!(map.iter().all(|&index| index == INVALID));
    }

    #[test]
    fn shrinking_list_can_remap_a_previous_high_index() {
        let mut history = LightHistory::default();
        history.commit(0, &[id(1), id(2), id(3)]);
        assert_eq!(&history.remap(1, &[id(3)])[..3], &[INVALID, INVALID, 0]);
    }

    #[test]
    fn reads_previous_slot_and_does_not_advance_until_commit() {
        let mut history = LightHistory::default();
        history.commit(0, &[id(1), id(2)]);
        assert_eq!(&history.remap(1, &[id(2), id(1)])[..2], &[1, 0]);
        assert_eq!(&history.remap(1, &[id(1), id(2)])[..2], &[0, 1]);
        history.commit(1, &[id(2), id(1)]);
        assert_eq!(&history.remap(0, &[id(1), id(2)])[..2], &[1, 0]);
    }

    #[test]
    fn history_header_has_exact_std430_offsets() {
        use super::super::buffers::LightHeader;
        assert_eq!(std::mem::offset_of!(LightHeader, previous_to_current), 16);
        assert_eq!(
            std::mem::size_of::<LightHeader>(),
            16 + (MAX_LIGHTS + 1) * 4
        );
        assert_eq!(std::mem::size_of::<LightHeader>() % 16, 0);
        for source in [
            include_str!("../../../shaders/include/bindings.glsl"),
            include_str!("../../../shaders/cluster_cull.comp"),
            include_str!("../../../shaders/caustic_splat.comp"),
            include_str!("../../../shaders/volumetrics_inject.comp"),
        ] {
            assert!(source
                .contains("uint previousLightToCurrent[MAX_LIGHTS + 1u];\n    GpuLight lights[];"));
        }
        let shader = include_str!("../../../shaders/triangle.frag");
        assert!(shader.contains("uint rpLightIndex = remapReservoirLight(rp.lightAndSurface);"));
        assert!(shader.contains("uint rnLightIndex = remapReservoirLight(rn.lightAndSurface);"));
    }

    #[test]
    fn same_current_lights_can_need_a_different_mapping() {
        let mut history = LightHistory::default();
        let ids = [id(1), id(2)];
        history.commit(0, &ids);
        let first = history.remap(1, &ids);
        history.commit(0, &[id(2), id(1)]);
        let second = history.remap(1, &ids);
        // The SSBO dirty gate must hash the mapping as well as current lights.
        assert_ne!(first, second);
    }
}
