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
/// dielectric (`metalness < 0.3`), not-a-decal, and a glass keyword in
/// EITHER the texture path OR the mesh name. The mesh-name source is what
/// catches texture-less glass: FNV `ShotGlass` / `DrinkingGlass` share
/// the atlas texture `kitchenutensils01.dds` (no keyword) but their NIF
/// node name carries "glass". The alpha gate is what keeps an opaque
/// `PawnShopWindow` (name has "window", but no blend) OUT of the glass
/// path — pane vs. frame is disambiguated by blend state, never by the
/// keyword alone.
///
/// On a match, `material_kind` becomes `MATERIAL_KIND_GLASS` and
/// `roughness_override` is forced glass-smooth ([`GLASS_ROUGHNESS`]) as a
/// consequence. Engine-synthesized kinds (≥ 100, e.g. EFFECT_SHADER) are
/// never overridden. Call AFTER `Material::resolve_classifier_overrides`
/// so the override write wins over the keyword-derived roughness.
pub(crate) fn classify_glass_into_material(
    material: &mut Material,
    mesh_name: Option<&str>,
    texture_path: Option<&str>,
    has_alpha: bool,
    is_decal: bool,
) {
    // Already an engine-synthesized kind (glass / effect-shader / …) — leave it.
    if material.material_kind >= 100 {
        return;
    }
    if !has_alpha || is_decal {
        return;
    }
    // Conductors are never glass; the keyword classifier marks obvious
    // metal via `metalness_override`. Absent override → treat as dielectric.
    if material.metalness_override.unwrap_or(0.0) >= 0.3 {
        return;
    }
    let glass = texture_path.is_some_and(is_glass_keyword_path)
        || mesh_name.is_some_and(is_glass_keyword_path);
    if !glass {
        return;
    }
    material.material_kind = byroredux_renderer::MATERIAL_KIND_GLASS;
    material.roughness_override = Some(GLASS_ROUGHNESS);
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
        m.roughness_override = Some(0.40); // post-step-2 glossiness value
        classify_glass_into_material(
            &mut m,
            Some("WhiskeyBottle01:0"),
            Some("textures/clutter/liquorbottles/whiskeybottle01.dds"),
            true,
            false,
        );
        assert_eq!(m.material_kind, GLASS);
        assert!(m.roughness_override.unwrap() <= 0.11, "forced glass-smooth");
    }

    #[test]
    fn mesh_name_glass_catches_keywordless_texture() {
        // FNV ShotGlass / DrinkingGlass: atlas texture has NO keyword,
        // but the NIF node name does. Alpha-blended → glass.
        let mut m = mat();
        m.roughness_override = Some(0.60);
        classify_glass_into_material(
            &mut m,
            Some("DrinkingGlass:0"),
            Some("textures/clutter/junk/kitchenutensils01.dds"),
            true,
            false,
        );
        assert_eq!(m.material_kind, GLASS);
        assert!(m.roughness_override.unwrap() <= 0.11);
    }

    #[test]
    fn opaque_window_name_is_not_glass() {
        // PawnShopWindow: name has "window" but NO alpha-blend → the pane
        // is baked / separate; this opaque mesh must NOT become glass and
        // its roughness must be left untouched (no over-shine).
        let mut m = mat();
        m.roughness_override = Some(0.80);
        classify_glass_into_material(
            &mut m,
            Some("PawnShopWindow:0"),
            Some("textures/architecture/westside/pawnshop_d.dds"),
            false, // no alpha blend
            false,
        );
        assert_eq!(m.material_kind, 0, "opaque window stays non-glass");
        assert_eq!(m.roughness_override, Some(0.80), "roughness untouched");
    }

    #[test]
    fn conductor_with_glass_keyword_is_not_glass() {
        // A metal-classified surface (metalness override ≥ 0.3) is never
        // glass even if a keyword matches.
        let mut m = mat();
        m.metalness_override = Some(0.90);
        m.roughness_override = Some(0.30);
        classify_glass_into_material(&mut m, Some("glasscasing"), Some("metalglass.dds"), true, false);
        assert_eq!(m.material_kind, 0);
    }

    #[test]
    fn effect_shader_kind_is_preserved() {
        // Engine-synthesized kinds (≥ 100) win — never demote a fire plane.
        let mut m = mat();
        m.material_kind = byroredux_renderer::MATERIAL_KIND_EFFECT_SHADER;
        classify_glass_into_material(&mut m, Some("glassfire"), Some("glass.dds"), true, false);
        assert_eq!(m.material_kind, byroredux_renderer::MATERIAL_KIND_EFFECT_SHADER);
    }

    #[test]
    fn decal_and_opaque_non_keyword_are_not_glass() {
        // Decal with glass keyword + alpha → excluded by the decal gate.
        let mut m = mat();
        classify_glass_into_material(&mut m, Some("glassdecal"), Some("glass.dds"), true, true);
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
        );
        assert_eq!(w.material_kind, 0);
    }
}
