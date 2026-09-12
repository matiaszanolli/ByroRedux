//! Particle-emitter extraction (#3856, split from `walk/mod.rs`).
//!
//! `walk_node_particle_emitters_flat` is an independent entry point — it is
//! called from `import/mod.rs`, never from the two scene-graph walkers — so
//! this cluster carries no shared traversal state and moved verbatim. It was
//! ~34 % of the pre-split file and is where the per-game particle work keeps
//! landing.

use crate::scene::NifScene;
use crate::types::{BlockRef, NiTransform};

use super::super::coord::zup_point_to_yup;
use super::super::transform::compose_transforms;
use super::{as_ni_node, switch_active_children};
use byroredux_core::string::StringPool;

/// Extract the sprite texture, blend factors and `BSEffectShaderProperty`
/// payload attached to a particle system. Particle systems carry the same
/// shader/property references as geometry, so route them through the shared
/// material walker instead of silently replacing the authored sprite with
/// bindless slot zero.
///
/// #2610 — the walker already builds the full [`MaterialInfo`] here, so the
/// authored BGEM effect payload (`effect_soft` / `effect_lit` / the two
/// greyscale-palette bits / `lighting_influence`) comes for free. It used to
/// be dropped on the floor, which is why every particle `DrawCommand`
/// hardcoded `effect_shader_flags: 0`.
///
/// [`MaterialInfo`]: super::super::material::MaterialInfo
pub(crate) fn extract_particle_material(
    scene: &NifScene,
    ps: &crate::blocks::particle::NiParticleSystem,
    inherited_props: &[BlockRef],
    pool: &mut StringPool,
) -> ParticleMaterial {
    let info = super::super::material::extract_material_info_from_refs(
        scene,
        ps.shader_property_ref,
        ps.alpha_property_ref,
        &ps.properties,
        inherited_props,
        pool,
    );
    let texture_path = info
        .texture_path
        .and_then(|path| pool.resolve(path).map(str::to_owned));
    let authored_blend = info.alpha_blend || info.alpha_blend_authored;
    // #3590 — the greyscale→palette LUT texture the two `effect_shader`
    // palette bits (`effect_palette_color`/`effect_palette_alpha`) index.
    // Mirrors the mesh path's exact resolution order
    // (`crates/nif/src/import/material/mod.rs`'s `greyscale_lut_map.or_else`
    // fallback to `effect_shader.greyscale_texture`): prefer the dedicated
    // `BSShaderTextureSet` slot 3 (FO4/FO76), fall back to the BGEM
    // `greyscale_texture` field (the older/common authoring path) when the
    // NIF didn't bind slot 3. Without this, `pack_effect_shader_flags`
    // still sets the palette bits from `effect_shader` below, but nothing
    // ever carries the palette itself past this boundary — see the issue.
    let greyscale_lut_map = info
        .greyscale_lut_map
        .and_then(|path| pool.resolve(path).map(str::to_owned))
        .or_else(|| {
            info.effect_shader
                .as_ref()
                .and_then(|data| data.greyscale_texture.clone())
        });
    ParticleMaterial {
        texture_path,
        src_blend: authored_blend.then_some(info.src_blend_mode),
        dst_blend: authored_blend.then_some(info.dst_blend_mode),
        effect_shader: info.effect_shader,
        greyscale_lut_map,
    }
}

/// Return of [`extract_particle_material`] — the authored material state a
/// particle system contributes to its spawned emitter.
pub(crate) struct ParticleMaterial {
    // #3856 — `walk_node_flat` (still in `mod.rs`) reads these directly, so
    // the fields cross a module boundary they did not before the split.
    pub(super) texture_path: Option<String>,
    pub(super) src_blend: Option<u8>,
    pub(super) dst_blend: Option<u8>,
    pub(super) effect_shader: Option<crate::import::BsEffectShaderData>,
    /// #3590 — the greyscale→palette LUT texture path, when authored. See
    /// [`extract_particle_material`]'s doc comment for the resolution
    /// order.
    pub(super) greyscale_lut_map: Option<String>,
}

