//! Camera component and ActiveCamera resource.

use crate::ecs::resource::Resource;
use crate::ecs::sparse_set::SparseSetStorage;
use crate::ecs::storage::{Component, EntityId};
use crate::math::{Mat4, Vec3};

use super::transform::Transform;

/// Shared geometry render distance for every scene type.
///
/// Interior cells do not get a shorter projection: they use the same camera
/// and frustum contract as streamed exterior worldspaces. Exterior LOD
/// production decides what geometry exists at long range; it does not change
/// the camera far plane.
///
/// **It must, however, reach as far as that geometry.** The distant-LOD ring
/// is a square centred on the player, so its furthest visible point is a
/// corner at `reach · √2`, and the frustum's far plane is extracted from the
/// view-projection matrix — geometry past it is *culled*, not merely clipped,
/// so whole LOD blocks wink out at the diagonals. The binding constraint is
/// the widest ring any supported game streams:
///
/// | Ring | Reach | Corner (`·√2`) |
/// |---|---|---|
/// | Synthesized (Oblivion / FO3 / FNV) | 48 cells = 196 608 BU | 278 046 BU |
/// | Baked ladder (Skyrim / FO4) | 61 cells = 249 856 BU | 353 350 BU |
///
/// 400 000 covers the 353 350 corner with ~13% left for terrain relief above
/// and below the camera. `cell_loader::terrain_lod` const-asserts this
/// against its own `MAX_LOD_RING_REACH_CELLS`, so retuning either side fails
/// the build rather than silently clipping the horizon.
///
/// ## Depth-precision policy
///
/// This far plane against a 0.1 near plane is a 4 000 000:1 depth range on
/// a `D32_SFLOAT` buffer with a conventional (non-reversed) 0→1 mapping —
/// the arrangement that wastes float depth most thoroughly, because distant
/// samples crowd into the region near 1.0 where f32 steps are coarsest.
/// [`Camera::depth_resolution_at`] quantifies it: **~37 250 world units per
/// depth step out at the 250 000 BU LOD ring** (and ~23 000 already at the
/// old synth ring's 196 608 BU) at `near = 0.1`, i.e. effectively no depth
/// discrimination between distant terrain and the object LOD standing on
/// it.
///
/// Two ways out were identified; only one is available cheaply:
///
/// * **Raising the near plane** — done, via [`Camera::for_content_scale`] /
///   [`NEAR_PLANE_BU_SCALE`]. Every shipped Gamebryo game does this
///   (`fNearDistance` is 5 in `Fallout_default.ini`, 10 in
///   `Oblivion_default.ini`, versus the 0.1 this engine's unit-scale demo
///   scenes still need). Raising `near` to 5.0 measures **745 world units
///   per depth step** at the same 250 000 BU ring — the ~50× this doc used
///   to cite as the theoretical payoff. It could not be a single global
///   constant while one `Camera` contract serves every scene — `scene.rs`'s
///   no-content fallback parks the camera 4 units from the origin, and some
///   calibrated renderer harnesses (Cornell/combustion-lab scenes) sit at
///   comparably small physical scale despite loading real NIF content — so
///   `for_content_scale` takes an explicit scale flag rather than inferring
///   it from content presence alone; `scene.rs`'s one production call site
///   passes `has_nif_content && harness_cam.is_none()`, which is true for
///   genuine BU-scale content (loaded worldspace/interior cells, loose NIF
///   mesh/tree views) and false for both the unit-scale procedural demo and
///   every harness scene that declares its own camera pose (those own their
///   physical scale directly and are excluded rather than guessed at).
/// * **Reversed-Z** (far→0 plus `GREATER_OR_EQUAL` depth compare) is the
///   actual, complete fix, and pairs with `D32_SFLOAT` to give near-uniform
///   world-space resolution at any near-plane value. Both halves of that
///   claim are now measured rather than asserted, by
///   [`Camera::depth_resolution_at_reversed`] — at the same 250 000 BU ring
///   it resolves **0.0057 world units** (against the raised near plane's
///   745), and moving `near` 50× shifts it by under 2× where the
///   conventional mapping moves ~50×. So the shipped near-plane fix reduced
///   the motivation for reversed-Z by 50× but left ~130 000× on the table:
///   745 BU is still ~10 m of depth quantisation at the ring, which distant
///   LOD objects standing on LOD terrain sit well inside. It is deliberately
///   *not* done: investigating it (#3308) found it touches the projection,
///   the depth clear, both static AND dynamic pipeline compare state (the
///   latter driven live, per draw batch, from authored Gamebryo Z-test
///   functions — every one of its 8 compare-op mappings would need
///   inverting to preserve authored semantics), at least 6 shader files'
///   hardcoded depth-clear-convention checks, and FSR3's vendored C++
///   FidelityFX shim (which needs its own depth-inverted context flag wired
///   through). None of those failure modes are visible to `cargo test`, and
///   this project has no RenderDoc GUI integration to validate a change
///   this pervasive live. Left for a dedicated multi-session effort; #3308
///   tracks it with the measured scope above.
///
///   The **comparison gate** that work needs does now exist, in both halves:
///   [`Camera::analyze_depth_field`] on the CPU side, and the `depth.stats`
///   console command (`byroredux/src/commands/depth.rs`) driving a real
///   depth-attachment readback over `byro-dbg`. Run it before the
///   conversion, run it after, and the far decades' `distinct_codes` are the
///   before/after evidence — the thing that was otherwise unobservable and
///   that made shipping reversed-Z speculative.
///
///   **Measured baseline** (RTX 4070 Ti, `--game fnv --grid 0,0 --radius 3
///   --upscaler taa`, camera on the Mojave satellite-dish rise looking down
///   the valley at the LOD ring, 1280×720, three captures agreeing within
///   2%):
///
///   | decade (BU) | samples | distinct codes | BU/step | reversed |
///   |---|---:|---:|---:|---:|
///   | 100–1 000 | 287 582 | 82 951 | 0.00 | 0.0000 |
///   | 1 000–10 000 | 292 502 | 47 312 | 0.12 | 0.0002 |
///   | 10 000–100 000 | 36 093 | 3 301 | 11.92 | 0.0029 |
///   | 100 000–400 000 | 1 410 | **104** | 476.83 | 0.0073 |
///
///   The last row is the finding: the distant-LOD ring's 1 410 covered
///   pixels share **104** depth values. Near field keeps ~3.5 samples per
///   code, the ring collapses to ~13.6 — an order of magnitude worse
///   discrimination exactly where terrain LOD and the object LOD standing on
///   it have to be told apart.
pub const DEFAULT_RENDER_DISTANCE: f32 = 400_000.0;

