//! Cross-command formatting helpers and the shared import prelude.
//!
//! Every command submodule pulls this in via `use super::shared::*`,
//! which re-exports the external types the commands reference plus the
//! formatting helpers shared across command groups (#1323 / TD9-NEW-03).

pub(crate) use crate::components::{
    AlphaBlend, DoorTeleport, InputState, IsFxMesh, TwoSided,
};
pub(crate) use crate::helpers::world_resource_set;
pub(crate) use byroredux_core::console::{CommandOutput, CommandRegistry, ConsoleCommand};
pub(crate) use byroredux_core::ecs::components::{
    CollisionShape, FormIdComponent, RenderLayer, RigidBodyData,
};
pub(crate) use byroredux_core::ecs::{
    AccessConflict, ActiveCamera, Camera, ConflictKind, DebugStats, EntityId, GlobalTransform,
    LightSource, Material, MeshHandle, Name, Parent, ParticleEmitter, SceneFlags,
    SchedulerAccessReport, ScratchTelemetry, SelectedRef, SkinCoverageStats, SkinnedMesh,
    TextureHandle, Transform, World, WorldBound,
};
pub(crate) use byroredux_core::math::{Mat4, Quat, Vec3};
pub(crate) use byroredux_core::string::StringPool;
pub(crate) use std::collections::HashMap;

pub(crate) use crate::cell_loader::{
    LoadedCellIndex, LoadedPluginSet, PendingCellTransition, PendingCellTransitionSlot,
    TransitionDestination,
};
pub(crate) use byroredux_core::ecs::SystemList;

// ── Shared formatting / lookup helpers ────────────────────

/// Resolve an entity's `Name` to a printable String via the
/// `StringPool` resource. Returns `None` when the entity has no
/// `Name`, when the pool isn't installed, or when the symbol doesn't
/// resolve (which would itself be a string-pool integrity bug).
pub(crate) fn resolve_entity_name(world: &World, entity: EntityId) -> Option<String> {
    let name_q = world.query::<Name>()?;
    let name = name_q.get(entity)?;
    let pool = world.try_resource::<StringPool>()?;
    pool.resolve(name.0).map(|s| s.to_string())
}
/// Derive fly-camera `(yaw, pitch)` in radians for a camera at `from`
/// to look at `to`. Matches `fly_camera_system`'s rotation composition
/// (`Quat::from_rotation_y(yaw) * Quat::from_rotation_x(pitch)` with
/// `forward = rotation * -Z`), so updating `InputState.{yaw, pitch}`
/// alongside `Transform.rotation` survives the next fly-camera tick.
/// Degenerate `to == from` returns `(0, 0)`.
pub(crate) fn look_at_yaw_pitch(from: Vec3, to: Vec3) -> (f32, f32) {
    let diff = to - from;
    let len_sq = diff.length_squared();
    if len_sq < 1e-6 {
        return (0.0, 0.0);
    }
    let dir = diff / len_sq.sqrt();
    let pitch = dir.y.clamp(-1.0, 1.0).asin();
    let yaw = (-dir.x).atan2(-dir.z);
    (yaw, pitch)
}
/// Strip the leading module path off a `std::any::type_name` so report
/// lines stay readable on narrow terminals.
pub(crate) fn short(name: &str) -> &str {
    name.rsplit("::").next().unwrap_or(name)
}

/// Resolve a console entity token: `.` → the current [`SelectedRef`] selection
/// (set by `prid`), otherwise a decimal [`EntityId`]. Shared by the
/// actor-state commands (`cond`, `setav`, `modav`).
pub(crate) fn resolve_console_entity(world: &World, tok: &str) -> Result<EntityId, String> {
    if tok == "." {
        return match world.try_resource::<SelectedRef>() {
            Some(sel) => sel
                .0
                .ok_or_else(|| "no selection — `prid <id>` first".to_string()),
            None => Err("SelectedRef resource not present".to_string()),
        };
    }
    tok.parse::<EntityId>()
        .map_err(|_| format!("bad entity `{tok}` (decimal id or `.`)"))
}

