//! Imported particle emitters, force fields and colour curves.

use super::*;

/// One particle emitter discovered while walking the NIF scene graph.
/// See [`ImportedScene::particle_emitters`] for the full picture.
#[derive(Debug, Clone)]
pub struct ImportedParticleEmitter {
    /// Index into [`ImportedScene::nodes`] for the host scene-graph
    /// node — the scene builder anchors the spawned ECS emitter entity
    /// at this node so the particles inherit its world position.
    pub parent_node: Option<usize>,
    /// Original NIF block type name — `"NiParticleSystem"`,
    /// `"NiParticles"`, `"NiMeshParticleSystem"`,
    /// `"NiParticleSystemController"`, etc. The scene builder reads
    /// this only for telemetry; the heuristic preset is driven off the
    /// host node's name (`torch` / `fire` → flame, `smoke` → smoke,
    /// `magic` / `enchant` → sparkles, fallback → flame).
    pub original_type: String,
    /// Authored billboard sprite texture resolved from the particle
    /// system's shader/property chain. `None` means the source omitted a
    /// texture; consumers may use a named preset fallback, but must not
    /// render the bindless diagnostic/white slot as a particle sprite.
    pub texture_path: Option<String>,
    /// Authored Gamebryo source/destination blend factors.
    pub src_blend: Option<u8>,
    pub dst_blend: Option<u8>,
    /// Authored start / end RGBA from a `NiPSysColorModifier ->
    /// NiColorData` chain when the NIF carries one. `None` falls back
    /// to the name-heuristic preset; `Some` overrides the preset's
    /// `start_color` / `end_color` so authored Dragonsreach embers
    /// (warm orange) read distinctly from generic torch flames. Pre-#707
    /// the parser captured the ref but immediately discarded it, so
    /// every emitter rendered with the heuristic preset's colour.
    pub color_curve: Option<ParticleColorCurve>,
    /// Force fields collected from the source NIF's `modifier_refs`
    /// list — one `ImportedParticleForceField` per
    /// `NiPSys{Gravity,Vortex,Drag,Turbulence,Air,Radial}FieldModifier`
    /// in the chain. Empty when the NIF authored no field modifiers
    /// (most static-FX emitters like torches and ambient embers).
    /// See #984 / NIF-D5-ORPHAN-A2.
    pub force_fields: Vec<ImportedParticleForceField>,
    /// Authored `NiPSysEmitter` base spawn parameters from the first
    /// emitter in the modifier chain, when present. Overrides the
    /// name-heuristic preset's spawn fields at translate time. See
    /// [`ImportedEmitterParams`].
    pub emitter_params: Option<ImportedEmitterParams>,
    /// Authored birth rate (particles/sec) from the emitter's
    /// `NiPSysEmitterCtlr` chain, when present. Overrides the preset's
    /// `rate` at translate time. `None` → keep the preset rate (legacy
    /// `NiParticleSystemController` content, or no controller). See
    /// `docs/engine/nifal.md` — particles spawn-rate follow-up.
    pub emitter_rate: Option<f32>,
    /// Authored particle budget — nif.xml `NiParticlesData.Num Vertices`
    /// ("the maximum number of particles"), surviving on Bethesda `#BS202#`
    /// streams as `BS Max Vertices`. `None` when the source authored no
    /// budget (or `0`).
    ///
    /// **Not a tight bound.** Measured over `Fallout - Meshes.bsa` (307
    /// emitter NIFs, 1,262 `NiPSysData` blocks): min 2, median 125, p75
    /// 1,604, p90 onward pinned at 10,000 — a saturating authoring default,
    /// not a considered budget. 41% exceed 256. So the consumer clamps to an
    /// engine ceiling rather than honouring the authored figure outright;
    /// see `apply_emitter_overlays` (#3344).
    pub max_particles: Option<u32>,
    /// Authored `BSEffectShaderProperty` payload from the particle
    /// system's own shader/property chain, when it binds one. Mirrors
    /// [`ImportedMaterial::effect_shader`]: the scene builder packs it into
    /// `ParticleEmitter::effect_shader_flags` at the importer boundary
    /// (`cell_loader::pack_effect_shader_flags`), which is what carries
    /// BGEM palette-remap / `EFFECT_SOFT` / `EFFECT_LIT` through to the
    /// particle draws. Pre-#2610 the walker built this data and dropped it,
    /// so every particle `DrawCommand` shipped `effect_shader_flags: 0`.
    pub effect_shader: Option<BsEffectShaderData>,
    /// #3590 — the greyscale→palette LUT texture the effect-shader
    /// `effect_palette_color`/`effect_palette_alpha` bits (packed into
    /// `effect_shader_flags` above) index. Prefers the dedicated
    /// `BSShaderTextureSet` slot 3 (FO4/FO76), falls back to
    /// `effect_shader`'s own `greyscale_texture` — mirrors
    /// `ImportedMaterial::greyscale_lut`'s resolution order on the mesh
    /// path. Pre-#3590 this was dropped entirely, so a particle system
    /// that authored a palette remap forwarded the *instruction* to remap
    /// without the palette to remap through, and rendered as the
    /// un-remapped luminance sprite.
    pub greyscale_lut_map: Option<String>,
    /// The particle block's own local translation (Y-up), relative to
    /// the host node. Pre-#1333 the `NiParticleSystem`'s `NiAVObjectData`
    /// base was parsed then dropped, so an emitter authored with a
    /// non-zero offset (campfire smoke above the fire, FO4 steam stacks)
    /// spawned at the host node origin. The scene builder anchors the
    /// emitter at host-world × this local TRS. `[0, 0, 0]` for the
    /// common vanilla case (offset already baked into the host node).
    pub local_translation: [f32; 3],
    /// The particle block's own local rotation (Y-up quaternion
    /// `[x, y, z, w]`), relative to the host node. See
    /// [`ImportedParticleEmitter::local_translation`]. Identity
    /// (`[0, 0, 0, 1]`) when the block authored no rotation.
    pub local_rotation: [f32; 4],
    /// The particle block's own local uniform scale. `1.0` when the
    /// block authored none. See
    /// [`ImportedParticleEmitter::local_translation`].
    pub local_scale: f32,
}

