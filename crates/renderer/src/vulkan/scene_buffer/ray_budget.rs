//! GPU-time-driven ray allocation shared by surface and volumetric lighting.

/// PAF-007: the direct-shadow filter's à-trous iteration range. The floor
/// is the pre-penumbra-aware chain (strides 1, 2: a 6 px reach); the
/// ceiling adds strides 4 and 8 (a 30 px reach) at ray quality tier 3.
pub const SHADOW_FILTER_MIN_ITERATIONS: u32 = 2;
pub const SHADOW_FILTER_MAX_ITERATIONS: u32 = 4;

/// PAF-012: the lowest ray quality tier that runs the penumbra-aware blocker
/// search. Below it the filter runs the pre-PAF chain — its 2-iteration floor
/// at full reach, no search, no Gaussian (`pixelAngle` 0) — so the adaptive
/// budget sheds the search with the shadow samples (M-LIGHT A13, #6603: the
/// search cost +0.24 ms in Prospector and +0.31 ms in Dugout at tier 0, the
/// tier both settle at under TAA). The reference gates read tier 3 and the
/// contact gate tiers 1-3, so neither sees the floor.
pub const SHADOW_FILTER_PENUMBRA_SEARCH_MIN_TIER: u32 = 1;

/// RCG-UPD-001 / RCG-PERF-002: the radiance cache's update-path stride per ray quality tier. One update
/// path is traced from one pixel of each `stride x stride` block of the frame, so the update fraction is
/// `1 / stride^2`; the block position varies between frames. A lower tier reduces this rate and never
/// removes the cache lookup (RCG-COV-004). Initial values, set against the A7 gate in Phase 1 of the
/// Radiance Cache and GI Coverage PRD; nothing reads the word until the cache's update pass exists.
pub const CACHE_UPDATE_STRIDE_BY_TIER: [u32; 4] = [8, 6, 5, 4];

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
    /// PAF-007: à-trous iterations the direct-shadow filter runs this frame
    /// (strides 1, 2, 4, 8; reach 2·(2^n − 1) px). Read by the CPU
    /// dispatch only; the shader copy keeps the word so the layout holds.
    pub shadow_filter_iterations: u32,
    /// RCG-UPD-001: the radiance cache's update stride this frame (see [`CACHE_UPDATE_STRIDE_BY_TIER`]);
    /// `0` traces no update paths. A policy word like the ones above, CPU-written for the whole frame.
    pub cache_update_stride: u32,
    pub lod_fragments: u32,
    pub lod_bin_0: u32,
    pub lod_bin_1: u32,
    pub lod_bin_2: u32,
    pub lod_bin_3: u32,
    pub reflection_traced: u32,
    pub reflection_lod_culled: u32,
    pub gi_traced: u32,
    pub gi_lod_culled: u32,
    /// RCG-DBG-002: traced-share pixels the radiance cache resolved this frame. Shader-written
    /// like the words above (telemetry only); `0` until a shader looks the cache up.
    pub cache_resolved: u32,
    /// RCG-DBG-002 / RCG-COV-003: lookups that missed and fell back to the untraced share for the
    /// frame. `0` until a shader looks the cache up, so a zero here is not evidence of coverage.
    pub cache_misses: u32,
}

impl GpuRayBudget {
    pub const WORDS: usize = 20;

    pub const fn words(self) -> [u32; Self::WORDS] {
        [
            self.ray_count,
            self.glass_ray_limit,
            self.direct_shadow_samples,
            self.max_path_segments,
            self.max_shaded_hits,
            self.volumetric_light_cap,
            self.quality_tier,
            self.shadow_filter_iterations,
            self.cache_update_stride,
            self.lod_fragments,
            self.lod_bin_0,
            self.lod_bin_1,
            self.lod_bin_2,
            self.lod_bin_3,
            self.reflection_traced,
            self.reflection_lod_culled,
            self.gi_traced,
            self.gi_lod_culled,
            self.cache_resolved,
            self.cache_misses,
        ]
    }

