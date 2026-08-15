//! GPU-time-driven ray allocation shared by surface and volumetric lighting.

/// Shader contract for scene set 1, binding 11.
///
/// The first word remains the atomic glass-ray counter. The remaining words
/// are immutable for the duration of a frame and select bounded loop limits.
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
}

impl GpuRayBudget {
    pub const WORDS: usize = 8;

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
        ]
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

    pub fn observe(&mut self, measured_lighting_ms: Option<f32>) {
        let Some(sample) = measured_lighting_ms.filter(|ms| ms.is_finite() && *ms > 0.0) else {
            return;
        };
        let smoothed = self
            .smoothed_lighting_ms
            .map_or(sample, |old| old + (sample - old) * 0.12);
        self.smoothed_lighting_ms = Some(smoothed);

        if self.cooldown_frames > 0 {
            self.cooldown_frames -= 1;
            return;
        }
        if smoothed > Self::DOWN_THRESHOLD_MS && self.tier > 0 {
            self.tier -= 1;
            self.under_budget_frames = 0;
            self.cooldown_frames = Self::COOLDOWN_FRAMES;
        } else if smoothed < Self::UP_THRESHOLD_MS && self.tier < 3 {
            self.under_budget_frames += 1;
            if self.under_budget_frames >= Self::UPGRADE_STABILITY_FRAMES {
                self.tier += 1;
                self.under_budget_frames = 0;
                self.cooldown_frames = Self::COOLDOWN_FRAMES;
            }
        } else {
            self.under_budget_frames = 0;
        }
    }

    pub const fn settings(&self) -> GpuRayBudget {
        match self.tier {
            0 => GpuRayBudget {
                ray_count: 0,
                glass_ray_limit: 262_144,
                direct_shadow_samples: 1,
                // True safe floor: gather GPU timing with direct shadows but
                // no diffuse path. A non-zero minimum here defeated the
                // controller on Cydonia: the 97k-instance first frame could
                // lose the device before a timing sample existed.
                max_path_segments: 0,
                max_shaded_hits: 0,
                volumetric_light_cap: 2,
                quality_tier: 0,
                reserved: 0,
            },
            1 => GpuRayBudget {
                ray_count: 0,
                glass_ray_limit: 524_288,
                direct_shadow_samples: 2,
                max_path_segments: 3,
                max_shaded_hits: 1,
                volumetric_light_cap: 4,
                quality_tier: 1,
                reserved: 0,
            },
            2 => GpuRayBudget {
                ray_count: 0,
                glass_ray_limit: 1_048_576,
                direct_shadow_samples: 4,
                max_path_segments: 4,
                max_shaded_hits: 2,
                volumetric_light_cap: 6,
                quality_tier: 2,
                reserved: 0,
            },
            _ => GpuRayBudget {
                ray_count: 0,
                glass_ray_limit: 2_097_152,
                direct_shadow_samples: 8,
                max_path_segments: 6,
                max_shaded_hits: 2,
                volumetric_light_cap: 8,
                quality_tier: 3,
                reserved: 0,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn shader_contract_fits_in_the_aligned_slot() {
        assert_eq!(std::mem::size_of::<GpuRayBudget>(), 32);
        assert!(std::mem::size_of::<GpuRayBudget>() as u64 <= super::super::RAY_BUDGET_STRIDE);
    }
}