/// Walk a `NiParticleSystem.modifier_refs` chain and collect every
/// `NiPSys{Gravity,Vortex,Drag,Turbulence,Air,Radial}FieldModifier`
/// into an `ImportedParticleForceField` list. Inactive modifiers
/// (per [`NiPSysModifierBase::active`]) and stale refs are skipped.
/// See #984 / NIF-D5-ORPHAN-A2.
pub(crate) fn collect_force_fields(
    scene: &NifScene,
    modifier_refs: &[crate::types::BlockRef],
) -> Vec<crate::import::ImportedParticleForceField> {
    use crate::blocks::particle::{
        NiPSysAirFieldModifier, NiPSysDragFieldModifier, NiPSysGravityFieldModifier,
        NiPSysRadialFieldModifier, NiPSysTurbulenceFieldModifier, NiPSysVortexFieldModifier,
    };
    use crate::import::ImportedParticleForceField as F;

    let mut out = Vec::new();
    for r in modifier_refs {
        let Some(idx) = r.index() else { continue };
        let Some(block) = scene.blocks.get(idx) else {
            continue;
        };
        let any = block.as_any();
        if let Some(g) = any.downcast_ref::<NiPSysGravityFieldModifier>() {
            if !g.modifier_base.active {
                continue;
            }
            out.push(F::Gravity {
                direction: g.direction,
                strength: g.field_base.magnitude,
                decay: g.field_base.attenuation,
            });
        } else if let Some(v) = any.downcast_ref::<NiPSysVortexFieldModifier>() {
            if !v.modifier_base.active {
                continue;
            }
            out.push(F::Vortex {
                axis: v.direction,
                strength: v.field_base.magnitude,
                decay: v.field_base.attenuation,
            });
        } else if let Some(d) = any.downcast_ref::<NiPSysDragFieldModifier>() {
            if !d.modifier_base.active {
                continue;
            }
            out.push(F::Drag {
                strength: d.field_base.magnitude,
                direction: d.direction,
                use_direction: d.use_direction,
            });
        } else if let Some(t) = any.downcast_ref::<NiPSysTurbulenceFieldModifier>() {
            if !t.modifier_base.active {
                continue;
            }
            out.push(F::Turbulence {
                frequency: t.frequency,
                scale: t.field_base.magnitude,
            });
        } else if let Some(a) = any.downcast_ref::<NiPSysAirFieldModifier>() {
            if !a.modifier_base.active {
                continue;
            }
            out.push(F::Air {
                direction: a.direction,
                strength: a.field_base.magnitude,
                falloff: a.field_base.attenuation,
            });
        } else if let Some(rd) = any.downcast_ref::<NiPSysRadialFieldModifier>() {
            if !rd.modifier_base.active {
                continue;
            }
            out.push(F::Radial {
                strength: rd.field_base.magnitude,
                falloff: rd.field_base.attenuation,
            });
        }
    }
    out
}

/// Resolve `modifier_refs` (a `NiParticleSystem`'s own modifier list) to
/// the first block downcasting to `T`, in list order. Shared helper for
/// the per-instance extractors below — mirrors the shape of
/// `scene.blocks.iter().find_map(downcast)` but scoped to one system's
/// own modifiers instead of the whole scene.
fn find_own_modifier<'a, T: 'static>(
    scene: &'a NifScene,
    modifier_refs: &[BlockRef],
) -> Option<&'a T> {
    modifier_refs.iter().find_map(|r| {
        let idx = r.index()?;
        scene.get_as::<T>(idx)
    })
}