    const fn settings(
        glass_ray_limit: u32,
        direct_shadow_samples: u32,
        max_path_segments: u32,
        max_shaded_hits: u32,
        volumetric_light_cap: u32,
        quality_tier: u32,
        shadow_filter_iterations: u32,
    ) -> Self {
        let last_tier = CACHE_UPDATE_STRIDE_BY_TIER.len() - 1;
        let stride_tier = if (quality_tier as usize) < last_tier {
            quality_tier as usize
        } else {
            last_tier
        };
        Self {
            ray_count: 0,
            glass_ray_limit,
            direct_shadow_samples,
            max_path_segments,
            max_shaded_hits,
            volumetric_light_cap,
            quality_tier,
            shadow_filter_iterations,
            cache_update_stride: CACHE_UPDATE_STRIDE_BY_TIER[stride_tier],
            lod_fragments: 0,
            lod_bin_0: 0,
            lod_bin_1: 0,
            lod_bin_2: 0,
            lod_bin_3: 0,
            reflection_traced: 0,
            reflection_lod_culled: 0,
            gi_traced: 0,
            gi_lod_culled: 0,
            cache_resolved: 0,
            cache_misses: 0,
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
    pub cache_resolved: u32,
    pub cache_misses: u32,
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
            cache_resolved: value.cache_resolved,
            cache_misses: value.cache_misses,
        }
    }
}

/// Hysteretic controller targeting the main lighting pass rather than a
/// hard-coded GPU model. It reacts immediately to sustained overload and only
/// spends recovered headroom after a long stable window.
#[derive(Debug, Clone, Default)]
pub struct AdaptiveRayBudget {
    // Cold-start conservatively. Before the first GPU timer sample, starting
    // above tier zero can launch watchdog-scale work with no feedback yet.
    tier: u32,
    smoothed_lighting_ms: Option<f32>,
    /// #7076: the smoothed cost of the pass with no tier-attributable work, the tier-0 floor (see
    /// [`Self::thresholds`]). Tracked live while at tier 0 and only ever lowered above it, since a
    /// higher tier cannot cost less than the floor of the same scene.
    floor_lighting_ms: Option<f32>,
    /// #7076: a climb the controller had to undo because the tier cost more than the floor's
    /// allowance; see [`RefusedClimb`].
    refused_climb: Option<RefusedClimb>,
    under_budget_frames: u32,
    cooldown_frames: u32,
}

/// #7076: with the target raised for a heavy floor, a tier whose added cost exceeds the allowance
/// would be climbed, undone and climbed again every `UPGRADE_STABILITY_FRAMES + COOLDOWN_FRAMES`
/// frames (measured on FO4 Galleria: floor 12.2 ms, tier 1 adds 7.6 ms). The controller remembers
/// the added cost it measured when it undid the climb and does not climb into that tier again while
/// the current floor plus that cost would still be over the down threshold, or until the memory
/// expires.
#[derive(Debug, Clone, Copy, PartialEq)]
struct RefusedClimb {
    /// The tier that proved too dear.
    tier: u32,
    /// Its measured added cost over the floor, ms.
    added_ms: f32,
    /// Frames until the memory expires and the climb is tried again.
    frames_left: u32,
}

impl AdaptiveRayBudget {
    /// How long a refused climb stays refused (30 s at 60 fps): a heavy scene is retried now and then
    /// in case it got lighter in ways the floor does not show.
    const REFUSAL_FRAMES: u32 = 1800;
    const TARGET_LIGHTING_MS: f32 = 11.0;
    const DOWN_THRESHOLD_MS: f32 = Self::TARGET_LIGHTING_MS * 1.12;
    const UP_THRESHOLD_MS: f32 = Self::TARGET_LIGHTING_MS * 0.68;
    const UPGRADE_STABILITY_FRAMES: u32 = 45;
    const COOLDOWN_FRAMES: u32 = 30;
    /// #7076: what the tiers may add on top of the tier-0 floor when the floor alone is over the
    /// target, in ms: the width of the controller's own hysteresis band (`DOWN - UP`, 4.84 ms).
    /// Measured on FO4 Downtown Fallon's (release, 1280x720, 2026-11-20) a tier adds +2.8 ms (1),
    /// +5.5 ms (2) and +14.0 ms (3) to a 13.1 ms floor, so the allowance holds that scene at tier 1.
    const ADDED_COST_ALLOWANCE_MS: f32 = Self::DOWN_THRESHOLD_MS - Self::UP_THRESHOLD_MS;