/// Near-plane distance for BU-scale content — matches vanilla `fNearDistance`
/// (5 in `Fallout_default.ini`; Oblivion's 10 would work too, but 5 is the
/// more conservative of the two shipped values and this project has no
/// per-game camera profile to pick between them). See
/// [`DEFAULT_RENDER_DISTANCE`]'s depth-precision policy for the measured
/// ~50× depth-resolution improvement this buys over the unit-scale default,
/// and [`Camera::for_content_scale`] for how callers select it.
pub const NEAR_PLANE_BU_SCALE: f32 = 5.0;

/// Perspective camera parameters.
///
/// Attach to an entity that also has a [`Transform`] component.
/// The entity's Transform determines the camera's position and orientation.
#[derive(Debug, Clone, Copy)]
#[cfg_attr(feature = "inspect", derive(serde::Serialize, serde::Deserialize))]
pub struct Camera {
    /// Vertical field of view in radians.
    pub fov_y: f32,
    /// Near clipping plane distance.
    pub near: f32,
    /// Far clipping plane distance.
    pub far: f32,
    /// Viewport aspect ratio (width / height). Updated on window resize.
    pub aspect: f32,
    /// Lens aperture half-radius in world units. `0.0` = pinhole camera (DOF disabled).
    /// The renderer jitters the camera position within a disk of this radius each frame;
    /// TAA accumulates the samples to produce a spatially-varying blur — surfaces at
    /// `focus_dist` are sharp, surfaces at other depths are progressively blurred.
    pub aperture: f32,
    /// Distance to the focal plane in world units. Surfaces at this depth are in sharp focus.
    /// Ignored when `aperture == 0.0`.
    pub focus_dist: f32,
}

impl Camera {
    pub fn new(fov_y: f32, aspect: f32, near: f32, far: f32) -> Self {
        Self {
            fov_y,
            near,
            far,
            aspect,
            aperture: 0.0,
            focus_dist: 20.0,
        }
    }

    /// Otherwise-default camera with a near plane chosen for the content's
    /// physical scale (#3308). `bu_scale_content` should be true for
    /// anything authored in Bethesda world units — a loaded worldspace or
    /// interior cell, a loose NIF mesh/tree view — and false for the
    /// procedural unit-scale demo scene and any calibrated renderer harness
    /// that declares its own camera pose (those own their physical scale
    /// directly; some sit at comparably small scale to the demo scene
    /// despite loading real NIF content, so content-presence alone isn't a
    /// safe signal — see `DEFAULT_RENDER_DISTANCE`'s doc for the exact
    /// signal `scene.rs`'s call site uses).
    pub fn for_content_scale(bu_scale_content: bool) -> Self {
        Self {
            near: if bu_scale_content {
                NEAR_PLANE_BU_SCALE
            } else {
                Self::default().near
            },
            ..Self::default()
        }
    }

