//! M39 texture pool budget (docs/engine/texture-streaming.md §7,
//! docs/prd/texture-streaming.md TXS-BUD, decision Q4).
//!
//! One number of bytes bounds what streamed textures may keep resident above
//! their floors. This module is the arithmetic only — no device, no registry
//! — so the derivation and the ledger constants are testable on their own;
//! `VulkanContext::refresh_texture_budget` gathers the inputs and
//! `TextureRegistry::apply_wanted` spends the result.

use ash::vk;

/// The headroom fraction `F` of design §7.1, in parts per thousand: textures
/// may use this share of the device-local capacity, less everything else the
/// process already holds. 80 % is the engine's existing "approaching OOM"
/// line (`allocator::approaching_oom_line`), so a texture budget never plans
/// past the point the allocator already warns at. It is a policy choice, not
/// a measured optimum: A8's minimum-card emulation (the override setting)
/// is the check that it leaves room on a small card.
pub const BUDGET_FRACTION_PERMILLE: u64 = 800;

/// The smallest budget the derivation returns. A transiently large
/// non-texture figure (a cell load's staging, a BLAS build's scratch) must
/// not collapse the budget to zero and evict every mip in the scene; below
/// this the derivation holds at the floor and the over-budget warning
/// (TXS-BUD-002) names the shortfall. An explicit override is not clamped.
pub const MIN_DERIVED_BUDGET_BYTES: u64 = 128 << 20;

/// TXS-BUD-006 — promotion upload bytes submitted per frame. One 4096² BC7
/// chain is ~21 MiB and one 2048² BC1 chain ~2.7 MiB, so this admits a
/// handful of promotions a frame while bounding the copy-queue stall one
/// frame can take. Recorded in `docs/engine/memory-budget.md` § Texture
/// Registry; `memory_budget_ledger_tests` holds that row to this value.
pub const MAX_PROMOTION_UPLOAD_BYTES_PER_FRAME: vk::DeviceSize = 32 << 20;

/// TXS-BUD-006 — residency changes (promotions and demotions) flushed per
/// frame. A demotion uploads nothing, so the byte cap alone would let a budget
/// drop land thousands of image replacements in one frame; each costs an image
/// allocation and a GPU copy of the levels it keeps.
pub const MAX_RESIDENCY_CHANGES_PER_FRAME: usize = 64;

/// Where the budget came from (printed by `tex.residency`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BudgetSource {
    /// `render.texture_budget_mib` was non-zero.
    Override,
    /// `VK_EXT_memory_budget`'s live DEVICE_LOCAL budget.
    LiveBudget,
    /// The smallest DEVICE_LOCAL heap, on a driver without the extension.
    SmallestHeap,
}

impl BudgetSource {
    pub fn label(self) -> &'static str {
        match self {
            Self::Override => "override setting",
            Self::LiveBudget => "live VK_EXT_memory_budget",
            Self::SmallestHeap => "smallest DEVICE_LOCAL heap",
        }
    }
}

/// What the derivation reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BudgetInputs {
    /// The device-local capacity the fraction applies to.
    pub capacity_bytes: u64,
    /// `true` when `capacity_bytes` is the live extension budget.
    pub capacity_is_live: bool,
    /// Device-local bytes this process holds now, textures included.
    pub in_use_bytes: u64,
    /// Of those, the bytes held by texture images (live and retiring).
    pub texture_bytes: u64,
    /// Static-BLAS growth still allowed under its own budget: bytes the
    /// acceleration manager may yet take that are not in `in_use_bytes`.
    pub blas_headroom_bytes: u64,
    /// `render.texture_budget_mib` in bytes; 0 means derive.
    pub override_bytes: u64,
}

/// The derived budget and the terms it came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextureBudget {
    pub bytes: u64,
    pub source: BudgetSource,
    pub capacity_bytes: u64,
    /// `in_use − textures + blas_headroom`: what the budget leaves to
    /// everything that is not a texture.
    pub reserve_bytes: u64,
}