/// One authored force field, mirrored 1:1 from a
/// `NiPSys{Gravity,Vortex,Drag,Turbulence,Air,Radial}FieldModifier`.
/// Renderer-facing fields stay in NIF Z-up local space — the scene
/// builder converts to engine Y-up world space when it spawns the
/// `ParticleEmitter.force_fields` entries (see byroredux/src/scene.rs).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ImportedParticleForceField {
    Gravity {
        direction: [f32; 3],
        strength: f32,
        decay: f32,
    },
    Vortex {
        axis: [f32; 3],
        strength: f32,
        decay: f32,
    },
    Drag {
        strength: f32,
        direction: [f32; 3],
        use_direction: bool,
    },
    Turbulence {
        frequency: f32,
        scale: f32,
    },
    Air {
        direction: [f32; 3],
        strength: f32,
        falloff: f32,
    },
    Radial {
        strength: f32,
        falloff: f32,
    },
}

/// Two-keyframe sample of a `NiColorData` curve, captured at NIF import
/// time. `start` is the t=0 RGBA value and `end` is the t=clip_end
/// RGBA value — i.e. the per-particle colour at spawn vs at death.
/// Skipping the in-between keys is the minimal first-pass per #707; a
/// full curve sampler is a follow-up. RGBA components are linear-
/// space floats in `[0, 1]` (NiColorData stores floats directly per
/// nif.xml's `Color4Key`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ParticleColorCurve {
    pub start: [f32; 4],
    pub end: [f32; 4],
}

/// Authored `NiPSysEmitter` base spawn parameters, harvested from the
/// first emitter block in the particle system's modifier chain. When
/// present these **override** the name-heuristic preset's spawn fields
/// (`torch_flame()` / `smoke()` / `magic_sparkles()` / `embers()`) so a
/// NIF that authored a fast, narrow, long-lived spray reads as one
/// rather than collapsing to a generic flame. `None` when the NIF has no
/// `NiPSys*Emitter` (every value comes from the preset). Mirrors the
/// `color_curve` / `force_fields` override precedent (#707 / #984). See
/// `docs/engine/nifal.md` — particles slice.
///
/// Angles are radians; `initial_color` is linear RGBA; `initial_radius`
/// / `life_span` are world units / seconds — all directly consumable by
/// the canonical `ParticleEmitter` (no Z-up→Y-up conversion: these are
/// scalars / angles / colour, not directions).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ImportedEmitterParams {
    pub speed: f32,
    pub speed_variation: f32,
    pub declination: f32,
    /// Raw Gamebryo half-spread: new particles draw declination uniformly
    /// over `[declination - var, declination + var]` (`NiPSEmitter.h`,
    /// Gamebryo 3.2). The canonical `ParticleEmitter` uses the full-width
    /// `± var/2` convention its speed/life jitter already uses, so the
    /// overlay boundary doubles this value (#4240).
    pub declination_variation: f32,
    /// `NiPSysEmitter.planar_angle` — azimuth of the declination plane,
    /// radians about the emitter's local Z axis from +X (Gamebryo Z-up).
    pub planar_angle: f32,
    /// `NiPSysEmitter.planar_angle_variation` — raw Gamebryo half-spread,
    /// same `± var` convention as [`Self::declination_variation`]. `>= π`
    /// covers the full circle (#4240).
    pub planar_angle_variation: f32,
    pub initial_color: [f32; 4],
    pub initial_radius: f32,
    pub life_span: f32,
    pub life_span_variation: f32,
    /// FO3+ `NiPSysGrowFadeModifier.base_scale` — multiplier on
    /// `initial_radius` giving the particle's full size. `None` when no
    /// grow/fade modifier (or Oblivion, which has no base_scale field)
    /// → treated as `1.0`. The grow/fade bell-curve *shape* is not
    /// modelled (canonical size is a linear start→end lerp); only the
    /// authored magnitude `initial_radius × base_scale` is translated.
    pub base_scale: Option<f32>,
    /// `NiPSysEmitter.radius_variation` — per-particle spawn-size spread in
    /// world units (version-gated `>= 10.4.0.1`, satisfied by every retail
    /// Bethesda title). Scaled by `base_scale` and consumed as
    /// `ParticleEmitter.start_size_variation` at the overlay boundary; `0.0`
    /// when the emitter authored no spread (#1775).
    pub radius_variation: f32,
}