    /// Build a perspective projection matrix (Vulkan clip space: Y-down, Z 0..1).
    pub fn projection_matrix(&self) -> Mat4 {
        // glam's perspective_rh already maps Z to [0, 1] (Vulkan/D3D convention).
        // Only the Y-flip is needed for Vulkan's inverted Y axis.
        // Note: the Y-flip reverses apparent triangle winding in clip space —
        // CW triangles (NIF/D3D) appear CCW after this, matching our front face setting.
        let mut proj = Mat4::perspective_rh(self.fov_y, self.aspect, self.near, self.far);
        proj.col_mut(1).y *= -1.0;
        proj
    }

    /// Smallest world-space distance this camera's depth buffer can still
    /// resolve at `distance` in front of the eye, in world units.
    ///
    /// Assumes the pipeline's actual configuration: a `D32_SFLOAT` buffer
    /// with the conventional near→0 / far→1 mapping [`Self::projection_matrix`]
    /// produces. Two surfaces closer together than the returned value land on
    /// the same depth sample and z-fight.
    ///
    /// Derivation — for that mapping, `z_ndc(d) = f/(f-n) · (1 - n/d)`, whose
    /// slope is `f·n / ((f-n)·d²)`. Inverting it against one f32 step (taken
    /// at the actual encoded value, so the exponent is the real one rather
    /// than an assumed worst case) gives the world-space span of a single
    /// depth increment.
    ///
    /// This is the measurement behind the reversed-Z note on
    /// [`DEFAULT_RENDER_DISTANCE`]. Returns `0.0` for degenerate inputs
    /// (`distance` at or inside the near plane, or a non-positive depth
    /// range) — there is no meaningful resolution to report there.
    pub fn depth_resolution_at(&self, distance: f32) -> f32 {
        let (n, f) = (self.near, self.far);
        // Reject NaN/inf first so the ordering comparisons below are total.
        if !(distance.is_finite() && n.is_finite() && f.is_finite()) {
            return 0.0;
        }
        if n <= 0.0 || f <= n || distance <= n {
            return 0.0;
        }
        let ndc = (f / (f - n)) * (1.0 - n / distance);
        // One f32 step at the encoded depth value.
        let ulp = f32::from_bits(ndc.to_bits() + 1) - ndc;
        ulp * (f - n) * distance * distance / (f * n)
    }

    /// The same measurement as [`Self::depth_resolution_at`], but for the
    /// **reversed-Z** mapping this engine does *not* currently use
    /// (near→1, far→0, `GREATER_OR_EQUAL` compare).
    ///
    /// Exists to make the reversed-Z payoff a measured number instead of an
    /// assertion, and to give whoever eventually does that work an analytic
    /// target to validate a GPU depth capture against — the comparison gate
    /// #3308's step 2 asks for, in the half that *is* `cargo test`-visible.
    /// Nothing in the render path reads this; it is a policy-measurement
    /// sibling, exactly like the function above.
    ///
    /// Derivation — reversed-Z encodes `z_ndc(d) = (n/d - n/f) / (1 - n/f)`,
    /// which is `1` at the near plane and `0` at the far plane. Its slope is
    /// `n·f / ((f - n)·d²)` in magnitude — the same factor as the
    /// conventional mapping, since the two differ only by the affine flip
    /// `z ↦ 1 - z`. **All** the difference is in where the f32 exponent
    /// lands: conventional crowds distant samples against 1.0, where f32
    /// steps are coarsest, while reversed puts them near 0.0, where they are
    /// finest. Taking the ulp at the actual encoded value (as the sibling
    /// does) is therefore what makes the comparison honest rather than an
    /// assumed worst case.
    ///
    /// Same degenerate-input contract as [`Self::depth_resolution_at`].
    pub fn depth_resolution_at_reversed(&self, distance: f32) -> f32 {
        let (n, f) = (self.near, self.far);
        if !(distance.is_finite() && n.is_finite() && f.is_finite()) {
            return 0.0;
        }
        if n <= 0.0 || f <= n || distance <= n {
            return 0.0;
        }
        let ndc = (n / distance - n / f) / (1.0 - n / f);
        // One f32 step at the encoded depth value. Guard the exact-zero case
        // (`distance == f`), where `to_bits() + 1` would step off the
        // subnormal floor and report a meaningless resolution.
        let ulp = if ndc > 0.0 {
            f32::from_bits(ndc.to_bits() + 1) - ndc
        } else {
            return 0.0;
        };
        ulp * (f - n) * distance * distance / (f * n)
    }

