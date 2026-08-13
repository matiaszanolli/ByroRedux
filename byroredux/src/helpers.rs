//! Small utility functions used across the application.

use byroredux_core::ecs::components::material::{is_glass_keyword_path, GLASS_SURFACE_BEHAVIOR};
use byroredux_core::ecs::components::Material;
use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::{Children, World};

/// Polished metal backing used for legacy mirror panes. A mirror is an
/// opaque conductor, not a transmissive dielectric: alpha test only cuts
/// away the broken portions of the sheet.
const MIRROR_ROUGHNESS: f32 = 0.04;
const MIRROR_METALNESS: f32 = 1.0;

fn is_mirror_pane(
    mesh_name: Option<&str>,
    texture_path: Option<&str>,
    has_transparent_coverage: bool,
) -> bool {
    has_transparent_coverage
        && mesh_name.is_some_and(|name| name.to_ascii_lowercase().contains("mirror"))
        && texture_path.is_some_and(is_glass_keyword_path)
}

/// Canonical glass classification at material-insert (spawn) time — the
/// single alpha-aware glass decision (canonical-material pass step 3).
///
/// Glass is the conjunction of an authoritative transparency signal
/// (`has_alpha` = the mesh authored `NiAlphaProperty` blend), a
/// dielectric (`metalness < 0.3`), not-a-decal, and EITHER:
///
/// 1. A glass keyword in the texture path OR the mesh name (the legacy
///    path that covers Oblivion / FO3 / FNV / Skyrim content).
/// 2. **`bgem_glass = true`** — the FO4+ BGEM `glass_enabled` flag set by
///    `merge_external_material`. This is an authoritative authored signal
///    that catches FO4 BGEM glass bottles whose atlas texture (e.g.
///    `clutter01.dds`) and node name (e.g. `Bottle:0`) match nothing in
///    the keyword list. Pre-#1280 sub-step 3b those bottles rendered as
///    opaque plastic (`material_kind = 0`, default roughness 0.80).
///
/// The mesh-name source on path 1 is what catches texture-less glass:
/// FNV `ShotGlass` / `DrinkingGlass` share the atlas texture
/// `kitchenutensils01.dds` (no keyword) but their NIF node name carries
/// "glass". The alpha gate is what keeps an opaque `PawnShopWindow`
/// (name has "window", but no blend) OUT of the glass path — pane vs.
/// frame is disambiguated by blend state, never by the keyword alone.
/// Alpha-tested glass is deliberately allowed: broken-pane sheets use alpha
/// test for shard coverage but still need dielectric shading on surviving
/// fragments. Mirror panes are the exception: their mesh name identifies an
/// opaque reflective backing even when the shared cutout texture contains a
/// `glass` keyword.
///
/// On a match, `material_kind` becomes `MATERIAL_KIND_GLASS` and the shared
/// [`GLASS_SURFACE_BEHAVIOR`] supplies metalness, roughness, and IOR. Authored
/// texture-map paths and their parameters remain untouched overlays.
///
/// Existing engine-synthesized kinds are preserved unless the material is an
/// effect-shader carrier authoring an explicit glass surface — either through
/// the authoritative FO4+ `bgem_glass` flag, or through a glass keyword on
/// content that resolved an external material file (`from_bgsm`). A keyword
/// alone, with no external material, cannot override an authored shader type:
/// Skyrim's alchemy workbench intentionally maps its emissive `InnerHaze`
/// effect layers to `plainglasstile01.dds`, and promoting those layers to
/// glass erases their emission and turns the enclosing apparatus black.
/// `from_bgsm` is the provenance signal that separates the two — `.bgem` does
/// not exist pre-FO4, so Skyrim's inline effect shaders never set it. See
/// #2710 for why the keyword arm is gated rather than absent.
/// Call AFTER `Material::resolve_pbr` so the behavior write wins over
/// source-derived PBR scalars.
pub(crate) fn classify_glass_into_material(
    material: &mut Material,
    mesh_name: Option<&str>,
    texture_path: Option<&str>,
    has_transparent_coverage: bool,
    is_decal: bool,
    bgem_glass: bool,
    from_bgsm: bool,
) {
    let keyword_match = texture_path.is_some_and(is_glass_keyword_path)
        || mesh_name.is_some_and(is_glass_keyword_path);
    // #2710 — the effect-shader carrier is promoted to glass by an
    // authored BGEM glass flag, or by a glass keyword **on external-material
    // content only**. `from_bgsm` is the provenance discriminator, not a
    // game check: an external `.bgsm`/`.bgem` resolved for this material.
    // That is what separates the two real cases, both of which are
    // effect-shader carriers with a glass keyword:
    //
    //   * FO4+ authors ordinary glass on `BSEffectShaderProperty` with a
    //     `.bgem` beside it (Nuka-Cola `nukacola_glass.dds`), sometimes
    //     without setting the BGEM glass flag. That should be glass.
    //   * Skyrim's alchemy bench authors emissive `InnerHaze` layers on the
    //     same carrier and shares `plainglasstile01.dds` with the
    //     surrounding shells — with no `.bgem`, because the format does not
    //     exist pre-FO4. Promoting those erases their emission and turns the
    //     apparatus black (`322f33a8`).
    //
    // `322f33a8` fixed the second case by deleting the keyword arm outright,
    // which made the first structurally unreachable for any effect material
    // whose BGEM misses `bgem_uses_glass_behavior`'s heuristic.
    let effect_glass_carrier = material.material_kind
        == byroredux_renderer::MATERIAL_KIND_EFFECT_SHADER
        && (bgem_glass || (keyword_match && from_bgsm));

    // Engine-synthesized behavior already selected — preserve it unless this
    // is the source-format effect carrier used to author an explicit glass
    // surface. The transparency/dielectric/decal gates below still apply.
    if material.material_kind >= 100
        && material.material_kind != byroredux_renderer::MATERIAL_KIND_GLASS
        && !effect_glass_carrier
    {
        return;
    }
    if is_mirror_pane(mesh_name, texture_path, has_transparent_coverage) {
        material.material_kind = 0;
        material.metalness = MIRROR_METALNESS;
        material.roughness = MIRROR_ROUGHNESS;
        return;
    }
    if !has_transparent_coverage || is_decal {
        return;
    }
    // Conductors are never inferred as glass from a keyword. An authored
    // BGEM glass signal is stronger than the source carrier's fallback PBR
    // values, however: legacy BSEffect defaults to metalness 0.4 even for
    // clear shells such as Port-A-Diner. The shared behavior overwrites that
    // placeholder metalness below.
    let bgem_effect_fallback =
        bgem_glass && material.material_kind == byroredux_renderer::MATERIAL_KIND_EFFECT_SHADER;
    if material.metalness >= 0.3 && !bgem_effect_fallback {
        return;
    }
    if !keyword_match && !bgem_glass {
        return;
    }
    material.material_kind = byroredux_renderer::MATERIAL_KIND_GLASS;
    material.apply_surface_behavior(GLASS_SURFACE_BEHAVIOR);
}