/// Scan `modifier_refs` — a `NiParticleSystem`'s own modifier list — for
/// its authored particle colour ramp. Two sources, in priority order:
///   1. `NiPSysColorModifier` → `NiColorData` keyframe stream (Oblivion /
///      Skyrim-era + the modern reference-based modifier). Returns the
///      t=0 and t=last RGBA keys.
///   2. `BSPSysSimpleColorModifier` (#1345) — the dominant FO3/FNV-era
///      modifier, which embeds its 3-key ramp INLINE rather than
///      referencing a `NiColorData` block. Returns `Colors[0]` (birth) and
///      `Colors[2]` (death).
///
/// `None` when neither is present (→ fall back to the heuristic preset).
///
/// #4261 (OB-D4-02) — scoped to `modifier_refs` rather than the whole
/// scene: a `NiPSysColorModifier`/`BSPSysSimpleColorModifier` is one of
/// this system's own `NiPSysModifier`-family entries, the same list
/// [`collect_force_fields`] already walks correctly for force fields.
/// The prior whole-scene scan gave every emitter in a multi-emitter NIF
/// the FIRST system's colour ramp — measured on 140 of 208 Oblivion+DLC
/// NIFs with more than one `NiPSysEmitter` (67.3%). See #707 / FX-2 +
/// #1345 + the #1402 comment this fix finally closes out.
pub(crate) fn extract_first_color_curve(
    scene: &NifScene,
    modifier_refs: &[BlockRef],
) -> Option<crate::import::ParticleColorCurve> {
    use crate::blocks::interpolator::NiColorData;
    use crate::blocks::particle::{BSPSysSimpleColorModifier, NiPSysColorModifier};

    // Reject NaN, ±Inf, and the nif.xml FLT_MAX sentinel (≥ 3.0e38) in any
    // component.  Analogous to `sane()` in `extract_emitter_rate` — authored
    // RGBA values are always in a normal float range; a sentinel or corrupt
    // parse would otherwise reach the shader as a wildly out-of-range colour.
    fn is_valid_color(c: [f32; 4]) -> bool {
        c.iter().all(|&x| x.is_finite() && x < 3.0e38)
    }

    // 1. Legacy reference-based modifier → NiColorData keyframe stream.
    if let Some(modifier) = find_own_modifier::<NiPSysColorModifier>(scene, modifier_refs) {
        if let Some(data_idx) = modifier.color_data_ref.index() {
            if let Some(data) = scene.get_as::<NiColorData>(data_idx) {
                let keys = &data.keys.keys;
                if !keys.is_empty() {
                    let start = keys[0].value;
                    let end = keys.last().expect("non-empty checked above").value;
                    if is_valid_color(start) && is_valid_color(end) {
                        return Some(crate::import::ParticleColorCurve { start, end });
                    }
                }
            }
        }
    }

    // 2. #1345 — fall back to the inline BSPSysSimpleColorModifier ramp.
    if let Some(scm) = find_own_modifier::<BSPSysSimpleColorModifier>(scene, modifier_refs) {
        let start = scm.colors[0];
        let end = scm.colors[2];
        if is_valid_color(start) && is_valid_color(end) {
            return Some(crate::import::ParticleColorCurve { start, end });
        }
    }

    None
}

/// Scan `modifier_refs` — a `NiParticleSystem`'s own modifier list — for
/// its `NiPSysEmitter` and return its decoded base spawn parameters.
/// `None` when this system has no emitter block among its own modifiers
/// (→ fall back to the heuristic preset). See `docs/engine/nifal.md` —
/// particles slice.
///
/// #4261 (OB-D4-02) — scoped to `modifier_refs` rather than the whole
/// scene: `NiPSysEmitter` (Box/Sphere/Cylinder/Mesh variants all parse to
/// this one struct) is itself a `NiPSysModifier` subtype, so it's one of
/// this system's own list entries — the same list [`collect_force_fields`]
/// already walks correctly. The prior whole-scene scan gave every emitter
/// in a multi-emitter NIF the FIRST system's kinematics (speed,
/// declination, life span, radius); measured on 140 of 208 Oblivion+DLC
/// NIFs with more than one `NiPSysEmitter` (67.3%), e.g. `transformation.nif`
/// (13 emitters), `obgatemini01.nif` (11).
pub(crate) fn extract_emitter_params(
    scene: &NifScene,
    modifier_refs: &[BlockRef],
) -> Option<crate::import::ImportedEmitterParams> {
    use crate::blocks::particle::NiPSysEmitter;

    let emitter = find_own_modifier::<NiPSysEmitter>(scene, modifier_refs)?;
    let p = &emitter.params;
    // Pair the emitter base with its OWN grow/fade modifier's base_scale
    // (size multiplier), if any — same list, same reasoning as the
    // emitter lookup above. NIFAL-S5 (#1434) — reject a non-finite or
    // non-positive raw scale here: it feeds `initial_radius × base_scale`
    // (systems/particle.rs), so 0.0/negative spawns zero-or-inverted-size
    // particles and NaN/Inf poisons the product. Dropping just the
    // modifier to `None` falls back to the ×1.0 default rather than
    // rejecting the whole (otherwise valid) emitter — sibling of the
    // NIFAL-S3 finite filter below.
    let base_scale =
        find_own_modifier::<crate::blocks::particle::NiPSysGrowFadeModifier>(scene, modifier_refs)
            .and_then(|m| m.base_scale)
            .filter(|s| s.is_finite() && *s > 0.0);
    // NIFAL-S3 (#1411) — reject corrupt emitter scalars before they reach
    // `apply_emitter_params`, which copies every one straight into the
    // particle preset. A single non-finite value (NaN/Inf from a malformed
    // NIF) poisons every spawned particle's per-frame integration, and a
    // non-positive `life_span` spawns already-dead particles. Fall back to
    // the heuristic preset (`None`) rather than leak garbage. Sibling of
    // `extract_emitter_rate`'s `sane()` finite filter; the positivity
    // checks match the issue's `life_span > 0` / `initial_radius >= 0`.
    let all_finite = p.speed.is_finite()
        && p.speed_variation.is_finite()
        && p.declination.is_finite()
        && p.declination_variation.is_finite()
        // #1445 — planar_angle / planar_angle_variation were lifted into
        // EmitterBaseParams but omitted from this sweep. Harmless today
        // (apply_emitter_params doesn't read them yet) but a latent NaN trap
        // the moment planar angle is wired into the spawn cone; include them
        // now so the guard can't be silently outrun by a future consumer.
        && p.planar_angle.is_finite()
        && p.planar_angle_variation.is_finite()
        && p.initial_radius.is_finite()
        // #1775 — radius_variation is now forwarded + consumed as per-particle
        // size jitter, so it joins the finite sweep (a NaN/Inf spread would
        // poison every spawned particle's start_size). Negative is tolerated:
        // the consumer (apply_emitter_params) takes its magnitude.
        && p.radius_variation.is_finite()
        && p.life_span.is_finite()
        && p.life_span_variation.is_finite();
    if !(all_finite && p.life_span > 0.0 && p.initial_radius >= 0.0) {
        log::debug!(
            "Rejecting NiPSysEmitter params (non-finite or non-positive): \
             speed={} speed_var={} decl={} decl_var={} radius={} life_span={} \
             life_var={} base_scale={:?} — falling back to heuristic preset",
            p.speed,
            p.speed_variation,
            p.declination,
            p.declination_variation,
            p.initial_radius,
            p.life_span,
            p.life_span_variation,
            base_scale,
        );
        return None;
    }
    Some(crate::import::ImportedEmitterParams {
        speed: p.speed,
        speed_variation: p.speed_variation,
        declination: p.declination,
        declination_variation: p.declination_variation,
        initial_color: p.initial_color,
        initial_radius: p.initial_radius,
        life_span: p.life_span,
        life_span_variation: p.life_span_variation,
        base_scale,
        radius_variation: p.radius_variation,
    })
}