    /// Recover the world-space eye distance a conventional-mapping depth
    /// sample encodes. Inverse of the `z_ndc(d)` in
    /// [`Self::depth_resolution_at`]'s derivation.
    ///
    /// A cleared sample (`z == 1.0`, nothing drawn) decodes to exactly
    /// `far`. Returns `0.0` for a sample outside `[0, 1]` or for a
    /// degenerate camera — same contract as the resolution functions.
    pub fn linear_distance_from_depth(&self, z: f32) -> f32 {
        let (n, f) = (self.near, self.far);
        if !(z.is_finite() && n.is_finite() && f.is_finite()) {
            return 0.0;
        }
        if n <= 0.0 || f <= n || !(0.0..=1.0).contains(&z) {
            return 0.0;
        }
        let denom = 1.0 - z * (f - n) / f;
        if denom <= 0.0 {
            return f;
        }
        (n / denom).min(f)
    }

    /// Bucket a captured depth field into distance decades and report, per
    /// decade, how many *distinct encoded values* it actually contains
    /// alongside what [`Self::depth_resolution_at`] predicts.
    ///
    /// This is the CPU half of #3308's step-2 comparison gate. The analytic
    /// functions say what the depth buffer's resolution *should* be; this
    /// says what a real captured frame's depth buffer *does* contain. Two
    /// things fall out of running it:
    ///
    /// * **Validation** — `distinct_codes` in a decade can never exceed the
    ///   sample count, and the decade's span divided by `distinct_codes`
    ///   should land in the same order of magnitude as
    ///   `analytic_resolution`. A capture that disagrees means the readback
    ///   is wrong (stale, wrong aspect, wrong format), not that the analysis
    ///   is.
    /// * **Comparison** — re-run after a reversed-Z conversion and the far
    ///   decades should gain orders of magnitude of `distinct_codes` while
    ///   the near decades barely move. That difference is the thing #3308
    ///   exists to buy, and it is not otherwise observable.
    ///
    /// Bands are decades of eye distance from `near` up to `far`, so this
    /// works unchanged for both the unit-scale demo camera and the BU-scale
    /// worldspace one. Samples that decode to `far` are counted as
    /// [`DepthFieldStats::cleared`] and excluded from the bands — they are
    /// background, not geometry, and would otherwise swamp the last decade.
    pub fn analyze_depth_field(&self, encoded: &[f32]) -> DepthFieldStats {
        use std::collections::HashSet;

        let mut stats = DepthFieldStats {
            total: encoded.len() as u32,
            ..Default::default()
        };
        if self.near <= 0.0 || self.far <= self.near {
            return stats;
        }

        // Decade edges from `near` to `far`, e.g. 5 → 10 → 100 → … → 400000.
        let mut edges: Vec<f32> = vec![self.near];
        let mut e = 10f32.powf(self.near.log10().floor() + 1.0);
        while e < self.far {
            edges.push(e);
            e *= 10.0;
        }
        edges.push(self.far);

        let mut codes: Vec<HashSet<u32>> = vec![HashSet::new(); edges.len() - 1];
        let mut counts = vec![0u32; edges.len() - 1];
        let (mut nearest, mut farthest) = (f32::INFINITY, 0.0f32);

        for &z in encoded {
            // Classify background on the ENCODED side. The depth clear value
            // is exactly 1.0 and any drawn fragment passed a LESS test
            // against it, so `z >= 1.0` is precisely "nothing drawn here" —
            // no decode needed, and therefore no decode error. Round-tripping
            // instead is not safe: at the far plane one depth step spans tens
            // of thousands of world units, so f32 error in the decode can put
            // a cleared sample just *under* `far` and drop the frame's entire
            // background into the last decade, swamping the one band the
            // gate most needs to read.
            if z >= 1.0 {
                stats.cleared += 1;
                continue;
            }
            let d = self.linear_distance_from_depth(z);
            if d <= 0.0 {
                stats.invalid += 1;
                continue;
            }
            // Belt-and-braces: a `z` just under 1.0 can still decode to at or
            // past the far plane. Same bucket — it is background either way.
            if d >= self.far {
                stats.cleared += 1;
                continue;
            }
            nearest = nearest.min(d);
            farthest = farthest.max(d);
            // Last edge is `far`, so `d < far` always lands in a band.
            let band = edges
                .windows(2)
                .position(|w| d >= w[0] && d < w[1])
                .unwrap_or(0);
            counts[band] += 1;
            codes[band].insert(z.to_bits());
        }

        stats.nearest = if nearest.is_finite() { nearest } else { 0.0 };
        stats.farthest = farthest;
        stats.bands = edges
            .windows(2)
            .enumerate()
            .map(|(i, w)| {
                // Geometric midpoint — the representative distance for a
                // decade, since the band is log-spaced.
                let mid = (w[0] * w[1]).sqrt();
                DepthBand {
                    near_edge: w[0],
                    far_edge: w[1],
                    samples: counts[i],
                    distinct_codes: codes[i].len() as u32,
                    analytic_resolution: self.depth_resolution_at(mid),
                    analytic_resolution_reversed: self.depth_resolution_at_reversed(mid),
                }
            })
            .collect();
        stats
    }

