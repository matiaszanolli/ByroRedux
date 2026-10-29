//! The imported material: texture provenance, the per-role texture set and
//! `ImportedMaterial`.

use super::*;

/// Provenance of a canonical texture path while it is still attached to an
/// imported NIF material.
///
/// Inline NIF authoring is the default. The application-side material
/// provider changes only roles it actually fills from an external sidecar;
/// this lets diagnostics distinguish NIF texture sets from BGSM/BGEM without
/// teaching the renderer about either file format.
///
/// #3906 (SF-2026-09-05-D3-01) — the `Mat` arm spent a while deliberately
/// absent: an earlier copy was **structurally unreachable** (no producer),
/// so `mat.dump`'s `src=mat` label could never appear and its absence read
/// as "not consulted" rather than "not implemented". #3906's own rule was
/// that the arm returns *at the site that fills a role from a `.mat`* —
/// which is what #4277's loose-file resolver and the CDB Phase-2 merge now
/// are, so the arm is back with real producers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ImportedTextureSource {
    #[default]
    NifTextureSet,
    Bgsm,
    Bgem,
    /// A Starfield `.mat` source: the loose JSON file (#4277 Stage A) or
    /// the compiled CDB row (`apply_cdb_material`).
    Mat,
    /// The engine replaced the authored path at spawn - an NPC's race / head /
    /// eye texture (#6131). Stamped by the caller that overrides the role, never
    /// by an importer, so a path that came from a RACE record is not reported
    /// as the NIF's own.
    RuntimeOverride,
}

/// Source-agnostic texture roles produced by NIF material translation.
///
/// Game-specific slot numbers and container formats stop at the NIF import
/// boundary. `NiTexturingProperty`, `BSShaderTextureSet`, BGSM/BGEM, and
/// `BSEffectShaderProperty` translators all populate these semantic roles;
/// asset resolution and rendering consume the same shape without branching on
/// the originating game.
///
/// `T` deliberately represents the texture at each pipeline stage:
/// `Option<FixedString>` for imported paths, `Option<String>` for resolved
/// archive paths, and bindless indices for renderer-facing material data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MaterialTextureSet<T> {
    pub base_color: T,
    pub normal: T,
    pub emissive: T,
    pub detail: T,
    /// Smoothness/specular-strength mask (legacy gloss map / BGSM smooth-spec).
    pub smooth_spec: T,
    /// Legacy multiplicative dark/light map.
    ///
    /// Population (full-corpus #4523 census, 2026-09-20): live on exactly
    /// one shipped corpus — Oblivion, 8 meshes in 6 files (the "evil"
    /// candlestick clutter pair, the persuasion minigame menu meshes, and
    /// the armor-repair white-spot overlay) — and zero everywhere else:
    /// FO3/FNV/Skyrim shipped no dark-slot NIF, and FO4+/modern content
    /// cannot reach the role at all (zero `NiTexturingProperty` blocks in
    /// all 299,936 FO4/FO76/Starfield mesh NIFs; BGSM/BGEM's version-gated
    /// slot vocabulary has no dark slot — 36,890 material files scanned,
    /// "dark" only ever inside texture file names). The combine is a plain
    /// multiplicative modulation whose neutral is white (1.0); the GLSL
    /// multiply is source-shape pinned in the renderer and the census is
    /// re-derivable via the translation-completeness dark-role harness.
    /// A new producer route must keep this census true or re-open the
    /// population question (#4422's unpinned-role lesson).
    pub dark: T,
    pub height: T,
    pub environment: T,
    pub environment_mask: T,
    pub tint: T,
    pub inner_layer: T,
    /// Standalone specular-colour map, distinct from [`Self::smooth_spec`].
    pub specular: T,
    /// Skyrim soft/rim-light mask (`BSShaderTextureSet` slot 2 when the
    /// corresponding SLSF2 feature is enabled).
    pub lighting_mask: T,
    /// Skyrim back-light transmission map (`BSShaderTextureSet` slot 7 when
    /// `SLSF2_Back_Lighting` is enabled).
    pub back_lighting: T,
    pub lighting: T,
    pub flow: T,
    pub wrinkle: T,
    pub greyscale_lut: T,
    pub reflectance: T,
    pub emittance_gradient: T,
    /// FO76/Starfield BGEM v21+ per-texel glass roughness/scratch mask.
    pub glass_roughness_scratch: T,
    /// FO76/Starfield BGEM v21+ dirt overlay composited over glass optics.
    pub glass_dirt_overlay: T,
    /// Ordered legacy material-overlay layers.
    pub decals: [T; 4],
}

