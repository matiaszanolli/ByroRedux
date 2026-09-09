//! Light extraction (#3856, split from `walk/mod.rs`).
//!
//! `walk_node_lights` is an independent entry point invoked from
//! `import/mod.rs`, not from the scene-graph walkers.

use crate::blocks::light::{NiAmbientLight, NiDirectionalLight, NiPointLight, NiSpotLight};
use crate::scene::NifScene;
use crate::types::NiTransform;

use super::super::coord::zup_point_to_yup;
use super::super::transform::compose_transforms;
use super::super::{ImportedLight, LightKind};
use super::node_attrs::is_editor_marker;
use super::{as_ni_node, resolve_affected_node_names, switch_active_children};

/// Recursively walk the scene graph accumulating world-space transforms
/// and collecting any NiLight subclass encountered.
pub(crate) fn walk_node_lights(
    scene: &NifScene,
    block_idx: usize,
    parent_transform: &NiTransform,
    out: &mut Vec<ImportedLight>,
) {
    let Some(block) = scene.get(block_idx) else {
        return;
    };

    // NiSwitchNode / NiLODNode: only walk the active children (#718).
    if let Some((node, active_children)) = switch_active_children(block) {
        if node.av.flags & 0x01 != 0 {
            return;
        }
        if is_editor_marker(node.av.net.name.as_deref()) {
            return;
        }
        let world_transform = compose_transforms(parent_transform, &node.av.transform);
        for idx in active_children {
            walk_node_lights(scene, idx, &world_transform, out);
        }
        return;
    }

    if let Some(node) = as_ni_node(block) {
        if node.av.flags & 0x01 != 0 {
            return;
        }
        if is_editor_marker(node.av.net.name.as_deref()) {
            return;
        }
        let world_transform = compose_transforms(parent_transform, &node.av.transform);
        for child_ref in &node.children {
            if let Some(idx) = child_ref.index() {
                walk_node_lights(scene, idx, &world_transform, out);
            }
        }
        return;
    }

    // NiLight subclasses — extract using the world transform composed from
    // the parent chain plus the light's own local transform.
    if let Some(l) = block.as_any().downcast_ref::<NiPointLight>() {
        let world = compose_transforms(parent_transform, &l.base.av.transform);
        let radius = attenuation_radius(
            l.constant_attenuation,
            l.linear_attenuation,
            l.quadratic_attenuation,
        );
        out.push(imported_light_from_base(
            scene,
            &world,
            &l.base,
            LightKind::Point,
            radius,
            0.0,
        ));
        return;
    }
    if let Some(l) = block.as_any().downcast_ref::<NiSpotLight>() {
        let world = compose_transforms(parent_transform, &l.point.base.av.transform);
        let radius = attenuation_radius(
            l.point.constant_attenuation,
            l.point.linear_attenuation,
            l.point.quadratic_attenuation,
        );
        out.push(imported_light_from_base(
            scene,
            &world,
            &l.point.base,
            LightKind::Spot,
            radius,
            l.outer_spot_angle,
        ));
        return;
    }
    if let Some(l) = block.as_any().downcast_ref::<NiAmbientLight>() {
        let world = compose_transforms(parent_transform, &l.base.av.transform);
        out.push(imported_light_from_base(
            scene,
            &world,
            &l.base,
            LightKind::Ambient,
            0.0,
            0.0,
        ));
        return;
    }
    if let Some(l) = block.as_any().downcast_ref::<NiDirectionalLight>() {
        let world = compose_transforms(parent_transform, &l.base.av.transform);
        out.push(imported_light_from_base(
            scene,
            &world,
            &l.base,
            LightKind::Directional,
            0.0,
            0.0,
        ));
        // no return — directional lights are leaves
    }
}