    /// Build a view matrix from the camera entity's transform.
    ///
    /// The transform's translation is the camera position.
    /// The transform's rotation determines the look direction
    /// (forward is -Z in the camera's local space).
    pub fn view_matrix(transform: &Transform) -> Mat4 {
        let position = transform.translation;
        let forward = transform.rotation * -Vec3::Z;
        let up = transform.rotation * Vec3::Y;
        Mat4::look_at_rh(position, position + forward, up)
    }
}

impl Default for Camera {
    fn default() -> Self {
        Self {
            fov_y: std::f32::consts::FRAC_PI_4, // 45°
            // 0.1 is far nearer than any shipped Gamebryo game clips at,
            // and it is what makes the depth range so lopsided — but one
            // camera contract serves both BU-scale worldspaces and the
            // unit-scale demo/loose-NIF scenes, and the latter sit ~4 units
            // from the origin. See `DEFAULT_RENDER_DISTANCE`'s depth-precision
            // policy for the full reasoning and the measured numbers.
            near: 0.1,
            // Sized to clear the widest distant-LOD ring's far-corner
            // diagonal; `cell_loader::terrain_lod` const-asserts the
            // relation. Rationale + depth-precision policy live on the
            // constant.
            far: DEFAULT_RENDER_DISTANCE,
            aspect: 16.0 / 9.0,
            aperture: 0.0,
            focus_dist: 20.0,
        }
    }
}

/// One decade of eye distance within a captured depth field.
/// See [`Camera::analyze_depth_field`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DepthBand {
    pub near_edge: f32,
    pub far_edge: f32,
    /// Depth samples that decoded into this decade.
    pub samples: u32,
    /// How many *distinct* encoded depth values those samples used — the
    /// empirical depth discrimination available in this decade. When this is
    /// far below `samples`, surfaces in the band are collapsing onto shared
    /// depth values, which is z-fighting waiting to happen.
    pub distinct_codes: u32,
    /// What [`Camera::depth_resolution_at`] predicts at the decade's
    /// geometric midpoint, in world units per depth step.
    pub analytic_resolution: f32,
    /// The same prediction under reversed-Z
    /// ([`Camera::depth_resolution_at_reversed`]) — the payoff a conversion
    /// would buy in this decade.
    pub analytic_resolution_reversed: f32,
}

/// Summary of one captured depth buffer. See [`Camera::analyze_depth_field`].
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DepthFieldStats {
    /// Every sample handed in.
    pub total: u32,
    /// Samples at or past the far plane — background, nothing drawn.
    pub cleared: u32,
    /// Samples that decoded to nothing usable (outside `[0, 1]`, or a
    /// degenerate camera). Non-zero here means the capture is suspect.
    pub invalid: u32,
    /// Closest / furthest geometry distance seen, world units.
    pub nearest: f32,
    pub farthest: f32,
    pub bands: Vec<DepthBand>,
}

impl Component for Camera {
    type Storage = SparseSetStorage<Self>;
}

/// Resource indicating which entity is the active camera.
pub struct ActiveCamera(pub EntityId);
impl Resource for ActiveCamera {}

#[cfg(test)]
mod tests {
    use super::*;
    /// Encode a world distance the way the conventional projection does —
    /// the inverse of `linear_distance_from_depth`, used to build synthetic
    /// depth fields with known content.
    fn encode(cam: &Camera, d: f32) -> f32 {
        let (n, f) = (cam.near, cam.far);
        (f / (f - n)) * (1.0 - n / d)
    }

    /// #3308 step 2 — the decode must round-trip the projection, or every
    /// number the capture gate reports is measuring the decoder rather than
    /// the depth buffer.
    #[test]
    fn depth_decode_round_trips_the_projection() {
        for cam in [Camera::default(), Camera::for_content_scale(true)] {
            for d in [
                cam.near * 2.0,
                100.0,
                1_000.0,
                50_000.0,
                196_608.0,
                250_000.0,
            ] {
                if d <= cam.near || d >= cam.far {
                    continue;
                }
                let back = cam.linear_distance_from_depth(encode(&cam, d));
                // The right tolerance is the depth buffer's OWN resolution at
                // that distance, not a flat percentage: a single f32 depth
                // code spans `depth_resolution_at(d)` world units, so no
                // decoder can recover `d` better than that — at `near = 0.1`
                // and `d = 196 608` one step is ~23 000 BU, a 5.9% band. Tying
                // the tolerance to the analytic function cross-validates the
                // decoder against the measurement the whole gate rests on,
                // instead of hiding the coarseness behind a hand-picked
                // epsilon.
                let budget = cam.depth_resolution_at(d);
                assert!(
                    (back - d).abs() <= budget,
                    "near={} d={d} decoded {back} — off by {} with a one-step \
                     budget of {budget}",
                    cam.near,
                    (back - d).abs()
                );
            }
        }
        // A cleared sample is background, at exactly the far plane.
        let cam = Camera::default();
        assert_eq!(cam.linear_distance_from_depth(1.0), cam.far);
        // Out-of-range / degenerate inputs report nothing rather than guess.
        assert_eq!(cam.linear_distance_from_depth(-0.1), 0.0);
        assert_eq!(cam.linear_distance_from_depth(1.5), 0.0);
        assert_eq!(cam.linear_distance_from_depth(f32::NAN), 0.0);
    }

