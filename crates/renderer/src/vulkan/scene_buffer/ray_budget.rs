//! GPU-time-driven ray allocation shared by surface and volumetric lighting.

/// Shader contract for scene set 1, binding 11.
///
/// The first word remains the atomic glass-work telemetry counter. The
/// remaining words are immutable for the duration of a frame and select
/// bounded loop limits. `glass_ray_limit` is retained in the ABI for telemetry
/// comparison; it is not a per-fragment admission threshold because unordered
/// atomic winners would split a glass surface between incompatible paths.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GpuRayBudget {
    pub ray_count: u32,
    pub glass_ray_limit: u32,
    pub direct_shadow_samples: u32,
    pub max_path_segments: u32,
    pub max_shaded_hits: u32,
    pub volumetric_light_cap: u32,
    pub quality_tier: u32,
    pub reserved: u32,
    pub lod_fragments: u32,
    pub lod_bin_0: u32,
    pub lod_bin_1: u32,
    pub lod_bin_2: u32,
    pub lod_bin_3: u32,
    pub reflection_traced: u32,
    pub reflection_lod_culled: u32,
    pub gi_traced: u32,
    pub gi_lod_culled: u32,
}

impl GpuRayBudget {
    pub const WORDS: usize = 17;

    pub const fn words(self) -> [u32; Self::WORDS] {
        [
            self.ray_count,
            self.glass_ray_limit,
            self.direct_shadow_samples,
            self.max_path_segments,
            self.max_shaded_hits,
            self.volumetric_light_cap,
            self.quality_tier,
            self.reserved,
            self.lod_fragments,
            self.lod_bin_0,
            self.lod_bin_1,
            self.lod_bin_2,
            self.lod_bin_3,
            self.reflection_traced,
            self.reflection_lod_culled,
            self.gi_traced,
            self.gi_lod_culled,
        ]
    }

    const fn settings(
        glass_ray_limit: u32,
        direct_shadow_samples: u32,
        max_path_segments: u32,
        max_shaded_hits: u32,
        volumetric_light_cap: u32,
        quality_tier: u32,
    ) -> Self {
        Self {
            ray_count: 0,
            glass_ray_limit,
            direct_shadow_samples,
            max_path_segments,
            max_shaded_hits,
            volumetric_light_cap,
            quality_tier,
            reserved: 0,
            lod_fragments: 0,
            lod_bin_0: 0,
            lod_bin_1: 0,
            lod_bin_2: 0,
            lod_bin_3: 0,
            reflection_traced: 0,
            reflection_lod_culled: 0,
            gi_traced: 0,
            gi_lod_culled: 0,
        }
    }
}

/// Fence-lagged counters for a single instrumented main pass.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct RtLodTelemetry {
    pub fragments: u32,
    pub bins: [u32; 4],
    pub reflection_traced: u32,
    pub reflection_lod_culled: u32,
    pub gi_traced: u32,
    pub gi_lod_culled: u32,
}

impl From<GpuRayBudget> for RtLodTelemetry {
    fn from(value: GpuRayBudget) -> Self {
        Self {
            fragments: value.lod_fragments,
            bins: [
                value.lod_bin_0,
                value.lod_bin_1,
                value.lod_bin_2,
                value.lod_bin_3,
            ],
            reflection_traced: value.reflection_traced,
            reflection_lod_culled: value.reflection_lod_culled,
            gi_traced: value.gi_traced,
            gi_lod_culled: value.gi_lod_culled,
        }
    }
}

/// Hysteretic controller targeting the main lighting pass rather than a
/// hard-coded GPU model. It reacts immediately to sustained overload and only
/// spends recovered headroom after a long stable window.
#[derive(Debug, Clone)]
pub struct AdaptiveRayBudget {
    tier: u32,
    smoothed_lighting_ms: Option<f32>,
    under_budget_frames: u32,
    cooldown_frames: u32,
}

impl Default for AdaptiveRayBudget {
    fn default() -> Self {
        Self {
            // Cold-start conservatively. Until the first completed GPU timer
            // sample exists the controller cannot know whether the scene is a
            // Cornell box or Cydonia's 97k-instance TLAS. Starting at tier 2
            // made that unknown first frame launch the four-shadow/two-hit GI
            // workload and could trip a device watchdog before feedback had a
            // chance to reduce it.
            tier: 0,
            smoothed_lighting_ms: None,
            under_budget_frames: 0,
            cooldown_frames: 0,
        }
    }
}

impl AdaptiveRayBudget {
    const TARGET_LIGHTING_MS: f32 = 11.0;
    const DOWN_THRESHOLD_MS: f32 = Self::TARGET_LIGHTING_MS * 1.12;
    const UP_THRESHOLD_MS: f32 = Self::TARGET_LIGHTING_MS * 0.68;
    const UPGRADE_STABILITY_FRAMES: u32 = 45;
    const COOLDOWN_FRAMES: u32 = 30;

    fn spend_stable_headroom(&mut self, max_tier: u32) {
        if self.cooldown_frames > 0 {
            self.cooldown_frames -= 1;
            return;
        }
        if self.tier >= max_tier {
            self.under_budget_frames = 0;
            return;
        }
        self.under_budget_frames += 1;
        if self.under_budget_frames >= Self::UPGRADE_STABILITY_FRAMES {
            self.tier += 1;
            self.under_budget_frames = 0;
            self.cooldown_frames = Self::COOLDOWN_FRAMES;
        }
    }