impl<T> MaterialTextureSet<T> {
    /// Every semantic role paired with its canonical name, in the same slot
    /// order [`Self::map_ref`] uses.
    ///
    /// Added for #3349: `tex.missing` needed to report fallbacks per role, and
    /// the only prior role enumeration was a hand-written table inside
    /// `mat.dump`. Because `MaterialTextureHandles.textures`, and the paths and
    /// sources on `MaterialTextureDebugInfo`, are all
    /// `MaterialTextureSet<_>`-shaped, one generic walk here serves every
    /// consumer instead of each growing its own list that can drift as roles
    /// are added.
    ///
    /// Decals are flattened to `decal0..decal3`.
    pub fn roles(&self) -> impl Iterator<Item = (&'static str, &T)> {
        [
            ("base_color", &self.base_color),
            ("normal", &self.normal),
            ("emissive", &self.emissive),
            ("detail", &self.detail),
            ("smooth_spec", &self.smooth_spec),
            ("dark", &self.dark),
            ("height", &self.height),
            ("environment", &self.environment),
            ("environment_mask", &self.environment_mask),
            ("tint", &self.tint),
            ("inner_layer", &self.inner_layer),
            ("specular", &self.specular),
            ("lighting_mask", &self.lighting_mask),
            ("back_lighting", &self.back_lighting),
            ("lighting", &self.lighting),
            ("flow", &self.flow),
            ("wrinkle", &self.wrinkle),
            ("greyscale_lut", &self.greyscale_lut),
            ("reflectance", &self.reflectance),
            ("emittance_gradient", &self.emittance_gradient),
            ("glass_roughness_scratch", &self.glass_roughness_scratch),
            ("glass_dirt_overlay", &self.glass_dirt_overlay),
            ("decal0", &self.decals[0]),
            ("decal1", &self.decals[1]),
            ("decal2", &self.decals[2]),
            ("decal3", &self.decals[3]),
        ]
        .into_iter()
    }

    /// Convert every role to the representation used by the next pipeline
    /// stage while preserving the semantic slot ordering.
    pub fn map_ref<U>(&self, mut map: impl FnMut(&T) -> U) -> MaterialTextureSet<U> {
        MaterialTextureSet {
            base_color: map(&self.base_color),
            normal: map(&self.normal),
            emissive: map(&self.emissive),
            detail: map(&self.detail),
            smooth_spec: map(&self.smooth_spec),
            dark: map(&self.dark),
            height: map(&self.height),
            environment: map(&self.environment),
            environment_mask: map(&self.environment_mask),
            tint: map(&self.tint),
            inner_layer: map(&self.inner_layer),
            specular: map(&self.specular),
            lighting_mask: map(&self.lighting_mask),
            back_lighting: map(&self.back_lighting),
            lighting: map(&self.lighting),
            flow: map(&self.flow),
            wrinkle: map(&self.wrinkle),
            greyscale_lut: map(&self.greyscale_lut),
            reflectance: map(&self.reflectance),
            emittance_gradient: map(&self.emittance_gradient),
            glass_roughness_scratch: map(&self.glass_roughness_scratch),
            glass_dirt_overlay: map(&self.glass_dirt_overlay),
            decals: std::array::from_fn(|index| map(&self.decals[index])),
        }
    }