    /// A synthetic field with known content must be bucketed into the right
    /// decades, with cleared background separated from geometry.
    #[test]
    fn depth_field_analysis_buckets_known_distances() {
        let cam = Camera::for_content_scale(true);
        let mut field = vec![1.0f32; 100]; // 100 cleared background samples
        for d in [50.0f32, 500.0, 5_000.0, 50_000.0] {
            field.push(encode(&cam, d));
        }
        field.push(f32::NAN); // one corrupt sample

        let stats = cam.analyze_depth_field(&field);
        assert_eq!(stats.total, 105);
        assert_eq!(stats.cleared, 100, "background must not enter the bands");
        assert_eq!(stats.invalid, 1, "a NaN sample must be counted, not hidden");
        assert_eq!(stats.bands.iter().map(|b| b.samples).sum::<u32>(), 4);
        assert!(stats.nearest > 40.0 && stats.nearest < 60.0);
        assert!(stats.farthest > 45_000.0 && stats.farthest < 55_000.0);
        // Each of the four landed in a different decade.
        assert_eq!(stats.bands.iter().filter(|b| b.samples > 0).count(), 4);
        for band in stats.bands.iter().filter(|b| b.samples > 0) {
            assert_eq!(band.distinct_codes, 1, "one sample, one code");
        }
    }

    /// The property the gate actually reads: in a far decade the
    /// conventional mapping collapses many distinct distances onto a handful
    /// of depth codes, while reversed-Z would keep them apart. Built from
    /// distances spaced *finer* than the conventional resolution at that
    /// range, so the collapse is the measurement rather than an artifact of
    /// the sampling.
    #[test]
    fn distinct_codes_expose_far_field_depth_collapse() {
        let cam = Camera::for_content_scale(true);
        // 200 surfaces spread over 2000 BU around the LOD ring — 10 BU
        // apart, well under the ~745 BU/step the conventional mapping
        // resolves there, and well over the ~0.006 reversed-Z would.
        let base = 249_000.0f32;
        let field: Vec<f32> = (0..200)
            .map(|i| encode(&cam, base + i as f32 * 10.0))
            .collect();
        let stats = cam.analyze_depth_field(&field);

        let far = stats
            .bands
            .iter()
            .find(|b| b.samples > 0)
            .expect("the ring distances must land in a band");
        assert_eq!(far.samples, 200);
        assert!(
            far.distinct_codes < 20,
            "200 surfaces 10 BU apart at the LOD ring must collapse onto a \
             handful of depth codes under the conventional mapping — got {} \
             distinct codes",
            far.distinct_codes
        );
        // And the analytic pair explains why, in the same row the operator
        // reads: conventional coarse, reversed fine.
        assert!(far.analytic_resolution > 100.0);
        assert!(far.analytic_resolution_reversed < 1.0);
    }

    /// A degenerate camera must report nothing rather than divide by zero or
    /// emit bands it cannot justify.
    #[test]
    fn depth_field_analysis_rejects_a_degenerate_camera() {
        let inverted = Camera::new(FRAC_PI_4, 1.0, 10.0, 1.0);
        let stats = inverted.analyze_depth_field(&[0.5, 0.5]);
        assert_eq!(stats.total, 2);
        assert!(stats.bands.is_empty());
    }

    /// #3308 — the reversed-Z payoff, measured rather than asserted. These
    /// are the numbers the issue's "is it still worth the blast radius?"
    /// question turns on, and the analytic target a future GPU depth capture
    /// validates against.
    ///
    /// At the 250 000 BU LOD ring:
    ///
    /// | mapping | `near = 0.1` | `near = 5.0` |
    /// |---|---:|---:|
    /// | conventional | 37 252.89 | 745.05 |
    /// | reversed | 0.0089 | 0.0057 |
    ///
    /// The near-plane fix already shipped (`for_content_scale`) bought 50×;
    /// reversed-Z is worth a further ~130 000× on top of it, so raising
    /// `near` reduced the motivation for reversed-Z but nowhere near
    /// removed it — 745 BU is still ~10 m of depth quantisation, which
    /// distant LOD objects standing on LOD terrain are well inside.
    #[test]
    fn reversed_z_resolution_is_orders_better_at_the_lod_ring() {
        const RING: f32 = 250_000.0;
        for cam in [Camera::default(), Camera::for_content_scale(true)] {
            let conventional = cam.depth_resolution_at(RING);
            let reversed = cam.depth_resolution_at_reversed(RING);
            assert!(
                reversed > 0.0 && reversed < 1.0,
                "reversed-Z must resolve to sub-world-unit at the ring, got {reversed}"
            );
            assert!(
                conventional / reversed > 10_000.0,
                "reversed-Z must be orders of magnitude finer at the ring \
                 (conventional {conventional}, reversed {reversed})"
            );
        }
    }

