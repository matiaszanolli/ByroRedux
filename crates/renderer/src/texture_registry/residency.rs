//! M39 texture residency: which levels of a streamable texture's mip chain
//! are on the GPU, and the plan that moves them
//! (docs/engine/texture-streaming.md §3, docs/prd/texture-streaming.md
//! TXS-RES). Pure bookkeeping, so the rules are testable without a device;
//! `Texture::record_dds_residency_change` executes a plan.

use ash::vk;

/// The floor edge, in texels: a streamable texture's floor is the levels
/// whose larger edge is at most this. 256 px is where the last BA2 DX10
/// chunk tops out for 97.5 % of FO4's and 89.4 % of Starfield's multi-chunk
/// records (design §2.1), so the BSA and loose games share BA2's floor
/// (decision Q3).
pub(crate) const FLOOR_EDGE_PX: u32 = 256;

/// The first level of the floor of a `width × height` chain of `mip_count`
/// levels, or `None` when the texture is **whole** (not streamable): a
/// single level, or a level 0 already within the floor edge. A truncated
/// chain whose smallest level is still above the edge floors at that
/// smallest level.
pub(crate) fn floor_first_level(width: u32, height: u32, mip_count: u32) -> Option<u32> {
    if mip_count <= 1 {
        return None;
    }
    let edge = width.max(height).max(1);
    let mut level = 0;
    while level + 1 < mip_count && (edge >> level).max(1) > FLOOR_EDGE_PX {
        level += 1;
    }
    (level > 0).then_some(level)
}

/// The resident range `[first_resident, mip_count)` of one streamable
/// texture. `first_resident` never rises above `floor_first`: the floor is
/// always resident while the texture has a holder.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Residency {
    pub mip_count: u32,
    pub floor_first: u32,
    pub first_resident: u32,
    /// Bytes the floor levels hold (never evicted, TXS-BUD-002).
    pub floor_bytes: u64,
}

impl Residency {
    /// A texture that has just had its floor uploaded.
    pub(crate) fn at_floor(mip_count: u32, floor_first: u32, floor_bytes: u64) -> Self {
        Self {
            mip_count,
            floor_first,
            first_resident: floor_first,
            floor_bytes,
        }
    }

    /// Levels in the image, which is also what its view spans
    /// (TXS-RES-003: no view addresses a non-resident level).
    pub(crate) fn resident_levels(&self) -> u32 {
        self.mip_count - self.first_resident
    }

    /// The change that makes `target` the first resident level, clamped to
    /// `[0, floor_first]`, or `None` when nothing would move.
    pub(crate) fn plan(&self, target: u32) -> Option<ResidencyChange> {
        let new_first = target.min(self.floor_first);
        (new_first != self.first_resident).then_some(ResidencyChange {
            mip_count: self.mip_count,
            old_first: self.first_resident,
            new_first,
        })
    }

    /// The residency after `change` lands. A change planned against another
    /// state (stale by the time it flushes) is refused.
    pub(crate) fn apply(&mut self, change: ResidencyChange) -> bool {
        if change.old_first != self.first_resident || change.mip_count != self.mip_count {
            return false;
        }
        self.first_resident = change.new_first;
        true
    }
}

/// One promotion (`new_first < old_first`) or demotion
/// (`new_first > old_first`): a new image of levels
/// `[new_first, mip_count)` replaces the image of `[old_first, mip_count)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ResidencyChange {
    pub mip_count: u32,
    pub old_first: u32,
    pub new_first: u32,
}

impl ResidencyChange {
    pub(crate) fn is_promotion(&self) -> bool {
        self.new_first < self.old_first
    }

    /// Chain levels uploaded from staging: `[new_first, old_first)` for a
    /// promotion, none for a demotion.
    pub(crate) fn upload_levels(&self) -> std::ops::Range<u32> {
        if self.is_promotion() {
            self.new_first..self.old_first
        } else {
            self.new_first..self.new_first
        }
    }

    /// Chain levels both images hold, copied from the old one on the GPU.
    pub(crate) fn shared_levels(&self) -> std::ops::Range<u32> {
        self.old_first.max(self.new_first)..self.mip_count
    }