    /// Zip two canonical role sets without repeating their field order at
    /// every pipeline boundary.
    pub fn zip_map_ref<U, V>(
        &self,
        other: &MaterialTextureSet<U>,
        mut map: impl FnMut(&T, &U) -> V,
    ) -> MaterialTextureSet<V> {
        MaterialTextureSet {
            base_color: map(&self.base_color, &other.base_color),
            normal: map(&self.normal, &other.normal),
            emissive: map(&self.emissive, &other.emissive),
            detail: map(&self.detail, &other.detail),
            smooth_spec: map(&self.smooth_spec, &other.smooth_spec),
            dark: map(&self.dark, &other.dark),
            height: map(&self.height, &other.height),
            environment: map(&self.environment, &other.environment),
            environment_mask: map(&self.environment_mask, &other.environment_mask),
            tint: map(&self.tint, &other.tint),
            inner_layer: map(&self.inner_layer, &other.inner_layer),
            specular: map(&self.specular, &other.specular),
            lighting_mask: map(&self.lighting_mask, &other.lighting_mask),
            back_lighting: map(&self.back_lighting, &other.back_lighting),
            lighting: map(&self.lighting, &other.lighting),
            flow: map(&self.flow, &other.flow),
            wrinkle: map(&self.wrinkle, &other.wrinkle),
            greyscale_lut: map(&self.greyscale_lut, &other.greyscale_lut),
            reflectance: map(&self.reflectance, &other.reflectance),
            emittance_gradient: map(&self.emittance_gradient, &other.emittance_gradient),
            glass_roughness_scratch: map(
                &self.glass_roughness_scratch,
                &other.glass_roughness_scratch,
            ),
            glass_dirt_overlay: map(&self.glass_dirt_overlay, &other.glass_dirt_overlay),
            decals: std::array::from_fn(|index| map(&self.decals[index], &other.decals[index])),
        }
    }

    /// Iterate every semantic role in canonical order.
    ///
    /// Centralizing this walk keeps lifecycle operations exhaustive: callers
    /// do not need to repeat the role list when releasing, validating, or
    /// otherwise visiting a material's textures.
    pub fn values(&self) -> impl Iterator<Item = &T> {
        [
            &self.base_color,
            &self.normal,
            &self.emissive,
            &self.detail,
            &self.smooth_spec,
            &self.dark,
            &self.height,
            &self.environment,
            &self.environment_mask,
            &self.tint,
            &self.inner_layer,
            &self.specular,
            &self.lighting_mask,
            &self.back_lighting,
            &self.lighting,
            &self.flow,
            &self.wrinkle,
            &self.greyscale_lut,
            &self.reflectance,
            &self.emittance_gradient,
            &self.glass_roughness_scratch,
            &self.glass_dirt_overlay,
        ]
        .into_iter()
        .chain(self.decals.iter())
    }

    /// Iterate every role except base color.
    ///
    /// Runtime entities hold base color separately as `TextureHandle`; the
    /// remaining roles are owned by `MaterialTextureHandles`.
    pub fn secondary_values(&self) -> impl Iterator<Item = &T> {
        self.values().skip(1)
    }
}

#[cfg(test)]
mod material_texture_set_tests {
    use super::MaterialTextureSet;

    /// #4523 — the `dark` role's documented population must stay honest:
    /// exactly one producer site in the crate, and the measured census
    /// attached to the role definition. A second writer, or a doc stripped
    /// of the census, means the population premise is being inherited
    /// instead of re-derived — the #4422 class this pins against.
    #[test]
    fn dark_role_has_one_writer_and_carries_its_census_doc() {
        // Production text only: this module's own needles would otherwise
        // match themselves and the doc half could not fail (#6474).
        let types_src = byroredux_core::source_scan::production_text(include_str!("material.rs"));
        assert!(
            types_src.contains("#4523 census, 2026-09-20")
                && types_src.contains("8 meshes in 6 files"),
            "the dark role's doc must carry its measured census (Oblivion: 8 \
             meshes; everywhere else zero) — re-run the translation_\
             completeness dark-role harness if the population premise changes"
        );

        let legacy_src = include_str!("../material/legacy_properties.rs");
        assert_eq!(
            legacy_src.matches("info.dark_map = ").count(),
            1,
            "legacy_properties' NiTexturingProperty dark slot is documented as \
             the dark role's only writer; if you just added another producer \
             route, the population census and the role's pins need to be \
             re-derived (see the dark field's doc)"
        );
    }