/// `budget = F × capacity − reserve` (design §7.1), or the override.
pub fn derive_budget(input: &BudgetInputs) -> TextureBudget {
    let reserve_bytes = input
        .in_use_bytes
        .saturating_sub(input.texture_bytes)
        .saturating_add(input.blas_headroom_bytes);
    if input.override_bytes > 0 {
        return TextureBudget {
            bytes: input.override_bytes,
            source: BudgetSource::Override,
            capacity_bytes: input.capacity_bytes,
            reserve_bytes,
        };
    }
    let fraction = (u128::from(input.capacity_bytes) * u128::from(BUDGET_FRACTION_PERMILLE)
        / 1000) as u64;
    TextureBudget {
        bytes: fraction
            .saturating_sub(reserve_bytes)
            .max(MIN_DERIVED_BUDGET_BYTES),
        source: if input.capacity_is_live {
            BudgetSource::LiveBudget
        } else {
            BudgetSource::SmallestHeap
        },
        capacity_bytes: input.capacity_bytes,
        reserve_bytes,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GIB: u64 = 1 << 30;

    fn inputs() -> BudgetInputs {
        BudgetInputs {
            capacity_bytes: 12 * GIB,
            capacity_is_live: true,
            in_use_bytes: 4 * GIB,
            texture_bytes: GIB,
            blas_headroom_bytes: GIB / 2,
            override_bytes: 0,
        }
    }

    /// Q4 — a fraction of the live budget, less what is not a texture.
    #[test]
    fn budget_is_a_fraction_of_capacity_less_the_non_texture_reserve() {
        let b = derive_budget(&inputs());
        // reserve = (4 − 1) + 0.5 GiB; fraction = 0.8 × 12 = 9.6 GiB.
        assert_eq!(b.reserve_bytes, 3 * GIB + GIB / 2);
        assert_eq!(b.bytes, 12 * GIB * 8 / 10 - b.reserve_bytes);
        assert_eq!(b.source, BudgetSource::LiveBudget);
    }

    /// The reserve excludes the textures' own bytes: spending the budget on
    /// textures must not shrink it.
    #[test]
    fn textures_growing_do_not_shrink_their_own_budget() {
        let mut grown = inputs();
        grown.in_use_bytes += 2 * GIB;
        grown.texture_bytes += 2 * GIB;
        assert_eq!(derive_budget(&grown), derive_budget(&inputs()));
    }

    /// A driver without the extension falls back to the smallest heap.
    #[test]
    fn without_the_extension_the_smallest_heap_is_the_capacity() {
        let mut i = inputs();
        i.capacity_is_live = false;
        i.capacity_bytes = 6 * GIB;
        assert_eq!(derive_budget(&i).source, BudgetSource::SmallestHeap);
    }

    /// A card with nothing left over holds at the floor instead of zero.
    #[test]
    fn a_crowded_card_holds_at_the_minimum_budget() {
        let mut i = inputs();
        i.in_use_bytes = 11 * GIB;
        i.texture_bytes = 0;
        assert_eq!(derive_budget(&i).bytes, MIN_DERIVED_BUDGET_BYTES);
    }

    /// TXS-BUD-005 — the override replaces the whole expression, unclamped,
    /// so the minimum card can be emulated on the dev card.
    #[test]
    fn the_override_replaces_the_derivation() {
        let mut i = inputs();
        i.override_bytes = 64 << 20;
        let b = derive_budget(&i);
        assert_eq!((b.bytes, b.source), (64 << 20, BudgetSource::Override));
    }
}

/// TXS-BUD-006 / TXS-TEL-004 — the constants the budget spends are rows of
/// `docs/engine/memory-budget.md` § Texture Registry; this holds the page to
/// the code.
#[cfg(test)]
mod memory_budget_ledger_tests {
    use super::*;

    fn texture_registry_section() -> &'static str {
        let doc = include_str!("../../../../docs/engine/memory-budget.md");
        let section = doc
            .split_once("\n## Texture Registry\n")
            .expect("memory-budget.md must keep its Texture Registry section")
            .1;
        &section[..section.find("\n## ").unwrap_or(section.len())]
    }

    fn row(prefix: &str) -> &'static str {
        texture_registry_section()
            .lines()
            .find(|line| line.starts_with(prefix))
            .unwrap_or_else(|| panic!("no `{prefix}` row in the Texture Registry ledger"))
    }

    #[test]
    fn memory_budget_ledgers_the_promotion_cap() {
        let row = row("| Promotion upload cap |");
        assert!(row.contains("`MAX_PROMOTION_UPLOAD_BYTES_PER_FRAME`"), "{row}");
        assert!(
            row.contains(&format!(
                "**{} MiB**",
                MAX_PROMOTION_UPLOAD_BYTES_PER_FRAME >> 20
            )),
            "memory-budget.md states a stale promotion cap: {row}"
        );
    }

    #[test]
    fn memory_budget_ledgers_the_change_count_cap() {
        let row = row("| Promotion upload cap |");
        assert!(row.contains("`MAX_RESIDENCY_CHANGES_PER_FRAME`"), "{row}");
        assert!(
            row.contains(&format!("**{}** changes", MAX_RESIDENCY_CHANGES_PER_FRAME)),
            "memory-budget.md states a stale change cap: {row}"
        );
    }

    #[test]
    fn memory_budget_ledgers_the_budget_derivation() {
        let row = row("| Texture pool budget |");
        assert!(
            row.contains(&format!("**{BUDGET_FRACTION_PERMILLE}** ‰")),
            "stale budget fraction: {row}"
        );
        assert!(
            row.contains(&format!("**{} MiB**", MIN_DERIVED_BUDGET_BYTES >> 20)),
            "stale minimum budget: {row}"
        );
    }

    /// TXS-TEL-004 — the "no eviction policy" paragraph is gone with
    /// the eviction that replaces it.
    #[test]
    fn the_no_eviction_paragraph_is_rewritten() {
        let section = texture_registry_section();
        assert!(!section.contains("no explicit texture-count eviction policy"));
        assert!(section.contains("Texture pool and eviction (M39"));
    }
}