    /// `vkCmdCopyImage` regions for the shared levels: chain level `L` is
    /// level `L - old_first` of the old image and `L - new_first` of the
    /// new one. `width × height` is the chain's level-0 extent.
    pub(crate) fn image_copies(&self, width: u32, height: u32, layers: u32) -> Vec<vk::ImageCopy> {
        self.shared_levels()
            .map(|level| {
                let subresource = |mip_level| vk::ImageSubresourceLayers {
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    mip_level,
                    base_array_layer: 0,
                    layer_count: layers,
                };
                vk::ImageCopy {
                    src_subresource: subresource(level - self.old_first),
                    src_offset: vk::Offset3D::default(),
                    dst_subresource: subresource(level - self.new_first),
                    dst_offset: vk::Offset3D::default(),
                    extent: vk::Extent3D {
                        width: (width >> level).max(1),
                        height: (height >> level).max(1),
                        depth: 1,
                    },
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn floor_rule_streams_only_chains_above_the_edge() {
        // 2048² full chain: levels 2048..1; the 256-px level is 3.
        assert_eq!(floor_first_level(2048, 2048, 12), Some(3));
        // Non-square: the larger edge decides (4096×512 → 256 at level 4).
        assert_eq!(floor_first_level(4096, 512, 13), Some(4));
        // Within the floor already, or a single level: whole.
        assert_eq!(floor_first_level(256, 256, 9), None);
        assert_eq!(floor_first_level(2048, 2048, 1), None);
        // A truncated chain that never reaches 256 px floors at its last level.
        assert_eq!(floor_first_level(2048, 2048, 3), Some(2));
        // 512 px: one level above the floor.
        assert_eq!(floor_first_level(512, 512, 10), Some(1));
    }

    #[test]
    fn plan_clamps_to_the_floor_and_skips_no_ops() {
        let r = Residency::at_floor(12, 3, 0);
        assert_eq!(r.plan(3), None, "already there");
        assert_eq!(r.plan(9), None, "never coarser than the floor");
        let promote = r.plan(0).expect("promotion");
        assert!(promote.is_promotion());
        assert_eq!(promote.upload_levels(), 0..3);
        assert_eq!(promote.shared_levels(), 3..12);
    }

    #[test]
    fn a_stale_change_is_refused() {
        let mut r = Residency::at_floor(12, 3, 0);
        let first = r.plan(1).expect("promotion to 1");
        let second = r
            .plan(0)
            .expect("promotion to 0, planned against the floor");
        assert!(r.apply(first));
        assert!(!r.apply(second), "planned against level 3, now at 1");
        assert_eq!(r.first_resident, 1);
    }

    /// M39 TXS-RES-003 — after a promotion and after a demotion, the new
    /// image (and so the view spanning it) holds exactly the resident range:
    /// its uploaded levels and its copied levels tile `[0, levels)` of the
    /// new image once each, and every copy reads a level the old image has.
    #[test]
    fn slot_view_covers_only_resident_range() {
        let (w, h, mips) = (2048u32, 1024u32, 12u32);
        let mut r = Residency::at_floor(mips, 3, 0);
        for target in [0u32, 2, 3, 1] {
            let Some(change) = r.plan(target) else {
                continue;
            };
            let old_levels = mips - change.old_first;
            assert!(r.apply(change));
            let new_levels = mips - change.new_first;
            assert_eq!(
                new_levels,
                r.resident_levels(),
                "image levels == resident range"
            );

            let mut covered = vec![0u32; new_levels as usize];
            for level in change.upload_levels() {
                covered[(level - change.new_first) as usize] += 1;
            }
            for copy in change.image_copies(w, h, 1) {
                assert!(
                    copy.src_subresource.mip_level < old_levels,
                    "{change:?}: src in old image"
                );
                assert!(
                    copy.dst_subresource.mip_level < new_levels,
                    "{change:?}: dst in new image"
                );
                let chain_level = copy.dst_subresource.mip_level + change.new_first;
                assert_eq!(
                    copy.src_subresource.mip_level + change.old_first,
                    chain_level
                );
                assert_eq!(copy.extent.width, (w >> chain_level).max(1));
                assert_eq!(copy.extent.height, (h >> chain_level).max(1));
                covered[copy.dst_subresource.mip_level as usize] += 1;
            }
            assert!(covered.iter().all(|&n| n == 1), "{change:?}: {covered:?}");
        }
        assert_eq!(r.first_resident, 1);
    }
}