/// Parse a decimal or `0x`-hex `u32` console argument (FormIDs are hex).
pub(crate) fn parse_console_u32(tok: &str) -> Option<u32> {
    if let Some(hex) = tok.strip_prefix("0x").or_else(|| tok.strip_prefix("0X")) {
        u32::from_str_radix(hex, 16).ok()
    } else {
        tok.parse::<u32>().ok()
    }
}
/// Pure formatter — kept separate from the command impl so the test
/// can drive it without standing up a `ConsoleCommand` dispatcher.
pub(crate) fn format_skin_dump(world: &World, entity: u32, skin: &SkinnedMesh) -> Vec<String> {
    let mut lines = vec![format!(
        "SkinnedMesh dump for entity {} ({} bones):",
        entity,
        skin.bones.len()
    )];
    if let Some(root) = skin.skeleton_root {
        lines.push(format!("  skeleton_root: entity {}", root));
    } else {
        lines.push("  skeleton_root: (none)".to_string());
    }
    if skin.global_skin_transform != Mat4::IDENTITY {
        lines.push(
            "  global_skin_transform: NON-IDENTITY (informational; not multiplied at runtime)"
                .to_string(),
        );
        lines.push(format!(
            "    {}",
            format_mat4_row(&skin.global_skin_transform)
        ));
    } else {
        lines.push("  global_skin_transform: identity".to_string());
    }
    lines.push(String::new());
    lines.push(format!(
        "  {:>4} {:>10} {:<24} {:<11} {:<11} {:<11}",
        "slot", "entity", "name", "world(T)", "bind_inv(T)", "palette(T)"
    ));
    for (i, (maybe_bone, bind_inv)) in skin.bones.iter().zip(skin.bind_inverses.iter()).enumerate()
    {
        let (entity_str, name_str, world_mat) = match maybe_bone {
            Some(bone_e) => {
                // `get::<Name>` fully releases its internal lock before
                // returning, so acquiring `StringPool` only afterward
                // (rather than holding it across the call) never presents
                // the lock-order detector with a Pool-while-holding-Name
                // edge — matching `resolve_entity_name`'s Name-before-Pool
                // convention for this pair (#313).
                let name = world
                    .get::<Name>(*bone_e)
                    .and_then(|n| {
                        world
                            .try_resource::<StringPool>()
                            .and_then(|p| p.resolve(n.0).map(|s| s.to_string()))
                    })
                    .unwrap_or_else(|| "(no Name)".to_string());
                let world_mat = world
                    .get::<GlobalTransform>(*bone_e)
                    .map(|gt| gt.to_matrix());
                (format!("{}", bone_e), name, world_mat)
            }
            None => ("(None)".to_string(), "(unresolved)".to_string(), None),
        };
        let world_t = world_mat
            .map(|m| format_translation(&m))
            .unwrap_or_else(|| "(no GT)".to_string());
        let bind_t = format_translation(bind_inv);
        let palette = world_mat.map(|w| w * *bind_inv).unwrap_or(Mat4::IDENTITY);
        let pal_t = format_translation(&palette);
        lines.push(format!(
            "  {:>4} {:>10} {:<24} {:<11} {:<11} {:<11}",
            i,
            entity_str,
            truncate(&name_str, 24),
            world_t,
            bind_t,
            pal_t
        ));
        // Continuation lines: full matrices (one row of `world`,
        // `bind_inverse`, `palette`). Operators copy these into a
        // diff against `skinning_e2e`'s working baseline to find
        // the diverging slot per the #841 plan.
        if let Some(w) = world_mat {
            lines.push(format!("       world:   {}", format_mat4_row(&w)));
        }
        lines.push(format!("       bind_inv:{}", format_mat4_row(bind_inv)));
        lines.push(format!("       palette: {}", format_mat4_row(&palette)));
    }
    lines
}

pub(crate) fn format_translation(m: &Mat4) -> String {
    let t = m.w_axis;
    format!("({:.2},{:.2},{:.2})", t.x, t.y, t.z)
}

pub(crate) fn format_mat4_row(m: &Mat4) -> String {
    // Print matrix in row-major order on one line for grep/diff
    // friendliness. Column-vector convention — column N is
    // m.{x,y,z,w}_axis.
    let c = m.to_cols_array();
    format!(
        "[{:>7.3} {:>7.3} {:>7.3} {:>7.3} | {:>7.3} {:>7.3} {:>7.3} {:>7.3} | {:>7.3} {:>7.3} {:>7.3} {:>7.3} | {:>7.3} {:>7.3} {:>7.3} {:>7.3}]",
        c[0], c[4], c[8], c[12],
        c[1], c[5], c[9], c[13],
        c[2], c[6], c[10], c[14],
        c[3], c[7], c[11], c[15],
    )
}

pub(crate) fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}…", &s[..max - 1])
    }
}

pub(crate) fn fmt_opt_f32(v: Option<f32>) -> String {
    match v {
        Some(x) => format!("{:.3}", x),
        None => "None".to_string(),
    }
}

pub(crate) fn fmt_opt_rgb(v: Option<[f32; 3]>) -> String {
    match v {
        Some(c) => format!("[{:.3}, {:.3}, {:.3}]", c[0], c[1], c[2]),
        None => "None".to_string(),
    }
}