/// Authored particle budget from a `NiParticleSystem`'s own `data_ref` —
/// nif.xml `NiParticlesData.Num Vertices`, *"the maximum number of
/// particles"*, which on Bethesda `#BS202#` streams is the `BS Max
/// Vertices` upper bound (#3344). `None` when `data_ref` doesn't resolve
/// to a budget-bearing block or it authored `0`.
///
/// #4261 (OB-D4-02) — `data_ref` is now populated for every version band
/// (see the field's own doc comment on [`crate::blocks::particle::NiParticleSystem`]
/// — pre-SSE `NiGeometry.Data` and SSE+'s own later `NiPSysData` ref both
/// land in the one field), so this resolves exactly per-instance instead
/// of the prior whole-scene first-match, which gave every emitter in a
/// multi-emitter NIF the first system's particle budget. Falls back to
/// the old whole-scene scan only if `data_ref` doesn't resolve — a
/// defensive residual for any version/shape this pass didn't measure,
/// not the common case.
pub(crate) fn extract_emitter_max_particles(scene: &NifScene, data_ref: BlockRef) -> Option<u32> {
    if let Some(idx) = data_ref.index() {
        if let Some(budget) = scene
            .get_as::<crate::blocks::particle::NiPSysBlock>(idx)
            .and_then(|d| d.max_particles)
            .filter(|m| *m > 0)
        {
            return Some(budget);
        }
    }
    // Find the first block that actually *carries* a budget, not the first
    // `NiPSysBlock`: 27 other `NiPSys*` types deserialise to that same marker
    // struct with `max_particles: None`, so `find_map(downcast).and_then(..)`
    // stops at whichever marker happens to come first in block order and
    // reports no budget at all. Caught by the #3343 magnitude floors, which
    // read 0/346 on FNV against a measured 1,262 budget-bearing blocks.
    scene
        .blocks
        .iter()
        .find_map(|b| {
            b.as_any()
                .downcast_ref::<crate::blocks::particle::NiPSysBlock>()
                .and_then(|d| d.max_particles)
        })
        .filter(|m| *m > 0)
}