    #[test]
    fn canonical_iteration_covers_every_role_once() {
        let textures = MaterialTextureSet {
            base_color: 0,
            normal: 1,
            emissive: 2,
            detail: 3,
            smooth_spec: 4,
            dark: 5,
            height: 6,
            environment: 7,
            environment_mask: 8,
            tint: 9,
            inner_layer: 10,
            specular: 11,
            lighting_mask: 12,
            back_lighting: 13,
            lighting: 14,
            flow: 15,
            wrinkle: 16,
            greyscale_lut: 17,
            reflectance: 18,
            emittance_gradient: 19,
            glass_roughness_scratch: 20,
            glass_dirt_overlay: 21,
            decals: [22, 23, 24, 25],
        };

        assert_eq!(
            textures.values().copied().collect::<Vec<_>>(),
            (0..26).collect::<Vec<_>>()
        );
        assert_eq!(
            textures.secondary_values().copied().collect::<Vec<_>>(),
            (1..26).collect::<Vec<_>>()
        );
    }

    /// #3734 (NIFAL-2026-08-30-D8-02) — `values()` must enumerate every
    /// field of the set, the same requirement `roles_covers_every_field_in_the_set`
    /// already pins for `roles()`. The sequential-integer test above catches a
    /// *reordering* but not an *omission*: add a 23rd role, give it `26` in
    /// that test's literal (which the compiler forces), and forget it in
    /// `values()` — the assert above still passes with 26 elements `0..=25`,
    /// silently dropping the new role from `docs/engine/nifal.md`'s
    /// designated exhaustive lifecycle contract (texture release on cell
    /// unload chief among its consumers). Cross-check against `map_ref`,
    /// which touches every field by construction, the same pattern
    /// `roles_covers_every_field_in_the_set` uses.
    #[test]
    fn values_covers_every_field_in_the_set() {
        let set = MaterialTextureSet::<u32>::default();
        let mut mapped = 0usize;
        let _ = set.map_ref(|_| {
            mapped += 1;
        });
        assert_eq!(
            set.values().count(),
            mapped,
            "values() enumerates {} slots but map_ref touches {mapped} — a role \
             was added to the struct without being added to values()",
            set.values().count(),
        );
    }
}