/// Add a child entity to a parent's Children component, creating it if needed.
pub(crate) fn add_child(world: &mut World, parent: EntityId, child: EntityId) {
    let has_children = world
        .query::<Children>()
        .map(|q| q.get(parent).is_some())
        .unwrap_or(false);

    if has_children {
        let mut cq = world.query_mut::<Children>().unwrap();
        cq.get_mut(parent).unwrap().0.push(child);
    } else {
        world.insert(parent, Children(vec![child]));
    }
}

pub(crate) fn world_resource_set<R: byroredux_core::ecs::Resource>(
    world: &World,
    f: impl FnOnce(&mut R),
) {
    let mut guard = world.resource_mut::<R>();
    f(&mut guard);
}

#[cfg(test)]
mod glass_classification_tests {
    use super::*;

    const GLASS: u32 = byroredux_renderer::MATERIAL_KIND_GLASS;

    fn mat() -> Material {
        Material::default()
    }

    #[test]
    fn texture_keyword_glass_with_alpha_is_classified_and_smoothed() {
        // FNV whiskey bottle: texture path carries "bottle", alpha-blend.
        let mut m = mat();
        m.roughness = 0.40; // post-resolve_pbr glossiness value
        classify_glass_into_material(
            &mut m,
            Some("WhiskeyBottle01:0"),
            Some("textures/clutter/liquorbottles/whiskeybottle01.dds"),
            true,
            false,
            false,
            false,
        );
        assert_eq!(m.material_kind, GLASS);
        assert!(m.roughness <= 0.11, "forced glass-smooth");
    }