/// #4261 (OB-D4-02) — the modern-tier controller lookup below now walks
/// `controller_ref`'s own chain (`NiObjectNETData.controller_ref` →
/// `next_controller_ref`, the same mechanism `crate::anim` already uses
/// for embedded-animation import) for the `NiPSysEmitterCtlr` that
/// actually belongs to THIS particle system, instead of the prior
/// whole-scene first-match. Returns the controller's `interpolator_ref`
/// (a plain `BlockRef`, not the controller itself, to sidestep threading
/// a borrow out through the chain-walk callback's per-call lifetime).
fn find_own_emitter_ctlr_interpolator(
    scene: &NifScene,
    controller_ref: BlockRef,
) -> Option<BlockRef> {
    use crate::blocks::particle::NiPSysEmitterCtlr;

    let mut found: Option<BlockRef> = None;
    crate::anim::walk_controller_chain(scene, controller_ref, |_idx, block, _base| {
        if found.is_none() {
            if let Some(ctlr) = block.as_any().downcast_ref::<NiPSysEmitterCtlr>() {
                found = Some(ctlr.interpolator_ref);
            }
        }
    });
    found
}

/// Legacy `NiPSysEmitterCtlrData` tier (below) stays a whole-scene scan:
/// deprecated pre-10.2 (nif.xml), attached via an even older
/// `NiParticleSystemController` (until v10.0.1.0) this codebase doesn't
/// currently link back to a specific `NiParticleSystem` at all — a
/// residual, lower-priority scope this #4261 pass didn't extend to. The
/// modern tier above (the dominant case on every measured multi-emitter
/// NIF) is now exact.
pub(crate) fn extract_emitter_rate(scene: &NifScene, controller_ref: BlockRef) -> Option<f32> {
    use crate::anim::resolve_blend_interpolator_target;
    use crate::blocks::interpolator::{NiBlendFloatInterpolator, NiFloatData, NiFloatInterpolator};
    use crate::blocks::particle::NiPSysEmitterCtlrData;

    fn sane(r: f32) -> Option<f32> {
        // Reject non-finite, negative, the FLT_MAX sentinel (`>= 3.0e38`, the
        // shader rimlight/backlight threshold + nif.xml's "use the keyed data"
        // marker — blocks/shader.rs), AND an exact 0.0. A zero first-key is a
        // ramp-up emitter (rate climbs from 0 over the clip — geyser/steam/
        // ignition FX); taking it as a permanent-zero constant rate makes the
        // spawn guard (`em.rate > 0.0`) kill the emitter for the whole clip, so
        // fall back to the preset spawn rate instead (#1771). Rate-curve
        // sampling over time is the fuller fix (#1402).
        (r.is_finite() && 0.0 < r && r < 3.0e38).then_some(r)
    }

    // NiFloatInterpolator → (keyed data | constant). Shared by the direct
    // case below and the NiBlendFloatInterpolator sub-interpolator case
    // (#2548), so the two chains can't silently diverge.
    fn float_interpolator_rate(
        scene: &NifScene,
        interp_idx: usize,
        curves: CurveTier,
    ) -> Option<f32> {
        let interp = scene.get_as::<NiFloatInterpolator>(interp_idx)?;
        if let Some(data_idx) = interp.data_ref.index() {
            if let Some(keys) = scene.get_as::<NiFloatData>(data_idx).map(|d| &d.keys.keys) {
                if let Some(r) = keys.first().and_then(|k| sane(k.value)) {
                    return Some(r);
                }
                // #3754 — the first key was rejected (a `0.0` ramp-up start,
                // per `sane`'s note), but the rest of the curve is right
                // here and used to be discarded whole: the interpolator's
                // own `value` on this shape is the `-FLT_MAX` "use the keyed
                // data" sentinel, which `sane` also rejects, so the emitter
                // fell all the way through to `fog.rs::particle_preset`'s
                // name-heuristic guess.
                if curves == CurveTier::Allowed {
                    if let Some(r) = curve_mean_rate(keys) {
                        return Some(r);
                    }
                }
            }
        }
        sane(interp.value)
    }

    /// The constant spawn rate that reproduces an authored rate curve's
    /// particle count — its **time-weighted mean**, by trapezoid over the
    /// authored keys (#3754).
    ///
    /// Why the mean and not the peak: `ParticleEmitter::rate` is particles
    /// *per second*, applied continuously and forever, so the faithful
    /// scalar reduction of a curve is the one that emits the same number of
    /// particles per clip cycle. The curves this reaches are not the
    /// ramp-to-plateau shape they were assumed to be — measured off the
    /// three meshes the report cites:
    ///
    /// ```text
    /// tenpengate01     `Close` 2 s  : 0,0,600,0,0     — a 0.13 s spike
    /// fxfallingrocks01 `Idle` 20 s  : two 300 spikes + two 120 spikes
    /// fxbubblestall01  `Idle` 16.7 s: ramp to a 30 plateau, then holds
    /// ```
    ///
    /// Only the third is a plateau. Taking each curve's maximum would run
    /// Tenpenny's gate at 600 /s forever against an authored average of
    /// ~20 /s, and the falling-rock ambient at 300 /s against ~19 /s — 30×
    /// and 16× overshoots, i.e. *further* from the file than the 35 /s
    /// preset this replaces. The mean is within a factor of two on all
    /// three.
    ///
    /// Sampling the curve over time is still the fuller fix (#1402); this is
    /// the best constant the file supports until an emitter can hold one.
    ///
    /// Returns `None` — leaving the existing fallbacks intact — when the
    /// curve carries no positive area, spans no time, or contains a
    /// non-finite / sentinel key. Negative key values are clamped to zero
    /// rather than subtracting area: a negative spawn rate has no meaning,
    /// and letting one cancel real emission would silently mute the emitter.
    fn curve_mean_rate(keys: &[crate::blocks::interpolator::FloatKey]) -> Option<f32> {
        let mut area = 0.0f64;
        let mut span = 0.0f64;
        for w in keys.windows(2) {
            let (a, b) = (&w[0], &w[1]);
            // A sentinel or garbage key poisons the whole average, so bail
            // to the caller's fallbacks rather than averaging it in.
            if !a.value.is_finite()
                || !b.value.is_finite()
                || a.value.abs() >= 3.0e38
                || b.value.abs() >= 3.0e38
            {
                return None;
            }
            let dt = f64::from(b.time) - f64::from(a.time);
            if !dt.is_finite() || dt <= 0.0 {
                continue;
            }
            area += 0.5 * (f64::from(a.value.max(0.0)) + f64::from(b.value.max(0.0))) * dt;
            span += dt;
        }
        if span <= 0.0 {
            return None;
        }
        sane((area / span) as f32)
    }

    /// #3329 tier (d) — recover the authored rate from the scene's embedded
    /// `NiControllerSequence` blocks when the emitter controller's own
    /// interpolator is a manager-driven blend with no items.
    ///
    /// Walks every sequence's `controlled_blocks` for one whose resolved
    /// `controller_type` names an emitter controller, and runs its
    /// `interpolator_ref` through the same `float_interpolator_rate` the
    /// direct tiers use — so the four chains cannot diverge.
    ///
    /// Steady-state sequences win over transient ones. A single NIF commonly
    /// carries several (`Idle`, `Forward`, `OFF`, `Open`, …) and they author
    /// *different* rates: an ignition ramp or a one-shot burst is not the
    /// density the emitter runs at while the player is looking at it. Names
    /// are ranked, and a tie falls back to block order.
    fn sequence_emitter_rate(scene: &NifScene, curves: CurveTier) -> Option<f32> {
        use crate::anim::{resolve_cb_string, CbString};
        use crate::blocks::controller::NiControllerSequence;

        /// Lower is preferred. `Idle`/`SpecialIdle` are the steady-state
        /// loops; everything else is a transition whose rate is only correct
        /// for the moment it plays.
        fn sequence_rank(name: Option<&str>) -> u8 {
            match name.map(str::to_ascii_lowercase).as_deref() {
                Some("idle") => 0,
                Some(n) if n.ends_with("idle") => 1,
                Some(_) => 2,
                None => 3,
            }
        }

        let mut best: Option<(u8, f32)> = None;
        for seq in scene
            .blocks
            .iter()
            .filter_map(|b| b.as_any().downcast_ref::<NiControllerSequence>())
        {
            let rank = sequence_rank(seq.name.as_deref());
            // Nothing here can beat an already-found better-ranked hit.
            if best.as_ref().is_some_and(|(r, _)| *r <= rank) {
                continue;
            }
            for cb in &seq.controlled_blocks {
                let Some(ctype) = resolve_cb_string(scene, cb, CbString::ControllerType) else {
                    continue;
                };
                if !ctype.contains("EmitterCtlr") {
                    continue;
                }
                let Some(idx) = cb.interpolator_ref.index() else {
                    continue;
                };
                if let Some(r) = float_interpolator_rate(scene, idx, curves) {
                    best = Some((rank, r));
                    break;
                }
            }
        }
        best.map(|(_, r)| r)
    }

    /// Whether this pass may fall back to a rate curve's time-weighted mean
    /// (#3754). The whole tier chain runs once with [`Self::Rejected`] and,
    /// only if that finds nothing at all, again with [`Self::Allowed`].
    ///
    /// Two passes rather than one because the tiers resolve on the *first*
    /// emitter controller that yields a rate, so enabling the curve tier
    /// inline does not merely fill gaps — it lets an earlier controller win
    /// a mesh that already resolved. Measured over `Fallout - Meshes.bsa`:
    /// inline, 10 meshes gained a rate but 5 that already had one changed
    /// (`fxharoldfire` 90 → 41 /s, `ppurityfxtankfog01` 7.5 → 86.5 /s).
    /// Those five are not the defect being fixed, and re-ranking a mesh's
    /// emitters is #1402's business, not this fix's. Split into passes, the
    /// change is exactly additive: same 10 gained, zero changed.
    #[derive(Clone, Copy, PartialEq, Eq)]
    enum CurveTier {
        Rejected,
        Allowed,
    }

    fn resolve(scene: &NifScene, controller_ref: BlockRef, curves: CurveTier) -> Option<f32> {
        // Modern: controller → interpolator → (keyed data | constant).
        if let Some(interp_ref) = find_own_emitter_ctlr_interpolator(scene, controller_ref) {
            if let Some(interp_idx) = interp_ref.index() {
                if let Some(r) = float_interpolator_rate(scene, interp_idx, curves) {
                    return Some(r);
                }
                // #2548 — 78% of real FO3 NiPSysEmitterCtlr.interpolator_ref
                // targets are NiBlendFloatInterpolator (most of FO3's fire/
                // explosion/dust/blood/gore VFX library), a weighted-array
                // wrapper this branch never followed at all — only the bare
                // NiFloatInterpolator case above, on 22% of real targets.
                // `resolve_blend_interpolator_target` (#334 / AR-08, already
                // used by the KF channel-extraction path) picks the highest-
                // `normalized_weight` sub-interpolator; `None` for the
                // manager-controlled case (no items to pick from — those are
                // driven externally and don't apply to a particle emitter
                // rate anyway). Fall back to the blend interpolator's own
                // constant `value` if no item resolves.
                if let Some(sub_idx) = resolve_blend_interpolator_target(scene, interp_idx) {
                    if let Some(r) = float_interpolator_rate(scene, sub_idx, curves) {
                        return Some(r);
                    }
                }
                if let Some(blend) = scene.get_as::<NiBlendFloatInterpolator>(interp_idx) {
                    if let Some(r) = sane(blend.value) {
                        return Some(r);
                    }
                }
                // #3329 — the manager-controlled residual #2548 left behind. When
                // the controller's interpolator is a `NiBlendFloatInterpolator`
                // with an EMPTY `items` array, tier (b) above cannot resolve a
                // sub-interpolator (`resolve_blend_interpolator_target` returns
                // `None` by design for that shape — those blends are driven
                // externally by a `NiControllerManager`, not from their own array)
                // and tier (c) reads a non-positive `value`. On vanilla FNV that
                // is 168 of the 307 emitter-bearing meshes: every single affected
                // file's blend has `items.len() == 0`.
                //
                // The authored rate is still in the file — it lives on the
                // sibling `NiControllerSequence`'s controlled block for the same
                // emitter controller. Scanning those recovers 155 of the 168
                // (`fxambdust*` 25/s, snowglobes 6/s, Lucky 38 reactor, the Strip
                // fountain, Helios steam, `dlc04fxcrashthroughfloor` 510/s).
                // Without it `apply_emitter_overlays` leaves the density at
                // `fog.rs::particle_preset`'s name-heuristic guess — off by ~6×
                // for snowglobes and ~15× for the DLC04 crash FX.
                if scene
                    .get_as::<NiBlendFloatInterpolator>(interp_idx)
                    .is_some()
                {
                    if let Some(r) = sequence_emitter_rate(scene, curves) {
                        return Some(r);
                    }
                }
            }
        }
        // Legacy: NiPSysEmitterCtlrData first birth-rate key.
        scene
            .blocks
            .iter()
            .find_map(|b| b.as_any().downcast_ref::<NiPSysEmitterCtlrData>())
            .and_then(|d| d.birth_rate_first)
            .and_then(sane)
    }

    resolve(scene, controller_ref, CurveTier::Rejected)
        .or_else(|| resolve(scene, controller_ref, CurveTier::Allowed))
}

