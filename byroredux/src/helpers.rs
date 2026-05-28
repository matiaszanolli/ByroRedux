//! Small utility functions used across the application.

use byroredux_core::ecs::components::material::is_glass_keyword_path;
use byroredux_core::ecs::components::Material;
use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::{Children, World};

/// Glass-smooth roughness forced on a surface once it is classified
/// `MATERIAL_KIND_GLASS`. Roughness is a *consequence* of being glass,
/// not a gate for it — this value clears both the CPU glass gate
/// (`render/static_meshes.rs` `< 0.4`) and the shader gate
/// (`triangle.frag` `roughness < 0.35`) so the surface renders through
/// the IOR refraction path. Glass is microfacet-smooth (~0.05–0.1).
const GLASS_ROUGHNESS: f32 = 0.10;

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
///    `merge_bgsm_into_mesh`. This is an authoritative authored signal
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
///
/// On a match, `material_kind` becomes `MATERIAL_KIND_GLASS` and
/// `roughness` is forced glass-smooth ([`GLASS_ROUGHNESS`]) as a
/// consequence. Engine-synthesized kinds (≥ 100, e.g. EFFECT_SHADER) are
/// never overridden. Call AFTER `Material::resolve_pbr` so the glass
/// write wins over the keyword-derived roughness.
pub(crate) fn classify_glass_into_material(
    material: &mut Material,
    mesh_name: Option<&str>,
    texture_path: Option<&str>,
    has_alpha: bool,
    is_decal: bool,
    bgem_glass: bool,
) {
    // Already an engine-synthesized kind (glass / effect-shader / …) — leave it.
    if material.material_kind >= 100 {
        return;
    }
    if !has_alpha || is_decal {
        return;
    }
    // Conductors are never glass; `resolve_pbr` has already marked
    // obvious metal in `metalness` (BGSM or keyword classifier).
    if material.metalness >= 0.3 {
        return;
    }
    let keyword_match = texture_path.is_some_and(is_glass_keyword_path)
        || mesh_name.is_some_and(is_glass_keyword_path);
    if !keyword_match && !bgem_glass {
        return;
    }
    material.material_kind = byroredux_renderer::MATERIAL_KIND_GLASS;
    material.roughness = GLASS_ROUGHNESS;
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
        classify_glass_into_material(&mut m,
            Some("WhiskeyBottle01:0"),
            Some("textures/clutter/liquorbottles/whiskeybottle01.dds"),
            true,
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
        classify_glass_into_material(&mut m,
            Some("DrinkingGlass:0"),
            Some("textures/clutter/junk/kitchenutensils01.dds"),
            true,
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
        classify_glass_into_material(&mut m,
            Some("PawnShopWindow:0"),
            Some("textures/architecture/westside/pawnshop_d.dds"),
            false, // no alpha blend
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
        classify_glass_into_material(&mut m, Some("glasscasing"), Some("metalglass.dds"), true, false,
            false,
        );
        assert_eq!(m.material_kind, 0);
    }

    #[test]
    fn effect_shader_kind_is_preserved() {
        // Engine-synthesized kinds (≥ 100) win — never demote a fire plane.
        let mut m = mat();
        m.material_kind = byroredux_renderer::MATERIAL_KIND_EFFECT_SHADER;
        classify_glass_into_material(&mut m, Some("glassfire"), Some("glass.dds"), true, false,
            false,
        );
        assert_eq!(m.material_kind, byroredux_renderer::MATERIAL_KIND_EFFECT_SHADER);
    }

    #[test]
    fn decal_and_opaque_non_keyword_are_not_glass() {
        // Decal with glass keyword + alpha → excluded by the decal gate.
        let mut m = mat();
        classify_glass_into_material(&mut m, Some("glassdecal"), Some("glass.dds"), true, true,
            false,
        );
        assert_eq!(m.material_kind, 0);
        // Plain alpha-blend wood (no keyword) stays non-glass — guards the
        // historical FNV-wood-table / Markarth-banner false positives.
        let mut w = mat();
        classify_glass_into_material(&mut w,
            Some("WoodTable01"),
            Some("textures/furniture/woodtable01.dds"),
            true,
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
        // `merge_bgsm_into_mesh` forwarded the bit; this assertion
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
        );
        assert_eq!(m.material_kind, 0);
    }
}