/// Source-normalized material payload attached to an [`ImportedMesh`].
///
/// Mesh extractors translate their property blocks into this shape once;
/// geometry-specific constructors no longer repeat the same material field
/// forwarding. BGSM/BGEM resolution may subsequently patch this payload, but
/// consumers never need to know which mesh block carried it.
#[derive(Debug, Clone)]
pub struct ImportedMaterial {
    /// Skyrim+ `BSWaterShaderProperty` authored water feature flags.
    pub water_shader_flags: u32,
    /// Whether the source shader is a dedicated mesh-water shader.
    pub is_water_shader: bool,
    /// The FO3/FNV lit shader authors its `Hair` bit: this mesh's vertex
    /// colours are a tint mask, not a colour (#6130). Raw-tier fact for the
    /// NPC spawn hook; no canonical component reads it.
    pub fallout_hair: bool,
    /// The base colour is a race-skin texture: authored under
    /// `textures\characters\`, and not one of Oblivion's underwear / groin
    /// sub-meshes that share that folder (#6090, #6477). Raw-tier fact for the
    /// NPC spawn hook's race-skin override; no canonical component reads it.
    /// Decided here, where the authored path is in hand, so the hook needs no
    /// pool snapshot and no spawn-order luck (#6476).
    pub character_skin: bool,
    pub textures: MaterialTextureSet<Option<FixedString>>,
    /// Per-role source retained for cold-path diagnostics. A role with no
    /// path is still treated as absent downstream regardless of this value.
    pub texture_sources: MaterialTextureSet<ImportedTextureSource>,
    /// #4422 — the encoded-space sample value the detail combine treats as
    /// a no-op, declared by the producer route that resolved the Detail
    /// role (`slot_role::detail_neutral_for`). Forwarded to the canonical
    /// `Material` and from there to `GpuMaterial.detailNeutral`; the shader
    /// divides by it and carries no per-game branch.
    pub detail_neutral: f32,
    pub material_path: Option<FixedString>,
    pub has_alpha: bool,
    pub src_blend_mode: u8,
    pub dst_blend_mode: u8,
    pub alpha_test: bool,
    pub alpha_threshold: f32,
    pub alpha_test_func: u8,
    pub two_sided: bool,
    /// `NiAlphaProperty.flags` bit 13 (0x2000, "No Sorter"). See
    /// `crates/nif/src/import/material/mod.rs::MaterialInfo::no_sorter`
    /// for the census and consuming sort key. #3797.
    pub no_sorter: bool,
    pub is_decal: bool,
    pub is_pbr: bool,
    pub has_translucency: bool,
    pub model_space_normals: bool,
    /// Source-authored Skyrim/FO4 soft, rim, and back-light feature gates.
    /// Scalars and maps may be present while their gate is clear, so the
    /// booleans must survive translation independently.
    pub soft_lighting: bool,
    pub rim_lighting: bool,
    pub back_lighting: bool,
    /// **Provenance only**: an external `.bgsm` *or* `.bgem` file resolved for
    /// this material. Says nothing about which fields it supplied — the BGEM
    /// arm sets this too, and BGEM authors no smoothness/specular at all.
    /// Load-bearing for the FO4-vs-Skyrim glass-keyword discriminator
    /// (`classify_glass_into_material`, #2710), which is why it stays set on
    /// both arms. For "authoritative PBR scalars were merged" use
    /// [`Self::bgsm_pbr_scalars_authored`] instead (#2609).
    pub from_bgsm: bool,
    /// #4283 — "an external material file was resolved for this
    /// material", whatever its format: the `.bgsm`/`.bgem` merge arms
    /// AND Starfield's CDB fallback all set it. Distinct from
    /// [`Self::from_bgsm`], which stays the FO4 BGSM
    /// spec-glossiness-convention signal (and is deliberately clear on
    /// the CDB path). Read by the glass keyword promotion in
    /// `classify_glass_into_material`.
    pub external_material_resolved: bool,
    pub bgem_glass: bool,
    pub thin_glass: bool,
    /// BGEM v21+ artist tint for the dielectric reflection/Fresnel lobe.
    pub glass_fresnel_color: [f32; 3],
    /// BGEM v21+ refraction deviation scale. The format default is 0.05.
    pub glass_refraction_scale: f32,
    /// BGEM v21+ base blur applied to reflected/refracted radiance.
    pub glass_blur_scale: f32,
    /// BGEM v22+ multiplier applied to [`Self::glass_blur_scale`].
    pub glass_blur_scale_factor: f32,
    /// #2609 — the BGSM arm of `merge_external_material` resolved authoritative
    /// spec-glossiness scalars and wrote them into
    /// [`Self::metalness_override`] / [`Self::roughness_override`]. With the
    /// specular block disabled only metalness is written, as the dielectric
    /// `0.0` the disabled block implies (#4941); roughness keeps the NIF side's.
    ///
    /// Distinct from [`Self::from_bgsm`], which is set on the BGEM arm as well
    /// even though BGEM leaves both overrides `None`. It is also distinct from
    /// `roughness_override.is_some()`: legacy inline-NIF content arrives with
    /// `Some(...)` from the keyword classifier, which is a *guess*, not
    /// authored data. Only this flag means "a real material file said so", so
    /// it is the correct gate for any heuristic that would otherwise overwrite
    /// resolved roughness.
    pub bgsm_pbr_scalars_authored: bool,
    /// True only when the NIF legacy keyword classifier produced the PBR
    /// overrides. Other importers may set deliberate overrides directly.
    pub pbr_classified_at_import: bool,
    pub metalness_override: Option<f32>,
    pub roughness_override: Option<f32>,
    pub translucency_subsurface_color: [f32; 3],
    pub translucency_transmissive_scale: f32,
    pub translucency_turbulence: f32,
    pub translucency_thick_object: bool,
    pub translucency_mix_albedo: bool,
    pub parallax_max_passes: Option<f32>,
    pub parallax_height_scale: Option<f32>,
    pub vertex_color_mode: u8,
    /// `NiTexturingProperty`/`BSShaderProperty` texture-address mode, per
    /// nif.xml's `TexClampMode`: 0=CLAMP_S_CLAMP_T, 1=CLAMP_S_WRAP_T,
    /// 2=WRAP_S_CLAMP_T, 3=WRAP_S_WRAP_T.
    ///
    /// Defaults to **3** (WRAP_S_WRAP_T), the Gamebryo default #610
    /// established and the value `resolve_texture` hardcodes for its
    /// clamp-unaware variant. #3515 — this used to default to `0`, the
    /// OPPOSITE end of the enum, while `MaterialInfo` (the tier the NIF
    /// walker actually fills) defaulted to `3`. Inert at the time because
    /// `into_imported_material` overwrites the field verbatim and the only
    /// production consumers keeping this default are the untextured fog
    /// volumes — but the next synthetic-geometry producer to build through
    /// `ImportedMesh::from_geometry` and bind a *tiling* texture would have
    /// got CLAMP/CLAMP on an atlas, reading as one stretched edge texel per
    /// axis.
    pub texture_clamp_mode: u8,
    /// Whether `parallax_map`'s height lives in the texture's alpha channel
    /// rather than `.r`. Set only by the Oblivion `APPLY_HILIGHT2` route,
    /// which binds the normal map into the height slot (#3530).
    pub parallax_height_in_alpha: bool,
    pub emissive_color: [f32; 3],
    pub emissive_mult: f32,
    pub emissive_source: byroredux_core::ecs::components::material::EmissiveSource,
    pub specular_color: [f32; 3],
    /// Whether [`Self::specular_color`] was actually authored by a bound
    /// `NiMaterialProperty` / `BSLightingShaderProperty`, forwarded from
    /// `MaterialInfo::specular_authored` (`crates/nif/src/import/material/`)
    /// by `into_imported_material`. Consumed by `Material::resolve_pbr`'s
    /// classifier backstop on the canonical side — see that field's doc
    /// for why the color VALUE alone cannot distinguish "authored white
    /// specular" from the unauthored struct default (#1873, #2573).
    pub specular_authored: bool,
    pub diffuse_color: [f32; 3],
    pub ambient_color: [f32; 3],
    pub specular_strength: f32,
    pub glossiness: f32,
    pub refraction_strength: f32,
    pub lighting_effect_1: f32,
    pub lighting_effect_2: f32,
    pub subsurface_rolloff: f32,
    pub rimlight_power: f32,
    pub backlight_power: f32,
    pub grayscale_to_palette_scale: f32,
    pub bgsm_greyscale_lut_is_alpha: bool,
    /// #2643 (SF-D9-2026-08-07-04) — BGEM's `grayscale_to_palette_color`
    /// enable bit, tracked independently from
    /// `bgsm_greyscale_lut_is_alpha`. The BGEM format permits authoring
    /// BOTH the shared `grayscale_to_palette_color` bit and BGEM's own
    /// `grayscale_to_palette_alpha` bit at once — `bgsm_greyscale_lut_is_alpha`
    /// alone can't represent that combination, so a BGEM authoring both
    /// used to silently lose the color variant (only `EFFECT_PALETTE_ALPHA`
    /// packed). `pack_imported_material_flags` now ORs `EFFECT_PALETTE_COLOR`
    /// and `EFFECT_PALETTE_ALPHA` independently from this field and
    /// `bgsm_greyscale_lut_is_alpha`, matching how the inline NIF
    /// effect-shader path (`pack_effect_shader_flags`) already derives the
    /// two bits independently. Always mirrors the BGSM/BGEM `base.
    /// grayscale_to_palette_color` bit; BGSM materials (no alpha variant)
    /// only ever set this one.
    pub bgsm_greyscale_lut_color: bool,
    /// #2108 (SF-D9-01) — the authored BGSM/BGEM palette-remap ENABLE bit
    /// (`grayscale_to_palette_color`, or BGEM's `grayscale_to_palette_alpha`
    /// as an alternate enable), captured at the same merge step that fills
    /// `textures.greyscale_lut`. The greyscale LUT slot is a legal,
    /// always-serialized field — its mere presence does NOT mean the
    /// material wants the palette remap; only this flag does. Gates
    /// `EFFECT_PALETTE_COLOR`/`ALPHA` in `pack_imported_material_flags`,
    /// mirroring how the inline NIF effect-shader path
    /// (`pack_effect_shader_flags`) gates on the real SLSF enable bit
    /// instead of texture presence.
    pub bgsm_greyscale_lut_enabled: bool,
    pub fresnel_power: f32,
    pub uv_offset: [f32; 2],
    pub uv_scale: [f32; 2],
    pub mat_alpha: f32,
    pub env_map_scale: f32,
    /// #4237 / FO3-D1-2026-09-11-02 — FO3/FNV `Window_Environment_Mapping`/
    /// `Eye_Environment_Mapping` shader-flag bit, forwarded as a positive
    /// glass signal for `classify_glass_into_material` (a window authored
    /// with this flag but a filename that doesn't match a glass keyword
    /// would otherwise be misclassified as opaque dielectric). `false` on
    /// every non-FO3/FNV source and on FO3/FNV meshes that authored
    /// neither bit.
    pub window_env_mapping: bool,
    pub z_test: bool,
    pub z_write: bool,
    pub z_function: u8,
    pub effect_shader: Option<BsEffectShaderData>,
    /// #4282 — FO4+ `BSSPWetnessParams` envelope captured off the bound
    /// `BSLightingShaderProperty`, in the core-owned canonical shape.
    /// Forwarded by `translate_material` to `Material::wetness`.
    /// Capture-only (no renderer consumer yet) — the sink exists so
    /// authored values are no longer discarded at the parse tier.
    pub wetness: Option<byroredux_core::ecs::components::material::WetnessShading>,
    /// #4282 — FO76+ `BSSPLuminanceParams` quad off the same property;
    /// same capture-only contract as [`Self::wetness`].
    pub luminance: Option<byroredux_core::ecs::components::material::LuminanceShading>,
    pub material_kind: u32,
    /// Normalised `BSLightingShaderType` this material's texture slots were
    /// resolved with (#2695). The REFR texture overlay reads it back so an
    /// `XTXR` slot swap lands in the same canonical role
    /// [`crate::import::material::slot_to_role`] gave the mesh's own slot.
    /// `0` (Default) for materials with no dedicated BSLightingShaderProperty
    /// — legacy FO3/FNV/Oblivion property chains, BGSM/BGEM-only, synthesized.
    pub shader_type: u32,
    /// Per-game slot vocabulary used by `BSShaderTextureSet` and REFR XTXR.
    pub texture_slot_layout: crate::import::TextureSlotLayout,
    /// Host material's explicit slot-2 glow enable. Required because Skyrim
    /// also puts soft/rim masks in slot 2 (#3068).
    pub slot2_glow_enabled: bool,
    pub shader_type_fields: byroredux_core::ecs::components::material::ShaderTypeFields,
    pub no_lighting_falloff: Option<NoLightingFalloff>,
    pub wireframe: bool,
    pub flat_shading: bool,
}