    /// The property that makes reversed-Z *the* fix rather than another
    /// tuning knob: its resolution is near-uniform with distance and barely
    /// depends on the near plane, whereas the conventional mapping degrades
    /// quadratically and is dominated by `near`.
    #[test]
    fn reversed_z_is_near_plane_insensitive_unlike_the_conventional_mapping() {
        const RING: f32 = 250_000.0;
        let unit = Camera::default();
        let bu = Camera::for_content_scale(true);

        // Conventional: raising `near` 50× changes the answer by ~50×.
        let conventional_ratio = unit.depth_resolution_at(RING) / bu.depth_resolution_at(RING);
        assert!(
            conventional_ratio > 40.0,
            "the conventional mapping must be strongly near-plane dependent, got {conventional_ratio}"
        );

        // Reversed: the same 50× near-plane change moves it by under 2×.
        let a = unit.depth_resolution_at_reversed(RING);
        let b = bu.depth_resolution_at_reversed(RING);
        let reversed_ratio = if a > b { a / b } else { b / a };
        assert!(
            reversed_ratio < 2.0,
            "reversed-Z must be near-plane insensitive, got {reversed_ratio} ({a} vs {b})"
        );
    }

    /// Same degenerate-input contract as the conventional sibling, plus the
    /// `distance == far` case, where the encoded value is exactly 0.0 and
    /// stepping its bits would report a meaningless subnormal resolution.
    #[test]
    fn reversed_z_rejects_degenerate_inputs() {
        let cam = Camera::default();
        assert_eq!(cam.depth_resolution_at_reversed(cam.near), 0.0);
        assert_eq!(cam.depth_resolution_at_reversed(0.0), 0.0);
        assert_eq!(cam.depth_resolution_at_reversed(f32::NAN), 0.0);
        assert_eq!(cam.depth_resolution_at_reversed(f32::INFINITY), 0.0);
        // At and beyond the far plane the encoded depth is <= 0.
        assert_eq!(cam.depth_resolution_at_reversed(cam.far), 0.0);

        let inverted = Camera::new(FRAC_PI_4, 1.0, 10.0, 1.0);
        assert_eq!(inverted.depth_resolution_at_reversed(5.0), 0.0);
        let zero_near = Camera::new(FRAC_PI_4, 1.0, 0.0, 100.0);
        assert_eq!(zero_near.depth_resolution_at_reversed(50.0), 0.0);
    }

    use crate::math::{Quat, Vec3, Vec4};
    use std::f32::consts::FRAC_PI_4;

    #[test]
    fn default_camera() {
        let cam = Camera::default();
        assert!((cam.fov_y - FRAC_PI_4).abs() < 1e-6);
        assert!((cam.near - 0.1).abs() < 1e-6);
        assert!((cam.far - DEFAULT_RENDER_DISTANCE).abs() < 1e-6);
    }

    /// #2371 / EX-11 — the far plane must clear the far-corner diagonal of
    /// the widest distant-LOD ring (Skyrim/FO4's 61-cell baked ladder), or
    /// the frustum's far plane culls whole LOD blocks at the diagonals.
    /// `cell_loader::terrain_lod` const-asserts the same relation against its
    /// own reach constant; this pins the core half so the number cannot drift
    /// without a failure on both sides.
    #[test]
    fn far_plane_clears_the_widest_lod_ring_corner() {
        const CELL_UNITS: f32 = 4096.0;
        for (label, cells) in [("synthesized ring", 48.0), ("baked ladder", 61.0)] {
            let corner = cells * CELL_UNITS * 2f32.sqrt();
            assert!(
                DEFAULT_RENDER_DISTANCE > corner,
                "{label}: far plane {DEFAULT_RENDER_DISTANCE} does not reach its \
                 {corner} BU far-corner diagonal"
            );
        }
        // The 61-cell corner is the binding constraint; anything below it is
        // a regression, and the margin above covers terrain relief.
        let binding = 61.0 * CELL_UNITS * 2f32.sqrt();
        assert!((353_300.0..353_400.0).contains(&binding));
        assert!(DEFAULT_RENDER_DISTANCE / binding > 1.1);
    }

