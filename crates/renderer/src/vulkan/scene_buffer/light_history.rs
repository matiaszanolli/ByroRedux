//! Translate reservoir indices across the per-frame light priority sort.
use super::{GpuLight, MAX_LIGHTS};
use crate::vulkan::sync::MAX_FRAMES_IN_FLIGHT;
use std::collections::HashMap;

const INVALID: u32 = u32::MAX;
type Identity = [u32; 4];

#[derive(Default)]
pub(super) struct LightHistory {
    ids: [Vec<Identity>; MAX_FRAMES_IN_FLIGHT],
    // Previous index/count and current index/count. Duplicate identities on
    // either side are ambiguous and must never silently select another lamp.
    scratch: HashMap<Identity, (u32, u32, u32, u32)>,
}

impl LightHistory {
    pub(super) fn remap(&mut self, frame: usize, lights: &[GpuLight]) -> [u32; MAX_LIGHTS + 1] {
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
        for (index, light) in lights.iter().take(MAX_LIGHTS).enumerate() {
            if let Some(entry) = self.scratch.get_mut(&light.history_id) {
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

    pub(super) fn commit(&mut self, frame: usize, lights: &[GpuLight]) {
        self.ids[frame].clear();
        self.ids[frame].extend(lights.iter().take(MAX_LIGHTS).map(|light| light.history_id));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn light(id: u32) -> GpuLight {
        GpuLight {
            history_id: [id, 1, 0, 0],
            ..Default::default()
        }
    }

    #[test]
    fn follows_identity_through_sort_animation_and_motion() {
        let mut history = LightHistory::default();
        history.commit(0, &[light(10), light(20), light(30)]);
        let mut moved = light(20);
        moved.position_radius = [100.0, 200.0, 300.0, 90.0];
        moved.color_type = [0.5, 0.1, 0.0, 0.0];
        assert_eq!(
            &history.remap(1, &[light(30), moved, light(10)])[..3],
            &[2, 1, 0]
        );
    }

    #[test]
    fn rejects_missing_unknown_and_ambiguous_identities() {
        let mut history = LightHistory::default();
        history.commit(
            0,
            &[light(1), light(2), light(2), light(3), GpuLight::default()],
        );
        let map = history.remap(1, &[light(2), light(3), light(3), GpuLight::default()]);
        assert!(map.iter().all(|&index| index == INVALID));
    }

    #[test]
    fn shrinking_list_can_remap_a_previous_high_index() {
        let mut history = LightHistory::default();
        history.commit(0, &[light(1), light(2), light(3)]);
        assert_eq!(&history.remap(1, &[light(3)])[..3], &[INVALID, INVALID, 0]);
    }

    #[test]
    fn reads_previous_slot_and_does_not_advance_until_commit() {
        let mut history = LightHistory::default();
        history.commit(0, &[light(1), light(2)]);
        assert_eq!(&history.remap(1, &[light(2), light(1)])[..2], &[1, 0]);
        assert_eq!(&history.remap(1, &[light(1), light(2)])[..2], &[0, 1]);
        history.commit(1, &[light(2), light(1)]);
        assert_eq!(&history.remap(0, &[light(1), light(2)])[..2], &[1, 0]);
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
        let lights = [light(1), light(2)];
        history.commit(0, &lights);
        let first = history.remap(1, &lights);
        history.commit(0, &[light(2), light(1)]);
        let second = history.remap(1, &lights);
        // The SSBO dirty gate must hash the mapping as well as current lights.
        assert_ne!(first, second);
    }
}