    #[test]
    fn mesh_name_glass_catches_keywordless_texture() {
        // FNV ShotGlass / DrinkingGlass: atlas texture has NO keyword,
        // but the NIF node name does. Alpha-blended → glass.
        let mut m = mat();
        m.roughness = 0.60;
        classify_glass_into_material(
            &mut m,
            Some("DrinkingGlass:0"),
            Some("textures/clutter/junk/kitchenutensils01.dds"),
            true,
            false,
            false,
            false,
        );
        assert_eq!(m.material_kind, GLASS);
        assert!(m.roughness <= 0.11);
    }

    #[test]
    fn alpha_tested_restroom_mirror_is_opaque_reflective() {
        // FNV restroom mirrors use a shared broken-glass alpha-test texture
        // to cut holes in a silver-backed pane. The surviving coverage must
        // reflect, not refract the room behind the wall.
        let mut m = mat();
        m.alpha_test = true;
        m.roughness = 0.80;
        classify_glass_into_material(
            &mut m,
            Some("RestroomMirror01:1"),
            Some("textures/clutter/junk/brokenglasssheet01.dds"),
            true,
            false,
            false,
            false,
        );
        assert_eq!(m.material_kind, 0);
        assert_eq!(m.metalness, MIRROR_METALNESS);
        assert_eq!(m.roughness, MIRROR_ROUGHNESS);
    }

    #[test]
    fn alpha_tested_broken_pane_without_mirror_name_is_glass() {
        let mut m = mat();
        m.alpha_test = true;
        m.roughness = 0.80;
        classify_glass_into_material(
            &mut m,
            Some("BrokenWindowPane:1"),
            Some("textures/clutter/junk/brokenglasssheet01.dds"),
            true,
            false,
            false,
            false,
        );
        assert_eq!(m.material_kind, GLASS);
        assert!(m.roughness <= 0.11);
    }

    #[test]
    fn opaque_window_name_is_not_glass() {
        // PawnShopWindow: name has "window" but NO alpha-blend → the pane
        // is baked / separate; this opaque mesh must NOT become glass and
        // its roughness must be left untouched (no over-shine).
        let mut m = mat();
        m.roughness = 0.80;
        classify_glass_into_material(
            &mut m,
            Some("PawnShopWindow:0"),
            Some("textures/architecture/westside/pawnshop_d.dds"),
            false, // no alpha blend
            false,
            false,
            false,
        );
        assert_eq!(m.material_kind, 0, "opaque window stays non-glass");
        assert_eq!(m.roughness, 0.80, "roughness untouched");
    }

    #[test]
    fn conductor_with_glass_keyword_is_not_glass() {
        // A metal-classified surface (metalness override ≥ 0.3) is never
        // glass even if a keyword matches.
        let mut m = mat();
        m.metalness = 0.90;
        m.roughness = 0.30;
        classify_glass_into_material(
            &mut m,
            Some("glasscasing"),
            Some("metalglass.dds"),
            true,
            false,
            false,
            false,
        );
        assert_eq!(m.material_kind, 0);
    }

    #[test]
    fn unrelated_effect_shader_kind_is_preserved() {
        // Engine-synthesized kinds (≥ 100) win when there is no explicit
        // glass signal — never demote an ordinary fire plane.
        let mut m = mat();
        m.material_kind = byroredux_renderer::MATERIAL_KIND_EFFECT_SHADER;
        classify_glass_into_material(
            &mut m,
            Some("fireplane"),
            Some("textures/effects/fire.dds"),
            true,
            false,
            false,
            false,
        );
        assert_eq!(
            m.material_kind,
            byroredux_renderer::MATERIAL_KIND_EFFECT_SHADER
        );
    }