    /// Pins the depth-precision budget the reversed-Z note argues from, so
    /// the claim cannot quietly go stale. At the LOD ring the buffer resolves
    /// nothing finer than tens of kilometres — distant terrain and the object
    /// LOD standing on it share a depth sample.
    #[test]
    fn depth_resolution_collapses_at_the_lod_ring() {
        let cam = Camera::default();

        // Close up the buffer is effectively exact.
        assert!(cam.depth_resolution_at(10.0) < 0.01);

        // At the ring a single depth step spans tens of thousands of world
        // units (measured: ~37 250 BU at 250 000 BU out).
        let at_ring = cam.depth_resolution_at(250_000.0);
        assert!(
            (30_000.0..50_000.0).contains(&at_ring),
            "expected a collapsed depth budget at the LOD ring, got {at_ring}"
        );

        // Resolution degrades with the square of distance, so the vanilla
        // near-plane values would buy roughly the ratio they change: a 5.0
        // near plane (Fallout_default.ini's fNearDistance, what
        // `for_content_scale(true)` selects, #3308) is ~50× better.
        let raised = Camera::for_content_scale(true);
        let ratio = at_ring / raised.depth_resolution_at(250_000.0);
        assert!(
            (25.0..100.0).contains(&ratio),
            "a 0.1 → 5.0 near plane should buy roughly 50×, got {ratio}"
        );
    }

    /// #3308 — `for_content_scale` is the one place a caller picks between
    /// the unit-scale and BU-scale near planes; pin both arms and the
    /// otherwise-unaffected fields.
    #[test]
    fn for_content_scale_selects_the_right_near_plane() {
        let bu = Camera::for_content_scale(true);
        assert!((bu.near - NEAR_PLANE_BU_SCALE).abs() < 1e-6);
        assert!((bu.far - DEFAULT_RENDER_DISTANCE).abs() < 1e-6);
        assert!((bu.fov_y - FRAC_PI_4).abs() < 1e-6);

        let unit = Camera::for_content_scale(false);
        assert!((unit.near - Camera::default().near).abs() < 1e-6);
        assert!(
            unit.near < NEAR_PLANE_BU_SCALE,
            "unit-scale content must keep the smaller near plane, or a \
             4-unit-away demo camera clips"
        );
    }

    /// Degenerate inputs report `0.0` rather than infinities or NaN — the
    /// helper is called with whatever the active camera holds.
    #[test]
    fn depth_resolution_rejects_degenerate_inputs() {
        let cam = Camera::default();
        assert_eq!(cam.depth_resolution_at(cam.near), 0.0);
        assert_eq!(cam.depth_resolution_at(0.0), 0.0);
        assert_eq!(cam.depth_resolution_at(f32::NAN), 0.0);
        let inverted = Camera::new(FRAC_PI_4, 1.0, 10.0, 1.0);
        assert_eq!(inverted.depth_resolution_at(5.0), 0.0);
        let zero_near = Camera::new(FRAC_PI_4, 1.0, 0.0, 100.0);
        assert_eq!(zero_near.depth_resolution_at(50.0), 0.0);
    }

    #[test]
    fn projection_matrix_is_valid() {
        let cam = Camera::new(FRAC_PI_4, 16.0 / 9.0, 0.1, 100.0);
        let proj = cam.projection_matrix();

        // Near plane should map to Z=0 in Vulkan clip space.
        // Far plane should map to Z=1.
        // Check that the matrix is not all zeros.
        assert!(proj.col(0).x.abs() > 0.0);
        assert!(proj.col(1).y.abs() > 0.0);

        // Y should be flipped for Vulkan (negative).
        assert!(proj.col(1).y < 0.0);
    }

    #[test]
    fn view_matrix_at_origin_looking_forward() {
        let transform = Transform::IDENTITY;
        let view = Camera::view_matrix(&transform);

        // Camera at origin looking down -Z.
        // Point at (0, 0, -5) should be in front of the camera.
        let point = view * Vec4::new(0.0, 0.0, -5.0, 1.0);
        // In view space, the point should have negative Z (in front).
        assert!(point.z < 0.0);
    }

    #[test]
    fn view_matrix_translated() {
        let transform = Transform::from_translation(Vec3::new(0.0, 0.0, 5.0));
        let view = Camera::view_matrix(&transform);

        // Camera at (0, 0, 5) looking down -Z.
        // Origin (0, 0, 0) should be 5 units in front.
        let point = view * Vec4::new(0.0, 0.0, 0.0, 1.0);
        assert!((point.z + 5.0).abs() < 1e-4);
    }

    #[test]
    fn view_matrix_rotated() {
        // Camera rotated 90° around Y — now looking down -X.
        let rotation = Quat::from_rotation_y(std::f32::consts::FRAC_PI_2);
        let transform = Transform::from_rotation(rotation);
        let view = Camera::view_matrix(&transform);

        // Point at (-5, 0, 0) should be in front of the camera.
        let point = view * Vec4::new(-5.0, 0.0, 0.0, 1.0);
        assert!(point.z < 0.0);
    }
}