impl Default for ImportedMaterial {
    fn default() -> Self {
        Self {
            water_shader_flags: 0,
            is_water_shader: false,
            fallout_hair: false,
            character_skin: false,
            textures: MaterialTextureSet::default(),
            texture_sources: MaterialTextureSet::default(),
            detail_neutral: byroredux_core::ecs::components::material::DETAIL_NEUTRAL_MODULATE2X,
            material_path: None,
            has_alpha: false,
            src_blend_mode: 6,
            dst_blend_mode: 7,
            alpha_test: false,
            alpha_threshold: 0.5,
            alpha_test_func: 6,
            two_sided: false,
            no_sorter: false,
            is_decal: false,
            is_pbr: false,
            has_translucency: false,
            model_space_normals: false,
            soft_lighting: false,
            rim_lighting: false,
            back_lighting: false,
            from_bgsm: false,
            external_material_resolved: false,
            bgem_glass: false,
            thin_glass: false,
            glass_fresnel_color: [1.0; 3],
            // #3912 — `translate_material` forwards these verbatim, so the
            // importer's un-authored default IS the canonical value; name
            // it rather than carrying a detached literal copy.
            glass_refraction_scale:
                byroredux_core::ecs::components::material::DEFAULT_GLASS_REFRACTION_SCALE,
            glass_blur_scale: byroredux_core::ecs::components::material::DEFAULT_GLASS_BLUR_SCALE,
            glass_blur_scale_factor: 1.0,
            bgsm_pbr_scalars_authored: false,
            pbr_classified_at_import: false,
            metalness_override: None,
            roughness_override: None,
            translucency_subsurface_color: [0.0; 3],
            translucency_transmissive_scale: 0.0,
            translucency_turbulence: 0.0,
            translucency_thick_object: false,
            translucency_mix_albedo: false,
            parallax_max_passes: None,
            parallax_height_scale: None,
            vertex_color_mode: 2,
            // 3 = WRAP_S_WRAP_T — the Gamebryo default (#610 / #3515),
            // matching `MaterialInfo` and `resolve_texture`'s own fallback.
            texture_clamp_mode: 3,
            parallax_height_in_alpha: false,
            emissive_color: [0.0; 3],
            emissive_mult: 0.0,
            emissive_source: byroredux_core::ecs::components::material::EmissiveSource::None,
            specular_color: [1.0; 3],
            specular_authored: false,
            diffuse_color: [1.0; 3],
            ambient_color: [1.0; 3],
            specular_strength: 0.0,
            glossiness: 0.0,
            refraction_strength: 0.0,
            lighting_effect_1: 0.0,
            lighting_effect_2: 0.0,
            subsurface_rolloff: 0.0,
            rimlight_power: 0.0,
            backlight_power: 0.0,
            grayscale_to_palette_scale: 1.0,
            bgsm_greyscale_lut_is_alpha: false,
            bgsm_greyscale_lut_color: false,
            bgsm_greyscale_lut_enabled: false,
            fresnel_power: 5.0,
            uv_offset: [0.0, 0.0],
            uv_scale: [1.0, 1.0],
            mat_alpha: 1.0,
            env_map_scale: 0.0,
            window_env_mapping: false,
            z_test: true,
            z_write: true,
            z_function: 3,
            effect_shader: None,
            wetness: None,
            luminance: None,
            material_kind: 0,
            shader_type: 0,
            texture_slot_layout: crate::import::TextureSlotLayout::default(),
            slot2_glow_enabled: false,
            shader_type_fields: Default::default(),
            no_lighting_falloff: None,
            wireframe: false,
            flat_shading: false,
        }
    }
}

