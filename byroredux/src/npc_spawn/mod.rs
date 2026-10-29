//! NPC spawning — assemble a placed NPC actor entity from its NPC_,
//! RACE, HDPT/EYES/HAIR, and FaceGen content.
//!
//! M41.0 lands the spawn function itself; this Phase 0 file ships the
//! game-variant path helpers that the spawn function will consume.
//! Each helper maps (game, gender) → a vanilla archive path string for
//! the per-game content layout.

use std::collections::HashMap;
use std::sync::Arc;

use byroredux_core::animation::AnimationClipRegistry;
use byroredux_core::animation::AnimationPlayer;
use byroredux_core::ecs::components::{

    EquipmentSlots, EquippedWeapon, FactionRanks, GlobalTransform, Inventory, InventoryIndex,
    ItemStack, MotionType, Name, Parent, RigidBodyData, Transform,
};
use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::World;
use byroredux_core::math::{Quat, Vec3};
use byroredux_core::string::StringPool;
use byroredux_plugin::esm::reader::GameKind;
use byroredux_plugin::esm::records::{EsmIndex, ItemKind, NpcRecord, RaceRecord};
use byroredux_renderer::VulkanContext;

/// AI-package behavior tagging — split out under #2198. See the module's own
/// docs for why it lives beside this file rather than in it.
pub(crate) mod ai_package;
#[cfg(test)]
use ai_package::apply_ai_package_behavior;
pub(crate) use ai_package::{ambient_ai_package_system, suspend_ambient_behavior_for_combat};
pub(crate) mod loot_appearance;
mod resumable;
mod seam_blend;
pub(crate) use resumable::{NpcSpawnJob, NpcSpawnProgress};

use crate::anim_convert::convert_nif_clip;
use crate::asset_provider::{MaterialProvider, TextureProvider};
use crate::helpers::add_child;
use crate::scene::{load_nif_bytes_with_skeleton, load_nif_extracting_with_skeleton};

use byroredux_plugin::equip::{Gender, ResolvedNpc};

use crate::asset_provider::normalize_mesh_path;

mod anim_clips;
mod equip;
mod fallback_physics;
mod skeleton_paths;
mod stamps;

pub(crate) use anim_clips::*;
pub(crate) use equip::*;
pub(crate) use fallback_physics::*;
pub(crate) use skeleton_paths::*;
pub(crate) use stamps::*;

/// The actor's effective level for anything that treats level as a number —
/// re-exported from the plugin crate, where it lives beside the `NPC_` record
/// whose overloaded `level` field it decodes.
///
/// #3171 — this used to be defined here, and both copies made of it drifted to
/// `.max(1)` on the non-multiplier branch (see the definition's docs). It now
/// sits at the ESM-record boundary so the binary imports the rule instead of
/// re-deriving it.
pub(crate) use byroredux_plugin::esm::records::effective_actor_level;

/// Every production file of this module, for the source-shape scans that used to `include_str!` the
/// single `npc_spawn.rs` (#6357). `tests.rs` and the sibling families with their own scans are not in it.
#[cfg(test)]
pub(crate) const SOURCES: &str = concat!(
    include_str!("mod.rs"),
    include_str!("stamps.rs"),
    include_str!("fallback_physics.rs"),
    include_str!("skeleton_paths.rs"),
    include_str!("anim_clips.rs"),
    include_str!("equip.rs"),
);

/// Walk the subtree rooted at `root` and tag every descendant entity
/// carrying a [`MeshHandle`] with [`RenderLayer::Actor`]. Loose-NIF
/// spawns at `scene::load_nif_bytes` default each mesh entity to
/// `RenderLayer::Architecture` (no REFR base record available), so
/// every NPC body / head / armor / FaceGen mesh comes out of that path
/// with the wrong layer for depth-bias purposes — without this
/// override every standing NPC z-fights the floor at the foot-plant
/// patch. Called from each success path of [`NpcSpawnJob::advance`]
/// (`npc_spawn/resumable/{runtime,prebaked}.rs`) before yielding a completed placement
/// root, for both the kf-era runtime recipe and the pre-baked-FaceGen
/// recipe. BFS over `Children`, mirrors
/// [`crate::anim_convert::build_subtree_name_map`]'s walk shape.
pub(crate) fn tag_descendants_as_actor(world: &mut World, root: EntityId) {
    use byroredux_core::ecs::components::RenderLayer;
    use byroredux_core::ecs::MeshHandle;

    // Collect first (read locks), mutate after (write locks). The
    // ECS API forbids holding read + write guards simultaneously.
    //
    // #6105 — the subtree comes from the shared cycle-safe collector (visited
    // set + traversal budget, like every other whole-subtree walk since #5513 /
    // #5713); a cyclic or duplicated `Children` edge used to spin this loop with
    // an unbounded queue. The `MeshHandle` guard is taken after it, so the
    // `Children` -> `MeshHandle` order is the collector's, not hand-held here.
    let subtree = byroredux_core::ecs::collect_subtree(world, root);
    let to_tag: Vec<EntityId> = match world.query::<MeshHandle>() {
        Some(mesh_q) => subtree.into_iter().filter(|&e| mesh_q.get(e).is_some()).collect(),
        None => Vec::new(),
    };
    for e in to_tag {
        world.insert(e, RenderLayer::Actor);
    }
}

#[cfg(test)]
mod tests;