/// Flat-import variant of [`ImportedParticleEmitter`] used by the cell
/// loader, which doesn't reconstruct the NIF hierarchy. Carries the
/// emitter's NIF-local position (composed up to the host node), the
/// nearest named ancestor's name (for heuristic preset selection), and
/// the original NIF block type name. The cell loader composes
/// `local_position` with the REFR placement transform to land the
/// spawned emitter entity at the correct world position. See #401.
#[derive(Debug, Clone)]
pub struct ImportedParticleEmitterFlat {
    /// Y-up local position of the emitter inside its source NIF.
    pub local_position: [f32; 3],
    /// Y-up composed NIF rotation of the emitter (host chain × particle
    /// block), glam `(x, y, z, w)` order, identity `[0, 0, 0, 1]`. #4398 —
    /// Gamebryo's spawn direction is expressed in the emitter's own frame;
    /// pre-fix only the translation half of #1333 survived this boundary,
    /// so the cell loader spawned every emitter axis-aligned to the world
    /// and the authored azimuth wedge (#4240) aimed at a fixed world
    /// direction on any rotated placement. The cell spawn composes this
    /// with the REFR rotation; `particle_system` rotates the sampled
    /// offset and cone direction by the result. NB: for
    /// `NiPSysVolumeEmitter` blocks the authored frame is technically the
    /// discarded `Emitter Object` ref's node (see `blocks/particle.rs`);
    /// this is the host-chain frame, identical wherever authors leave that
    /// ref unset or self-pointing.
    pub local_rotation: [f32; 4],
    /// Nearest named ancestor's name in the NIF hierarchy. Used by the
    /// heuristic preset selector (`torch`/`fire`/`flame`/`brazier`/
    /// `candle` → flame, `smoke`/`steam` → smoke, `magic`/`enchant`/
    /// `sparkle`/`glow` → sparkles, fallback → flame).
    pub host_name: Option<std::sync::Arc<str>>,
    /// Original NIF block type name — used only for telemetry.
    pub original_type: String,
    /// Authored billboard sprite texture (owned because this flat import
    /// does not retain the temporary string pool used during extraction).
    pub texture_path: Option<String>,
    /// Authored Gamebryo source/destination blend factors.
    pub src_blend: Option<u8>,
    pub dst_blend: Option<u8>,
    /// Authored colour curve from `NiPSysColorModifier -> NiColorData`,
    /// when the NIF carries one. Overrides the heuristic preset's
    /// start / end colour at spawn time. `None` falls back to the
    /// preset. See [`ImportedParticleEmitter::color_curve`] for the
    /// rationale; same field, same #707 / FX-2 origin.
    pub color_curve: Option<ParticleColorCurve>,
    /// Force fields collected from the source NIF's
    /// `NiPSys{Gravity,Vortex,Drag,Turbulence,Air,Radial}FieldModifier`
    /// chain — empty for most non-FX emitters. See
    /// [`ImportedParticleEmitter::force_fields`] / #984.
    pub force_fields: Vec<ImportedParticleForceField>,
    /// Authored `NiPSysEmitter` base spawn parameters, when present.
    /// Mirror of [`ImportedParticleEmitter::emitter_params`] for the
    /// flat (cell-loader) path. See [`ImportedEmitterParams`].
    pub emitter_params: Option<ImportedEmitterParams>,
    /// Mirror of [`ImportedParticleEmitter::emitter_rate`] for the flat
    /// (cell-loader) path. Authored birth rate (particles/sec).
    pub emitter_rate: Option<f32>,
    /// Mirror of [`ImportedParticleEmitter::max_particles`] for the flat
    /// (cell-loader) path. Authored particle budget (`BS Max Vertices`).
    pub max_particles: Option<u32>,
    /// Mirror of [`ImportedParticleEmitter::effect_shader`] for the flat
    /// (cell-loader) path. See #2610.
    pub effect_shader: Option<BsEffectShaderData>,
    /// Mirror of [`ImportedParticleEmitter::greyscale_lut_map`] for the
    /// flat (cell-loader) path. See #3590.
    pub greyscale_lut_map: Option<String>,
}