#[cfg(test)]
mod role_enumeration_tests {
    use super::MaterialTextureSet;

    /// #3349 — `roles()` must enumerate every field of the set. If a role is
    /// added to the struct and not to `roles()`, `tex.missing` silently stops
    /// reporting it, which is the exact blind spot #3349 was filed about.
    /// Cross-check against `map_ref`, which touches every field by
    /// construction: count the roles the mapper visits and compare.
    #[test]
    fn roles_covers_every_field_in_the_set() {
        let set = MaterialTextureSet::<u32>::default();
        let mut mapped = 0usize;
        let _ = set.map_ref(|_| {
            mapped += 1;
        });
        assert_eq!(
            set.roles().count(),
            mapped,
            "roles() enumerates {} slots but map_ref touches {mapped} — a role \
             was added to the struct without being added to roles()",
            set.roles().count(),
        );
    }

    /// Role names are the operator-facing labels in `tex.missing` output, so a
    /// duplicate would merge two distinct slots into one bucket.
    #[test]
    fn role_names_are_unique() {
        let set = MaterialTextureSet::<u32>::default();
        let names: Vec<&str> = set.roles().map(|(n, _)| n).collect();
        let mut sorted = names.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(
            sorted.len(),
            names.len(),
            "duplicate role name in {names:?}"
        );
    }

    /// The values yielded must be the matching field, not a fixed slot.
    #[test]
    fn roles_yields_the_matching_field() {
        let mut set = MaterialTextureSet::<u32> {
            normal: 7,
            ..Default::default()
        };
        set.decals[2] = 9;
        let by_name = |want: &str| -> u32 {
            *set.roles()
                .find(|(n, _)| *n == want)
                .expect("role must exist")
                .1
        };
        assert_eq!(by_name("normal"), 7);
        assert_eq!(by_name("decal2"), 9);
        assert_eq!(by_name("base_color"), 0);
    }
}