pub(crate) fn imported_light_from_base(
    scene: &NifScene,
    world: &NiTransform,
    base: &crate::blocks::light::NiLightBase,
    kind: LightKind,
    radius: f32,
    outer_angle: f32,
) -> ImportedLight {
    let translation = zup_point_to_yup(&world.translation);

    // Gamebryo lights point down the local -Z axis in their own space.
    // Transform that via the world rotation, then convert to Y-up.
    let rot = &world.rotation;
    // Extract local -Z column (light points along -Z in Gamebryo), then
    // convert Z-up to Y-up via the same [x, z, -y] swap that zup_point_to_yup uses.
    let [dx, dy, dz] = [-rot.rows[0][2], -rot.rows[1][2], -rot.rows[2][2]];
    let direction = byroredux_core::math::coord::zup_to_yup_pos([dx, dy, dz]);

    // Dimmer scales the diffuse contribution — the only channel the
    // engine currently consumes. Ambient/specular are stored for later.
    // Gamebryo stores light colors as raw floats in "monitor space" —
    // effectively sRGB values used as-is with no gamma conversion.  We
    // pass them through unchanged because the legacy content was
    // authored for this non-linear-aware pipeline.
    let d = base.dimmer;
    let diffuse = base.diffuse_color;
    let color = [diffuse.r * d, diffuse.g * d, diffuse.b * d];

    let affected_node_names = resolve_affected_node_names(scene, &base.affected_nodes);

    ImportedLight {
        translation,
        direction,
        color,
        radius,
        kind,
        outer_angle,
        affected_node_names,
        // #983 — surface the light's NIF block name so the cell
        // loader can spawn a matching `Name` component; the
        // animation system resolves NiLight*Controller channels by
        // that name. `None` for anonymous lights (rare).
        name: base.av.net.name.clone(),
    }
}

/// Solve `1 / (const + lin·d + quad·d²) = THRESHOLD` for distance.
/// A light's "effective radius" is the distance at which its contribution
/// drops below a small fraction of its peak. We use 1/256 (~0.4%) which
/// matches what Bethesda shaders use as a cull threshold.
pub(crate) fn attenuation_radius(k_const: f32, k_lin: f32, k_quad: f32) -> f32 {
    const THRESHOLD: f32 = 1.0 / 256.0;
    // Find distance d where k_quad·d² + k_lin·d + k_const = 1/THRESHOLD
    let target = 1.0 / THRESHOLD;
    if k_quad > 1e-6 {
        // Quadratic: d = (-b + sqrt(b² - 4a(c - target))) / 2a
        let a = k_quad;
        let b = k_lin;
        let c = k_const - target;
        let disc = b * b - 4.0 * a * c;
        if disc >= 0.0 {
            return ((-b + disc.sqrt()) / (2.0 * a)).max(0.0);
        }
    }
    if k_lin > 1e-6 {
        return ((target - k_const) / k_lin).max(0.0);
    }
    // #2210 (NIFAL-D3-02) — no attenuation → effectively infinite. This is
    // the OPERATIVE case, not a rare fallback: FNV/FO3 spawnable point
    // lights ship a zero-only attenuation triple (radius control is
    // deferred to the ESM LIGH record instead — see
    // `cell_loader/spawn.rs::spawn_nif_lights`'s ESM-radius preference),
    // so this branch is what 82/82 measured FNV lights actually hit.
    //
    // Pre-fix this returned a bare, uncited `2048.0`. `EXTERIOR_CELL_UNITS`
    // is a genuine citation instead of a nicer-looking guess: it's the
    // spec-defined Bethesda exterior cell size (4096 units on a side,
    // every Gamebryo/Creation title Oblivion→Starfield — see its own doc
    // comment) and `cell_loader/spawn.rs::light_radius_or_default` already
    // uses it as the sibling "we don't know this light's true radius, make
    // it at least visible across a cell" fallback for the exact same
    // semantic gap. Reusing it here (rather than an unrelated magic
    // number) keeps both "no radius info" fallbacks in this engine
    // grounded in the same cited constant instead of two independent
    // guesses that happen to differ.
    byroredux_core::math::coord::EXTERIOR_CELL_UNITS
}