/// Flat counterpart to the particle-emitter detection in
/// `walk_node_hierarchical`: walks the scene graph accumulating world-
/// space transforms and emits one [`crate::import::ImportedParticleEmitterFlat`]
/// per renderable particle block (`NiParticleSystem` and friends). Used
/// by the cell loader, which spawns one entity per emitter at the
/// composed REFR-times-host-NIF-local world position. See #401.
pub(crate) fn walk_node_particle_emitters_flat(
    scene: &NifScene,
    block_idx: usize,
    parent_transform: &NiTransform,
    parent_node_name: Option<std::sync::Arc<str>>,
    inherited_props: &mut Vec<BlockRef>,
    pool: &mut StringPool,
    out: &mut Vec<crate::import::ImportedParticleEmitterFlat>,
) {
    let Some(block) = scene.get(block_idx) else {
        return;
    };

    // NiSwitchNode / NiLODNode: only walk the active children (#718).
    if let Some((node, active_children)) = switch_active_children(block) {
        if node.av.flags & 0x01 != 0 {
            return;
        }
        let world_transform = compose_transforms(parent_transform, &node.av.transform);
        let new_parent_name = node.av.net.name.clone().or(parent_node_name);
        let prev_len = inherited_props.len();
        inherited_props.extend_from_slice(&node.av.properties);
        for idx in active_children {
            walk_node_particle_emitters_flat(
                scene,
                idx,
                &world_transform,
                new_parent_name.clone(),
                inherited_props,
                pool,
                out,
            );
        }
        inherited_props.truncate(prev_len);
        return;
    }

    if let Some(node) = as_ni_node(block) {
        if node.av.flags & 0x01 != 0 {
            return;
        }
        let world_transform = compose_transforms(parent_transform, &node.av.transform);
        // Pass this node's name down so descendant emitters inherit a
        // sensible host name even when the emitter block itself is
        // unnamed (the common case in vanilla content).
        let new_parent_name = node.av.net.name.clone().or(parent_node_name);
        let prev_len = inherited_props.len();
        inherited_props.extend_from_slice(&node.av.properties);
        for child_ref in &node.children {
            if let Some(idx) = child_ref.index() {
                walk_node_particle_emitters_flat(
                    scene,
                    idx,
                    &world_transform,
                    new_parent_name.clone(),
                    inherited_props,
                    pool,
                    out,
                );
            }
        }
        inherited_props.truncate(prev_len);
        return;
    }

    // Mirror the hierarchical-walk dispatch (#984): handle the typed
    // `NiParticleSystem` (carries `modifier_refs`). The legacy controller
    // / particle types dispatch to `legacy_particle::*`, not `NiPSysBlock`,
    // so the old `NiPSysBlock` fall-through never matched them — that dead
    // arm was removed in #1327.
    if let Some(ps) = block
        .as_any()
        .downcast_ref::<crate::blocks::particle::NiParticleSystem>()
    {
        // Compose the particle block's own local TRS onto the host-node
        // world transform (#1333). Pre-fix only `parent_transform` (the
        // host world) was used, zeroing any authored emitter offset —
        // smoke spawned inside the fire instead of above it.
        let world_transform = compose_transforms(parent_transform, &ps.transform);
        let pmat = extract_particle_material(scene, ps, inherited_props, pool);
        out.push(crate::import::ImportedParticleEmitterFlat {
            local_position: zup_point_to_yup(&world_transform.translation),
            host_name: parent_node_name,
            original_type: ps.original_type.clone(),
            texture_path: pmat.texture_path,
            src_blend: pmat.src_blend,
            dst_blend: pmat.dst_blend,
            effect_shader: pmat.effect_shader,
            greyscale_lut_map: pmat.greyscale_lut_map,
            color_curve: extract_first_color_curve(scene, &ps.modifier_refs),
            force_fields: collect_force_fields(scene, &ps.modifier_refs),
            emitter_params: extract_emitter_params(scene, &ps.modifier_refs),
            emitter_rate: extract_emitter_rate(scene, ps.controller_ref),
            max_particles: extract_emitter_max_particles(scene, ps.data_ref),
        });
    }
}
