//! `--material-dump PATH`: one row per rendered material of the loaded scene,
//! with its M-LIGHT material class (reference-lighting §2, RLT-IBL-008, A9).
//!
//! A material class is the pair (translation route, conductor or dielectric).
//! The route is the source signal the translate step read: BGSM-authored
//! scalars for BGSM/BGEM content (`material_flag::BGSM_AUTHORED`), otherwise
//! the arm of the legacy keyword classifier that decided it
//! ([`classify_pbr_keyword_routed`]). The conductor side is metalness > 0.3,
//! the cut the renderer's environment-specular gate uses (T9).
//!
//! The dump reports the *translated* roughness and metalness the renderer
//! receives, plus the inputs the RLT-IBL-008 flags need; the flags themselves
//! are computed by `scripts/m-light/baselines.py`, so their thresholds live in
//! one place with the rest of the Phase 1b report.

use std::collections::{BTreeMap, HashSet};
use std::fmt::Write as _;
use std::path::Path;

use byroredux_core::ecs::components::material::{
    classify_pbr_keyword_routed, PbrClassifierInputs,
};
use byroredux_core::ecs::{Material, MeshHandle, World};

/// The conductor cut (reference-lighting §2, T9).
pub(crate) const CONDUCTOR_METALNESS: f32 = 0.3;

const HEADER: &str = "route\tconductor\troughness\tmetalness\tmaterial_kind\tglossiness\tenv_map_scale\thas_gloss_map\tspecular_authored\tinstances\ttexture\n";

/// The translation route of one material.
pub(crate) fn material_route(material: &Material) -> &'static str {
    let bgsm = material.effect_shader_flags
        & byroredux_renderer::vulkan::material::material_flag::BGSM_AUTHORED
        != 0;
    if bgsm {
        return "bgsm";
    }
    classify_pbr_keyword_routed(PbrClassifierInputs {
        texture_path: material.texture_path.as_deref(),
        glossiness: material.glossiness,
        env_map_scale: material.env_map_scale,
        has_normal_map: material.normal_map.is_some(),
        specular_color: material.specular_color,
        specular_authored: material.specular_authored,
        has_gloss_map: material.gloss_map.is_some(),
        has_env_mask: false,
    })
    .1
    .as_str()
}

/// One TSV row per distinct material (by its reported fields), with the
/// number of rendered instances that share it. Rows are sorted, so two dumps
/// of the same scene diff cleanly.
pub(crate) fn material_rows(world: &World) -> String {
    let mut counts: BTreeMap<String, u32> = BTreeMap::new();
    // One storage guard at a time: holding `Material` while taking `MeshHandle`
    // closes a lock-order cycle with the render path (#6618).
    let meshed: HashSet<_> = world
        .query::<MeshHandle>()
        .map(|meshes| meshes.iter().map(|(entity, _)| entity).collect())
        .unwrap_or_default();
    if let Some(materials) = world.query::<Material>() {
        for (entity, material) in materials.iter() {
            if !meshed.contains(&entity) {
                continue;
            }
            let mut row = String::new();
            let _ = write!(
                row,
                "{}\t{}\t{:.4}\t{:.4}\t{}\t{:.3}\t{:.3}\t{}\t{}",
                material_route(material),
                if material.metalness > CONDUCTOR_METALNESS { "conductor" } else { "dielectric" },
                material.roughness,
                material.metalness,
                material.material_kind,
                material.glossiness,
                material.env_map_scale,
                u8::from(material.gloss_map.is_some()),
                u8::from(material.specular_authored),
            );
            let texture = material.texture_path.as_deref().unwrap_or("").replace('\t', " ");
            let key = format!("{row}\t{texture}");
            *counts.entry(key).or_insert(0) += 1;
        }
    }
    let mut out = String::from(HEADER);
    for (key, count) in counts {
        let (fields, texture) = key.rsplit_once('\t').unwrap_or((&key, ""));
        let _ = writeln!(out, "{fields}\t{count}\t{texture}");
    }
    out
}

pub(crate) fn write_material_dump(world: &World, path: &Path) -> Result<usize, String> {
    let rows = material_rows(world);
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent).map_err(|e| format!("create {}: {e}", parent.display()))?;
    }
    std::fs::write(path, &rows).map_err(|e| format!("write {}: {e}", path.display()))?;
    Ok(rows.lines().count().saturating_sub(1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn route_names_bgsm_before_the_keyword_classifier() {
        let mut material = Material {
            texture_path: Some("textures/metal/plate.dds".to_owned()),
            ..Default::default()
        };
        assert_eq!(material_route(&material), "legacy_keyword");
        material.effect_shader_flags |=
            byroredux_renderer::vulkan::material::material_flag::BGSM_AUTHORED;
        assert_eq!(material_route(&material), "bgsm");
    }

    #[test]
    fn rows_count_instances_and_skip_meshless_materials() {
        let mut world = World::new();
        let material = Material {
            texture_path: Some("textures/wood/beam.dds".to_owned()),
            roughness: 0.7,
            metalness: 0.0,
            ..Default::default()
        };
        for _ in 0..2 {
            let e = world.spawn();
            world.insert(e, material.clone());
            world.insert(e, MeshHandle(1));
        }
        let lone = world.spawn();
        world.insert(lone, material);
        let out = material_rows(&world);
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines.len(), 2, "{out}");
        assert!(lines[1].starts_with("legacy_keyword\tdielectric\t0.7000\t0.0000"), "{out}");
        assert!(lines[1].ends_with("\t2\ttextures/wood/beam.dds"), "{out}");
    }
}