#[cfg(test)]
mod light_dispatch_coverage_tests {
    //! #2532 (NIFAL-D9-04) — the Lights half of the canonical-tier
    //! completeness guard, mirroring `import::collision`'s
    //! `dispatch_coverage_tests`.
    //!
    //! `LightKind` resolution is the same shape as `resolve_shape_inner`: a
    //! `downcast_ref::<…>` chain over block types the parser dispatches. It
    //! has the same failure mode too — a light block that is parse-dispatched
    //! in `blocks/mod.rs` but has no arm here parses for byte correctness and
    //! is then silently dropped, with no warning and no missing-block
    //! diagnostic. One of the six translate-boundary bugs a prior sweep cited
    //! as evidence this guard was needed was in Lights, and it was found by
    //! manual code tracing, not by a test.
    //!
    //! Structural rather than value-based, deliberately: the property that
    //! actually rots is "every dispatched kind reaches the boundary", and a
    //! per-field value harness would not catch a whole type going missing.
    use std::collections::HashSet;

    /// Every `Ni…Light` struct produced by a dispatch arm whose match key is a
    /// quoted `"Ni…Light"`. Mirrors `constructed_shape` in the collision
    /// sibling; kept as its own copy rather than shared because the two scan
    /// different files for different identifier shapes, and a shared helper
    /// parameterised on both would be longer than either.
    fn dispatched_light_structs() -> HashSet<String> {
        let src = include_str!("../../blocks/mod.rs");
        let lines: Vec<&str> = src.lines().collect();
        let mut out = HashSet::new();
        for (i, line) in lines.iter().enumerate() {
            let is_light_arm = line.contains("=>")
                && line
                    .split('"')
                    .any(|tok| tok.starts_with("Ni") && tok.ends_with("Light"));
            if !is_light_arm {
                continue;
            }
            for candidate in &lines[i..=(i + 2).min(lines.len() - 1)] {
                let Some(after) = candidate.split("Box::new(").nth(1) else {
                    continue;
                };
                // Light arms are module-qualified (`light::NiPointLight::parse`)
                // where the collision sibling's shapes are not, so take the LAST
                // path segment before `::parse` rather than the first
                // identifier. Reading only the first would yield `light` and
                // silently produce an empty set — the anti-vacuity assertions
                // below exist because that is exactly what happened first.
                let Some(path) = after.split("::parse").next() else {
                    continue;
                };
                let ident: String = path
                    .rsplit("::")
                    .next()
                    .unwrap_or(path)
                    .chars()
                    .take_while(|c| c.is_alphanumeric() || *c == '_')
                    .collect();
                if ident.starts_with("Ni") && ident.ends_with("Light") {
                    out.insert(ident);
                    break;
                }
            }
        }
        out
    }

    /// Every `Ni…Light` struct with a `downcast_ref::<…>` arm in this module.
    fn resolved_light_structs() -> HashSet<String> {
        let src = include_str!("lights.rs");
        src.split("downcast_ref::<")
            .skip(1)
            .filter_map(|part| {
                let ident: String = part
                    .chars()
                    .take_while(|c| c.is_alphanumeric() || *c == '_')
                    .collect();
                (ident.starts_with("Ni") && ident.ends_with("Light")).then_some(ident)
            })
            .collect()
    }

    #[test]
    fn every_dispatched_light_reaches_the_lightkind_boundary() {
        let dispatched = dispatched_light_structs();
        let resolved = resolved_light_structs();

        // Anti-vacuity, same reasoning as the collision sibling: a reformat
        // that empties either set must fail loudly rather than pass silently.
        assert!(
            dispatched.contains("NiPointLight") && dispatched.contains("NiSpotLight"),
            "dispatch extractor regressed; found {dispatched:?}"
        );
        assert!(
            dispatched.len() >= 4,
            "expected >=4 dispatched Ni*Light structs, found {}: {dispatched:?}",
            dispatched.len()
        );
        assert!(
            resolved.contains("NiPointLight"),
            "resolve extractor regressed; found {resolved:?}"
        );

        let missing: Vec<_> = dispatched.difference(&resolved).cloned().collect();
        assert!(
            missing.is_empty(),
            "these Ni*Light blocks are parse-dispatched but never reach a \
             LightKind arm — the authored light is silently dropped: {missing:?}"
        );
    }
}