    pub fn observe(&mut self, measured_lighting_ms: Option<f32>) {
        let Some(sample) = measured_lighting_ms.filter(|ms| ms.is_finite() && *ms > 0.0) else {
            // Timestamp queries are diagnostic, not a prerequisite for GI.
            // Run open-loop to the normal tier-2 budget, retaining tier 3 as
            // measured headroom only so unknown hardware never cold-starts at
            // the maximum workload.
            self.spend_stable_headroom(2);
            return;
        };
        let smoothed = self
            .smoothed_lighting_ms
            .map_or(sample, |old| old + (sample - old) * 0.12);
        self.smoothed_lighting_ms = Some(smoothed);

        if smoothed > Self::DOWN_THRESHOLD_MS && self.tier > 0 {
            if self.cooldown_frames > 0 {
                self.cooldown_frames -= 1;
                return;
            }
            self.tier -= 1;
            self.under_budget_frames = 0;
            self.cooldown_frames = Self::COOLDOWN_FRAMES;
        } else if smoothed < Self::UP_THRESHOLD_MS && self.tier < 3 {
            self.spend_stable_headroom(3);
        } else {
            self.under_budget_frames = 0;
            self.cooldown_frames = self.cooldown_frames.saturating_sub(1);
        }
    }

    pub const fn settings(&self) -> GpuRayBudget {
        Self::settings_for_tier(self.tier)
    }

    pub const fn settings_for_tier(tier: u32) -> GpuRayBudget {
        // #2686 / SAFE-D7-01 — `glass_ray_limit` for each tier is derived
        // from `GLASS_RAY_BUDGET` (the documented single source of truth,
        // also mirrored into shader_constants.glsl) rather than an
        // independently hand-maintained literal, so editing the constant
        // actually changes the tier table instead of silently doing
        // nothing. Tiers 0-2 are power-of-two fractions of the tier-3
        // ceiling; pinned exactly by `glass_ray_limit_tiers_derive_from_
        // glass_ray_budget` below.
        use crate::shader_constants::GLASS_RAY_BUDGET;
        match tier {
            0 => GpuRayBudget::settings(
                GLASS_RAY_BUDGET / 8, 1,
                // True safe floor: gather GPU timing with direct shadows but
                // no diffuse path. A non-zero minimum here defeated the
                // controller on Cydonia: the 97k-instance first frame could
                // lose the device before a timing sample existed.
                0, 0, 2, 0,
            ),
            1 => GpuRayBudget::settings(GLASS_RAY_BUDGET / 4, 2, 3, 1, 4, 1),
            2 => GpuRayBudget::settings(GLASS_RAY_BUDGET / 2, 4, 4, 2, 6, 2),
            _ => GpuRayBudget::settings(GLASS_RAY_BUDGET, 8, 6, 2, 8, 3),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// #2686 / SAFE-D7-01 — every tier's `glass_ray_limit` must derive from
    /// `GLASS_RAY_BUDGET`, not an independently hand-maintained literal that
    /// only happens to numerically match it. Pins both the tier-3 ceiling
    /// and the tiers-0-2 fractions so editing the constant actually moves
    /// the whole table.
    #[test]
    fn glass_ray_limit_tiers_derive_from_glass_ray_budget() {
        use crate::shader_constants::GLASS_RAY_BUDGET;
        assert_eq!(
            AdaptiveRayBudget::settings_for_tier(3).glass_ray_limit,
            GLASS_RAY_BUDGET
        );
        assert_eq!(
            AdaptiveRayBudget::settings_for_tier(2).glass_ray_limit,
            GLASS_RAY_BUDGET / 2
        );
        assert_eq!(
            AdaptiveRayBudget::settings_for_tier(1).glass_ray_limit,
            GLASS_RAY_BUDGET / 4
        );
        assert_eq!(
            AdaptiveRayBudget::settings_for_tier(0).glass_ray_limit,
            GLASS_RAY_BUDGET / 8
        );
    }

    #[test]
    fn cold_start_uses_the_watchdog_safe_quality_floor() {
        let budget = AdaptiveRayBudget::default().settings();
        assert_eq!(budget.direct_shadow_samples, 1);
        assert_eq!(budget.max_path_segments, 0);
        assert_eq!(budget.max_shaded_hits, 0);
        assert_eq!(budget.quality_tier, 0);
    }

    #[test]
    fn overload_reduces_quality_without_oscillation() {
        let mut controller = AdaptiveRayBudget::default();
        controller.observe(Some(20.0));
        assert_eq!(controller.settings().quality_tier, 0);
        for _ in 0..20 {
            controller.observe(Some(1.0));
        }
        assert_eq!(controller.settings().quality_tier, 0);
    }

    #[test]
    fn stable_headroom_eventually_spends_more_rays() {
        let mut controller = AdaptiveRayBudget::default();
        for _ in 0..200 {
            controller.observe(Some(1.0));
        }
        assert_eq!(controller.settings().quality_tier, 3);
    }

    #[test]
    fn missing_timer_samples_promote_gi_to_the_normal_budget() {
        let mut controller = AdaptiveRayBudget::default();
        for _ in 0..200 {
            controller.observe(None);
        }
        let budget = controller.settings();
        assert_eq!(budget.quality_tier, 2);
        assert!(
            budget.max_path_segments > 0,
            "open-loop mode must enable GI"
        );

        for _ in 0..200 {
            controller.observe(None);
        }
        assert_eq!(
            controller.settings().quality_tier,
            2,
            "maximum quality remains gated on measured GPU headroom"
        );
    }

    #[test]
    fn shader_contract_fits_in_the_aligned_slot() {
        assert_eq!(std::mem::size_of::<GpuRayBudget>(), 68);
        assert!(std::mem::size_of::<GpuRayBudget>() as u64 <= super::super::RAY_BUDGET_STRIDE);
    }
}