    /// `(up, down)` thresholds on the smoothed pass cost. The pass cost contains work no tier
    /// changes (raster, material reads, direct shadows), so a scene whose floor alone is over the
    /// target could never climb out of tier 0, where the diffuse path allowance is zero (#7076).
    /// The target therefore moves up to `floor + ADDED_COST_ALLOWANCE_MS` when the floor is that
    /// high, and both thresholds keep their distances from it. A scene whose floor leaves the
    /// normal target room (`floor + allowance <= TARGET`) gets exactly the original thresholds.
    fn thresholds(&self) -> (f32, f32) {
        let target = self
            .floor_lighting_ms
            .map_or(Self::TARGET_LIGHTING_MS, |floor| {
                Self::TARGET_LIGHTING_MS.max(floor + Self::ADDED_COST_ALLOWANCE_MS)
            });
        (
            target - (Self::TARGET_LIGHTING_MS - Self::UP_THRESHOLD_MS),
            target + (Self::DOWN_THRESHOLD_MS - Self::TARGET_LIGHTING_MS),
        )
    }

    /// Whether the next tier up is the one the controller already found too dear for this floor.
    fn climb_is_refused(&self) -> bool {
        let (Some(refused), Some(floor)) = (self.refused_climb, self.floor_lighting_ms) else {
            return false;
        };
        refused.tier == self.tier + 1
            && refused.frames_left > 0
            && floor + refused.added_ms > self.thresholds().1
    }