    #[test]
    fn glass_keyword_does_not_override_effect_shader_semantics() {
        // Skyrim's alchemy workbench authors InnerHaze as BSEffectShader but
        // shares plainglasstile01.dds with the surrounding glass shells. The
        // explicit shader role outranks this ambiguous texture keyword.
        //
        // `from_bgsm = false` is the load-bearing input (#2710): Skyrim has
        // no `.bgem`, so nothing external corroborates the keyword. The FO4
        // twin of this test is
        // `glass_keyword_promotes_effect_shader_carrier_with_external_material`
        // below — the two directions must be pinned together, which is what
        // `322f33a8` lost when it rewrote this test in place.
        let mut m = mat();
        m.material_kind = byroredux_renderer::MATERIAL_KIND_EFFECT_SHADER;
        m.texture_path = Some("textures/clutter/plainglasstile01.dds".into());
        let texture_path = m.texture_path.clone();

        classify_glass_into_material(
            &mut m,
            Some("InnerHaze01:8"),
            texture_path.as_deref(),
            true,
            false,
            false,
            false,
        );

        assert_eq!(
            m.material_kind,
            byroredux_renderer::MATERIAL_KIND_EFFECT_SHADER
        );
        assert_eq!(
            m.texture_path.as_deref(),
            Some("textures/clutter/plainglasstile01.dds")
        );
    }

    /// #2710 — the FO4 direction, restored beside the Skyrim one rather
    /// than replacing it. FO4 commonly authors ordinary glass on a
    /// `BSEffectShaderProperty` whose `.bgem` does not set the glass flag
    /// (`bgem_uses_glass_behavior` is a heuristic bundle, see #2626), and
    /// the explicit semantic name/texture is then the only signal there is.
    /// The external material is what makes that signal trustworthy here.
    #[test]
    fn glass_keyword_promotes_effect_shader_carrier_with_external_material() {
        let mut m = mat();
        m.material_kind = byroredux_renderer::MATERIAL_KIND_EFFECT_SHADER;
        m.texture_path = Some("textures/clutter/nukacola_glass.dds".into());
        let texture_path = m.texture_path.clone();

        classify_glass_into_material(
            &mut m,
            Some("NukaCola_Glass:3"),
            texture_path.as_deref(),
            true,
            false,
            false, // BGEM resolved, but its glass flag is NOT set
            true,  // …and that resolution is the provenance signal
        );

        assert_eq!(m.material_kind, GLASS);
        assert_eq!(m.roughness, GLASS_SURFACE_BEHAVIOR.roughness);
        assert_eq!(m.ior, GLASS_SURFACE_BEHAVIOR.ior);
        assert_eq!(
            m.texture_path.as_deref(),
            Some("textures/clutter/nukacola_glass.dds"),
            "promotion must not disturb the authored texture overlay"
        );
    }

    /// #2710 — the discriminator is provenance, not game. The same FO4-shaped
    /// carrier and keyword with no external material resolved stays an effect
    /// surface, so a future refactor can't quietly turn `from_bgsm` back into
    /// "is this FO4" and get the Skyrim case right by accident.
    #[test]
    fn glass_keyword_alone_does_not_promote_an_inline_effect_carrier() {
        let mut m = mat();
        m.material_kind = byroredux_renderer::MATERIAL_KIND_EFFECT_SHADER;
        m.texture_path = Some("textures/clutter/nukacola_glass.dds".into());
        let texture_path = m.texture_path.clone();

        classify_glass_into_material(
            &mut m,
            Some("NukaCola_Glass:3"),
            texture_path.as_deref(),
            true,
            false,
            false,
            false, // no `.bgem` resolved for this mesh
        );

        assert_eq!(
            m.material_kind,
            byroredux_renderer::MATERIAL_KIND_EFFECT_SHADER,
            "an uncorroborated keyword must not outrank the authored carrier"
        );
    }

    #[test]
    fn decal_and_opaque_non_keyword_are_not_glass() {
        // Decal with glass keyword + alpha → excluded by the decal gate.
        let mut m = mat();
        classify_glass_into_material(
            &mut m,
            Some("glassdecal"),
            Some("glass.dds"),
            true,
            true,
            false,
            false,
        );
        assert_eq!(m.material_kind, 0);
        // Plain alpha-blend wood (no keyword) stays non-glass — guards the
        // historical FNV-wood-table / Markarth-banner false positives.
        let mut w = mat();
        classify_glass_into_material(
            &mut w,
            Some("WoodTable01"),
            Some("textures/furniture/woodtable01.dds"),
            true,
            false,
            false,
            false,
        );
        assert_eq!(w.material_kind, 0);
    }

    // ── #1280 sub-step 3b — BGEM glass_enabled as authoritative trigger ──