    fn spend_stable_headroom(&mut self, max_tier: u32) {
        if self.cooldown_frames > 0 {
            self.cooldown_frames -= 1;
            return;
        }
        if self.tier >= max_tier || self.climb_is_refused() {
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
        // A tier above 0 reached open-loop (no timer yet) has no floor to learn from: the original
        // thresholds hold until the controller next sits at tier 0.
        self.floor_lighting_ms = match (self.tier, self.floor_lighting_ms) {
            (0, _) => Some(smoothed),
            (_, floor) => floor.map(|floor| floor.min(smoothed)),
        };
        let (up_threshold, down_threshold) = self.thresholds();
        if let Some(refused) = self.refused_climb.as_mut() {
            refused.frames_left = refused.frames_left.saturating_sub(1);
        }

        if smoothed > down_threshold && self.tier > 0 {
            if self.cooldown_frames > 0 {
                self.cooldown_frames -= 1;
                return;
            }
            // Only a raised target can cycle: a scene with room under the normal target keeps the
            // original controller exactly.
            if let Some(floor) = self.floor_lighting_ms.filter(|floor| {
                floor + Self::ADDED_COST_ALLOWANCE_MS > Self::TARGET_LIGHTING_MS
            }) {
                self.refused_climb = Some(RefusedClimb {
                    tier: self.tier,
                    added_ms: smoothed - floor,
                    frames_left: Self::REFUSAL_FRAMES,
                });
            }
            self.tier -= 1;
            self.under_budget_frames = 0;
            self.cooldown_frames = Self::COOLDOWN_FRAMES;
        } else if smoothed < up_threshold && self.tier < 3 {
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
        use crate::shader_constants::{
            GLASS_RAY_BUDGET, MAX_DIRECT_SHADOW_SAMPLES, MAX_FROXEL_LIGHTS, MAX_PATH_SEGMENTS,
            MAX_RAY_QUALITY_TIER, MAX_SHADED_HITS,
        };
        match tier {
            0 => GpuRayBudget::settings(
                GLASS_RAY_BUDGET / 8,
                1,
                // True safe floor: gather GPU timing with direct shadows but
                // no diffuse path. A non-zero minimum here defeated the
                // controller on Cydonia: the 97k-instance first frame could
                // lose the device before a timing sample existed.
                0,
                0,
                2,
                0,
                SHADOW_FILTER_MIN_ITERATIONS,
            ),
            1 => GpuRayBudget::settings(
                GLASS_RAY_BUDGET / 4,
                2,
                3,
                1,
                4,
                1,
                SHADOW_FILTER_MIN_ITERATIONS,
            ),
            2 => GpuRayBudget::settings(
                GLASS_RAY_BUDGET / 2,
                4,
                4,
                2,
                6,
                2,
                SHADOW_FILTER_MIN_ITERATIONS + 1,
            ),
            // #3879 — the tier-3 arm is the shader-side ceiling. Every
            // field now reads the same constant the GLSL `#define` is
            // generated from, so raising one actually widens the loop
            // instead of being clamped straight back down.
            _ => GpuRayBudget::settings(
                GLASS_RAY_BUDGET,
                MAX_DIRECT_SHADOW_SAMPLES,
                MAX_PATH_SEGMENTS,
                MAX_SHADED_HITS,
                MAX_FROXEL_LIGHTS,
                MAX_RAY_QUALITY_TIER,
                SHADOW_FILTER_MAX_ITERATIONS,
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// PAF-007: the filter's iteration count per tier.
    #[test]
    fn shadow_filter_iterations_grow_with_the_tier() {
        let iters: Vec<u32> = (0..=3)
            .map(|t| AdaptiveRayBudget::settings_for_tier(t).shadow_filter_iterations)
            .collect();
        assert_eq!(iters, [2, 2, 3, 4], "PAF-007: the floor keeps today's 6 px reach");
        assert!(iters
            .iter()
            .all(|n| (SHADOW_FILTER_MIN_ITERATIONS..=SHADOW_FILTER_MAX_ITERATIONS).contains(n)));
    }

    /// PAF-012: the blocker search is off at the safe floor only. Tier 0 runs
    /// the pre-PAF chain; every tier a gate reads (contact at 1-3, the
    /// reference gates at 3) keeps the search.
    #[test]
    fn penumbra_search_is_shed_at_tier_zero_only() {
        let searches: Vec<bool> = (0..=3)
            .map(|t| {
                AdaptiveRayBudget::settings_for_tier(t).quality_tier
                    >= SHADOW_FILTER_PENUMBRA_SEARCH_MIN_TIER
            })
            .collect();
        assert_eq!(searches, [false, true, true, true]);
    }

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

    /// #7076: the measured Downtown Fallon's costs (floor 13.1 ms; a tier adds 2.8, 5.5 and 14.0 ms)
    /// used to hold the controller at tier 0 for good, because 13.1 ms is over the 7.48 ms climb
    /// threshold. It now leaves tier 0, stops at the tier whose added cost fits the allowance, and
    /// does not oscillate afterwards.
    #[test]
    fn a_scene_whose_floor_is_over_the_target_still_leaves_tier_zero_and_settles() {
        const FLOOR_MS: f32 = 13.1;
        const ADDED_MS: [f32; 4] = [0.0, 2.8, 5.5, 14.0];
        let mut controller = AdaptiveRayBudget::default();
        let mut tiers = Vec::new();
        for _ in 0..1500 {
            let tier = controller.settings().quality_tier;
            tiers.push(tier);
            controller.observe(Some(FLOOR_MS + ADDED_MS[tier as usize]));
        }
        let settled = *tiers.last().unwrap();
        assert_eq!(settled, 1, "the +2.8 ms tier fits the allowance, the +5.5 ms tier does not");
        let changes = tiers.windows(2).filter(|w| w[0] != w[1]).count();
        assert_eq!(changes, 1, "one climb and no oscillation: {changes} tier changes");
    }

    /// #7076: a floor of 12.2 ms where tier 1 adds 7.6 ms (FO4 Galleria, measured) is over the raised
    /// target's down threshold, so the climb is undone; it must stay undone instead of cycling every
    /// 75 frames, and the memory must expire so a scene that got lighter is retried.
    #[test]
    fn a_tier_dearer_than_the_allowance_is_not_climbed_again_and_cycled() {
        const FLOOR_MS: f32 = 12.2;
        const ADDED_MS: [f32; 4] = [0.0, 7.6, 9.0, 20.0];
        let run = |frames: usize| {
            let mut controller = AdaptiveRayBudget::default();
            let mut tiers = Vec::new();
            for _ in 0..frames {
                let tier = controller.settings().quality_tier;
                tiers.push(tier);
                controller.observe(Some(FLOOR_MS + ADDED_MS[tier as usize]));
            }
            tiers
        };
        let tiers = run(1500);
        let climbs = tiers.windows(2).filter(|w| w[1] > w[0]).count();
        assert_eq!(climbs, 1, "one probe, one refusal: {climbs} climbs in 1500 frames");
        assert_eq!(*tiers.last().unwrap(), 0);
        // After the refusal expires the climb is tried once more.
        let long = run(1500 + AdaptiveRayBudget::REFUSAL_FRAMES as usize);
        let climbs = long.windows(2).filter(|w| w[1] > w[0]).count();
        assert_eq!(climbs, 2, "the refusal expires: {climbs} climbs");
    }

    /// #7076: a refusal does not outlive the scene it was measured in. When the floor falls far enough
    /// that the same added cost fits, the climb is taken at once.
    #[test]
    fn a_refused_tier_is_taken_when_the_floor_falls_enough_for_it_to_fit() {
        let mut controller = AdaptiveRayBudget::default();
        for _ in 0..400 {
            let tier = controller.settings().quality_tier;
            controller.observe(Some(12.2 + [0.0, 7.6, 9.0, 20.0][tier as usize]));
        }
        assert_eq!(controller.settings().quality_tier, 0);
        assert!(controller.refused_climb.is_some());
        for _ in 0..400 {
            let tier = controller.settings().quality_tier;
            controller.observe(Some(2.0 + [0.0, 7.6, 9.0, 20.0][tier as usize]));
        }
        assert!(controller.settings().quality_tier >= 1, "the lighter scene takes tier 1");
    }

    /// #7076: a scene that leaves the target its normal room keeps exactly the original thresholds,
    /// so the fix cannot change the scenes that already worked.
    #[test]
    fn a_scene_with_room_under_the_target_keeps_the_original_thresholds() {
        let mut controller = AdaptiveRayBudget::default();
        controller.observe(Some(5.0));
        let (up, down) = controller.thresholds();
        assert_eq!(
            (up, down),
            (AdaptiveRayBudget::UP_THRESHOLD_MS, AdaptiveRayBudget::DOWN_THRESHOLD_MS)
        );
        // At the edge of the room the target still has not moved; one ms past it, it has.
        let edge = AdaptiveRayBudget::TARGET_LIGHTING_MS - AdaptiveRayBudget::ADDED_COST_ALLOWANCE_MS;
        let mut at_edge = AdaptiveRayBudget::default();
        at_edge.observe(Some(edge));
        assert_eq!(at_edge.thresholds(), (up, down));
        let mut past_edge = AdaptiveRayBudget::default();
        past_edge.observe(Some(edge + 1.0));
        assert!(past_edge.thresholds().0 > up && past_edge.thresholds().1 > down);
    }

    /// #7076: above tier 0 the floor is only ever lowered, so a scene that gets lighter cannot keep
    /// the inflated target of the heavy one, and a tier reached open-loop never invents a floor.
    #[test]
    fn the_floor_only_falls_above_tier_zero() {
        let mut controller = AdaptiveRayBudget::default();
        for _ in 0..300 {
            let tier = controller.settings().quality_tier;
            controller.observe(Some(13.1 + [0.0, 2.8, 5.5, 14.0][tier as usize]));
        }
        assert_eq!(controller.settings().quality_tier, 1);
        let heavy = controller.thresholds().0;
        for _ in 0..300 {
            controller.observe(Some(4.0));
        }
        assert!(controller.thresholds().0 < heavy, "a lighter scene lowers the target again");

        let mut open_loop = AdaptiveRayBudget::default();
        for _ in 0..100 {
            open_loop.observe(None);
        }
        assert!(open_loop.settings().quality_tier > 0);
        open_loop.observe(Some(30.0));
        assert_eq!(
            open_loop.thresholds(),
            (AdaptiveRayBudget::UP_THRESHOLD_MS, AdaptiveRayBudget::DOWN_THRESHOLD_MS),
            "no floor was learned at a tier above 0"
        );
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

    /// RCG-DBG-002: the cache's resolved and miss counts are the last two ABI words, they start every
    /// frame at zero and they reach the `rt-lod-telemetry:` sample unchanged.
    #[test]
    fn the_cache_counters_are_the_last_two_words_and_reach_the_telemetry_sample() {
        let fresh = AdaptiveRayBudget::settings_for_tier(3);
        assert_eq!((fresh.cache_resolved, fresh.cache_misses), (0, 0));
        let mut read_back = fresh;
        read_back.cache_resolved = 7;
        read_back.cache_misses = 3;
        let words = read_back.words();
        assert_eq!(words[GpuRayBudget::WORDS - 2..], [7, 3]);
        let sample = RtLodTelemetry::from(read_back);
        assert_eq!((sample.cache_resolved, sample.cache_misses), (7, 3));
        let logger = include_str!("../context/sync_and_acquire_frame.rs");
        assert!(logger.contains("cache_resolved={} cache_misses={}"));
    }

    /// RCG-UPD-001 / RCG-COV-004: a lower tier reduces the update rate and never turns it off, so the
    /// stride is positive at every tier and never grows with the tier; the word is in the shader ABI and
    /// the CPU-written words stay one per field.
    #[test]
    fn the_cache_update_stride_is_positive_and_falls_as_the_tier_rises() {
        let strides: Vec<u32> = (0..=3)
            .map(|t| AdaptiveRayBudget::settings_for_tier(t).cache_update_stride)
            .collect();
        assert_eq!(strides, CACHE_UPDATE_STRIDE_BY_TIER);
        assert!(strides.iter().all(|&s| s > 0), "no tier may stop updating the cache");
        assert!(strides.windows(2).all(|w| w[0] >= w[1]), "a higher tier updates at least as often");
        let words = AdaptiveRayBudget::settings_for_tier(2).words();
        assert_eq!(words.len(), GpuRayBudget::WORDS);
        assert_eq!(words[8], 5, "the stride sits after shadow_filter_iterations, before the telemetry words");
    }

    /// The GLSL `RayBudgetBuffer` block is the other half of this struct's ABI (set 1, binding 11): it must
    /// declare exactly [`GpuRayBudget::WORDS`] `uint` members, in the Rust field order. Read from
    /// `bindings.glsl`, so a field added on one side only fails here and not as a silently shifted counter.
    #[test]
    fn the_glsl_block_declares_the_rust_words_in_order() {
        let bindings = include_str!("../../../shaders/include/bindings.glsl");
        let start = bindings
            .find("coherent buffer RayBudgetBuffer {")
            .expect("the ray budget block");
        let body = &bindings[start..];
        let body = &body[body.find('{').unwrap() + 1..body.find("} rayBudget;").unwrap()];
        let members: Vec<&str> = body
            .lines()
            .filter_map(|line| line.trim().strip_prefix("uint "))
            .map(|rest| rest.split(|c: char| c == ';' || c.is_whitespace()).next().unwrap())
            .collect();
        assert_eq!(members.len(), GpuRayBudget::WORDS, "block members: {members:?}");
        assert_eq!(
            members,
            [
                "rayBudgetCount",
                "glassRayLimit",
                "directShadowSamples",
                "maxPathSegments",
                "maxShadedHits",
                "volumetricLightCap",
                "qualityTier",
                "shadowFilterIterations",
                "cacheUpdateStride",
                "rtLodFragments",
                "rtLodBin0",
                "rtLodBin1",
                "rtLodBin2",
                "rtLodBin3",
                "rtReflectionTraced",
                "rtReflectionLodCulled",
                "rtGiTraced",
                "rtGiLodCulled",
                "rtCacheResolved",
                "rtCacheMisses",
            ]
        );
    }

    #[test]
    fn shader_contract_fits_in_the_aligned_slot() {
        assert_eq!(std::mem::size_of::<GpuRayBudget>(), 80);
        assert!(std::mem::size_of::<GpuRayBudget>() as u64 <= super::super::RAY_BUDGET_STRIDE);
    }
}