    #[test]
    fn bgem_glass_classifies_keywordless_bottle() {
        // FO4 BGEM glass bottle: atlas texture `clutter01.dds` (no
        // keyword), node name `Bottle:0` (no glass keyword), but the
        // referenced .bgem file authored `glass_enabled = true`.
        // `merge_external_material` forwarded the bit; this assertion
        // pins that we honour it as an authoritative glass signal.
        let mut m = mat();
        m.roughness = 0.80; // post-BGSM-merge non-glass roughness
        classify_glass_into_material(
            &mut m,
            Some("Bottle:0"),
            Some("textures/clutter/clutter01.dds"),
            true,
            false,
            true, // bgem_glass
            false,
        );
        assert_eq!(
            m.material_kind, GLASS,
            "BGEM glass_enabled must classify even without keyword in path/name"
        );
        assert!(
            m.roughness <= 0.11,
            "BGEM glass must still be forced glass-smooth"
        );
    }

    #[test]
    fn bgem_glass_promotes_effect_carrier_and_preserves_texture_overlays() {
        // Real FO4 path: the NIF's BSEffectShaderProperty first selects kind
        // 101, then the referenced BGEM supplies glass_enabled plus its map
        // set. The carrier kind must not prevent the shared glass behavior.
        let mut m = mat();
        m.material_kind = byroredux_renderer::MATERIAL_KIND_EFFECT_SHADER;
        // Real BSEffect fallback before shared glass behavior is applied.
        m.metalness = 0.40;
        m.texture_path = Some("textures/clutter/clutter01.dds".into());
        m.normal_map = Some("textures/clutter/clutter01_n.dds".into());
        m.glow_map = Some("textures/clutter/clutter01_g.dds".into());
        let texture_path = m.texture_path.clone();

        classify_glass_into_material(
            &mut m,
            Some("Bottle:0"),
            texture_path.as_deref(),
            true,
            false,
            true,
            true,
        );

        assert_eq!(m.material_kind, GLASS);
        assert_eq!(m.roughness, GLASS_SURFACE_BEHAVIOR.roughness);
        assert_eq!(m.metalness, GLASS_SURFACE_BEHAVIOR.metalness);
        assert_eq!(m.ior, GLASS_SURFACE_BEHAVIOR.ior);
        assert_eq!(
            m.texture_path.as_deref(),
            Some("textures/clutter/clutter01.dds")
        );
        assert_eq!(
            m.normal_map.as_deref(),
            Some("textures/clutter/clutter01_n.dds")
        );
        assert_eq!(
            m.glow_map.as_deref(),
            Some("textures/clutter/clutter01_g.dds")
        );
    }

    #[test]
    fn bgem_glass_without_alpha_is_not_classified() {
        // Even when BGEM authored `glass_enabled = true`, an opaque
        // mesh (`has_alpha = false`) is left alone — the alpha gate is
        // the authoritative transparency check and applies to both
        // legacy keyword and BGEM signal paths. A modder's BGEM with a
        // stuck glass_enabled flag on an opaque architecture piece
        // must NOT silently become a glass surface.
        let mut m = mat();
        classify_glass_into_material(
            &mut m,
            Some("OpaqueFloor"),
            Some("textures/opaque.dds"),
            false, // no alpha
            false,
            true, // bgem_glass
            false,
        );
        assert_eq!(m.material_kind, 0);
    }

    #[test]
    fn bgem_glass_on_decal_is_not_classified() {
        // Decals are excluded from glass classification even with
        // bgem_glass set — the decal gate is upstream of the trigger
        // check.
        let mut m = mat();
        classify_glass_into_material(
            &mut m,
            Some("BgemDecal"),
            Some("textures/decal.dds"),
            true,
            true, // is_decal
            true, // bgem_glass
            false,
        );
        assert_eq!(m.material_kind, 0);
    }

    #[test]
    fn bgem_glass_on_conductor_is_not_classified() {
        // A material the keyword classifier marked as metal (e.g.
        // chromed surface that the BGEM author also flagged
        // glass-style for refraction) is still NOT classified as glass
        // — metalness >= 0.3 is the dielectric gate, and glass is
        // dielectric by definition. Mod conflict / mis-authoring stays
        // visible rather than being silently mis-classified.
        let mut m = mat();
        m.metalness = 0.85;
        classify_glass_into_material(
            &mut m,
            Some("ChromedGlassyTrim"),
            Some("textures/trim.dds"),
            true,
            false,
            true, // bgem_glass
            false,
        );
        assert_eq!(m.material_kind, 0);
    }
}
