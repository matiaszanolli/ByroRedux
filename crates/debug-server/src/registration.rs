//! Component registration — creates ComponentDescriptor instances for
//! each inspectable component type.

use byroredux_core::ecs::storage::Component;
use byroredux_core::ecs::world::World;
use byroredux_debug_protocol::registry::{ComponentDescriptor, ComponentRegistry};
use serde::de::DeserializeOwned;
use serde::Serialize;

/// Register a component type with the debug registry.
///
/// The component must implement `Serialize + DeserializeOwned` (gated behind
/// the `inspect` feature on byroredux-core) so we can convert to/from JSON.
fn register_component<T>(
    registry: &mut ComponentRegistry,
    name: &'static str,
    field_names: Vec<&'static str>,
) where
    T: Component + Serialize + DeserializeOwned,
{
    let desc = ComponentDescriptor {
        name,
        field_names,
        get_json: Box::new(|world_any: &dyn std::any::Any, entity: u32| {
            let world = world_any.downcast_ref::<World>()?;
            let comp = world.get::<T>(entity)?;
            serde_json::to_value(&*comp).ok()
        }),
        list_entities: Box::new(|world_any: &dyn std::any::Any| {
            let world = match world_any.downcast_ref::<World>() {
                Some(w) => w,
                None => return Vec::new(),
            };
            match world.query::<T>() {
                Some(q) => q.iter().map(|(id, _)| id).collect(),
                None => Vec::new(),
            }
        }),
        get_field: Box::new(|world_any: &dyn std::any::Any, entity: u32, field: &str| {
            let world = world_any.downcast_ref::<World>()?;
            let comp = world.get::<T>(entity)?;
            let full = serde_json::to_value(&*comp).ok()?;
            // Try named field access
            if let Some(val) = full.get(field) {
                return Some(val.clone());
            }
            // For tuple structs serialized as single values, return the whole thing
            // when field is "0" or matches the type
            if field == "0" {
                return Some(full);
            }
            None
        }),
        set_field: Box::new(
            |world_any: &dyn std::any::Any, entity: u32, field: &str, value: serde_json::Value| {
                // For mutation we need to: read → modify JSON → deserialize back → write.
                // This requires &mut World, but the exclusive system has &World.
                // We'll need to use query_mut which takes &self on World (interior mutability).
                let world = world_any
                    .downcast_ref::<World>()
                    .ok_or_else(|| "internal error: world downcast failed".to_string())?;

                let mut q = world
                    .query_mut::<T>()
                    .ok_or_else(|| "no storage for component".to_string())?;

                let comp = q
                    .get_mut(entity)
                    .ok_or_else(|| format!("entity {} has no component", entity))?;

                // Serialize current state, modify the field, deserialize back.
                let mut json =
                    serde_json::to_value(&*comp).map_err(|e| format!("serialize error: {}", e))?;

                // Handle named fields on objects
                if let serde_json::Value::Object(ref mut map) = json {
                    if map.contains_key(field) {
                        map.insert(field.to_string(), value);
                    } else {
                        return Err(format!("no field '{}' on component", field));
                    }
                } else if field == "0" {
                    // Tuple struct — replace the whole value
                    json = value;
                } else {
                    return Err("component is not a struct with named fields".to_string());
                }

                // Deserialize back and overwrite
                let new_comp: T = serde_json::from_value(json)
                    .map_err(|e| format!("deserialize error: {}", e))?;
                *comp = new_comp;

                Ok(())
            },
        ),
    };
    registry.insert(desc);
}

/// `FactionReputation` is inspectable state but is not part of the save
/// serialization surface. Expose a read-only JSON projection without adding
/// serde derives to the saved component model.
fn register_faction_reputation(registry: &mut ComponentRegistry) {
    use byroredux_core::character::FactionReputation;

    registry.insert(ComponentDescriptor {
        name: "FactionReputation",
        field_names: vec!["entries"],
        get_json: Box::new(|world_any, entity| {
            let world = world_any.downcast_ref::<World>()?;
            let component = world.get::<FactionReputation>(entity)?;
            Some(serde_json::json!({
                "entries": component.entries.iter().map(|entry| serde_json::json!({
                    "repu_form_id": entry.repu_form_id,
                    "fame": entry.fame,
                    "infamy": entry.infamy,
                })).collect::<Vec<_>>()
            }))
        }),
        list_entities: Box::new(|world_any| {
            let Some(world) = world_any.downcast_ref::<World>() else {
                return Vec::new();
            };
            world
                .query::<FactionReputation>()
                .map(|query| query.iter().map(|(entity, _)| entity).collect())
                .unwrap_or_default()
        }),
        get_field: Box::new(|world_any, entity, field| {
            if field != "entries" && field != "0" {
                return None;
            }
            let world = world_any.downcast_ref::<World>()?;
            let component = world.get::<FactionReputation>(entity)?;
            Some(serde_json::json!({
                "entries": component.entries.iter().map(|entry| serde_json::json!({
                    "repu_form_id": entry.repu_form_id,
                    "fame": entry.fame,
                    "infamy": entry.infamy,
                })).collect::<Vec<_>>()
            }))
        }),
        set_field: Box::new(|_, _, _, _| Err("FactionReputation is read-only".into())),
    });
}

/// Register all inspectable components with the registry.
pub fn register_all(registry: &mut ComponentRegistry) {
    use byroredux_core::animation::{AnimationPlayer, AnimationStack};
    use byroredux_core::character::Perks;
    use byroredux_core::ecs::components::*;
    use byroredux_core::ecs::components::water::WaterCurrentVolume;

    register_component::<Transform>(
        registry,
        "Transform",
        vec!["translation", "rotation", "scale"],
    );
    register_component::<GlobalTransform>(
        registry,
        "GlobalTransform",
        vec!["translation", "rotation", "scale"],
    );
    register_component::<Camera>(registry, "Camera", vec!["fov_y", "near", "far", "aspect"]);
    register_component::<LightSource>(registry, "LightSource", vec!["radius", "color", "flags"]);
    register_component::<Material>(
        registry,
        "Material",
        vec![
            "emissive_color",
            "emissive_mult",
            "specular_color",
            "specular_strength",
            "glossiness",
            "uv_offset",
            "uv_scale",
            "alpha",
            "env_map_scale",
            "normal_map",
            "texture_path",
            "glow_map",
            "detail_map",
            "gloss_map",
            "dark_map",
            "vertex_color_mode",
            "alpha_test",
            "alpha_threshold",
            "alpha_test_func",
        ],
    );
    register_component::<LocalBound>(registry, "LocalBound", vec!["center", "radius"]);
    register_component::<WorldBound>(registry, "WorldBound", vec!["center", "radius"]);
    register_component::<FogVolume>(
        registry,
        "FogVolume",
        vec![
            "bounds",
            "extinction_per_meter",
            "single_scatter_albedo",
            "edge_softness",
            "source",
        ],
    );
    register_component::<Billboard>(registry, "Billboard", vec!["mode"]);
    register_component::<MeshHandle>(registry, "MeshHandle", vec!["0"]);
    register_component::<TextureHandle>(registry, "TextureHandle", vec!["0"]);
    register_component::<BSXFlags>(registry, "BSXFlags", vec!["0"]);
    register_component::<BSBound>(registry, "BSBound", vec!["center", "half_extents"]);
    // M41.5 Phase B — furniture sit/sleep/lean markers, so `byro-dbg`'s
    // `entities Furniture` surfaces seatable furniture in a loaded cell.
    register_component::<Furniture>(registry, "Furniture", vec!["markers"]);
    // M42 — Sandbox AI markers: `entities SandboxBehavior` lists actors
    // running the idle-in-area procedure; `entities Seated` lists those
    // the seat system has placed in furniture.
    register_component::<SandboxBehavior>(registry, "SandboxBehavior", vec![]);
    register_component::<Seated>(registry, "Seated", vec!["furniture"]);
    // #5495 — creature-kind marker consulted by both M42 seating paths.
    register_component::<CreatureActor>(registry, "CreatureActor", vec![]);
    // #4063 — the other six M42 procedure runtimes. Sandbox landed with
    // M42 and got a registration above; M42.3-M42.8 each added a
    // Behavior/State pair and none extended this file, so `byro-dbg` could
    // inspect a *seated* NPC and nothing else. These six are the ones with
    // live per-tick state — Wander/Patrol's oscillation phase, Travel's
    // frozen destination, Follow/Escort's re-resolved target, Guard's
    // anchor and leash — which is exactly what an operator needs when an
    // actor misbehaves, and exactly what the env-var gates
    // (BYRO_WANDER / BYRO_TRAVEL / BYRO_FOLLOW / BYRO_ESCORT / BYRO_GUARD /
    // BYRO_PATROL) exist to be debugged through. Kept in roster order so
    // this list reads against `clear_ambient_behavior`'s teardown, which is
    // the other place the same seven-procedure set is enumerated.
    register_component::<WanderBehavior>(
        registry,
        "WanderBehavior",
        vec!["wander_radius", "form_id"],
    );
    register_component::<WanderState>(
        registry,
        "WanderState",
        vec!["home", "target", "phase", "pick_count"],
    );
    register_component::<TravelBehavior>(
        registry,
        "TravelBehavior",
        vec!["radius", "target_form_id", "form_id"],
    );
    register_component::<TravelState>(registry, "TravelState", vec!["destination"]);
    register_component::<Traveled>(registry, "Traveled", vec![]);
    register_component::<FollowBehavior>(
        registry,
        "FollowBehavior",
        vec!["target_form_id", "follow_distance"],
    );
    register_component::<FollowState>(registry, "FollowState", vec!["target_entity"]);
    register_component::<EscortBehavior>(
        registry,
        "EscortBehavior",
        vec![
            "target_form_id",
            "destination_form_id",
            "destination_radius",
            "collect_distance",
            "form_id",
        ],
    );
    register_component::<EscortState>(
        registry,
        "EscortState",
        vec!["target_entity", "destination"],
    );
    register_component::<Escorted>(registry, "Escorted", vec![]);
    register_component::<GuardBehavior>(
        registry,
        "GuardBehavior",
        vec!["anchor_form_id", "radius", "form_id"],
    );
    register_component::<GuardState>(registry, "GuardState", vec!["anchor"]);
    register_component::<PatrolBehavior>(
        registry,
        "PatrolBehavior",
        vec!["patrol_radius", "form_id"],
    );
    register_component::<PatrolState>(
        registry,
        "PatrolState",
        vec!["home", "target", "phase", "pick_count"],
    );
    // M42 — Eat/Sleep (procedures 3/4): the behavior pair shares one
    // walk-then-seat runtime; `EatSleepState` carries the one-shot walk
    // destination an operator needs when an actor circles instead of
    // dining.
    register_component::<EatBehavior>(registry, "EatBehavior", vec!["radius", "location", "form_id"]);
    register_component::<SleepBehavior>(
        registry,
        "SleepBehavior",
        vec!["radius", "location", "form_id"],
    );
    register_component::<EatSleepState>(registry, "EatSleepState", vec!["destination"]);
    // #5391 — the authored placement a "near editor location" Eat/Sleep
    // package anchors on.
    register_component::<EditorPlacement>(registry, "EditorPlacement", vec!["translation"]);
    register_component::<AnimatedVisibility>(registry, "AnimatedVisibility", vec!["0"]);
    register_component::<AnimatedAlpha>(registry, "AnimatedAlpha", vec!["0"]);
    // Post-#517 split: five target-specific color components replaced
    // the single `AnimatedColor` slot.
    register_component::<AnimatedDiffuseColor>(registry, "AnimatedDiffuseColor", vec!["0"]);
    register_component::<AnimatedAmbientColor>(registry, "AnimatedAmbientColor", vec!["0"]);
    register_component::<AnimatedSpecularColor>(registry, "AnimatedSpecularColor", vec!["0"]);
    register_component::<AnimatedEmissiveColor>(registry, "AnimatedEmissiveColor", vec!["0"]);
    register_component::<AnimatedShaderColor>(registry, "AnimatedShaderColor", vec!["0"]);
    // Live UV scroll state: the observable proof that a shape's authored
    // U/V offset controller (river whitewater, waterfall sheets) is running.
    register_component::<AnimatedUvTransform>(
        registry,
        "AnimatedUvTransform",
        vec!["offset", "scale", "rotation"],
    );

    // Animation playback state (#486) — debug snapshots must capture
    // `reverse_direction` (ping-pong flip latch for `CycleType::Reverse`)
    // plus the blend-in/out timers on each layer, otherwise reloading
    // a snapshot mid-pingpong resets the direction and the animation
    // steps backward across the boundary.
    register_component::<AnimationPlayer>(
        registry,
        "AnimationPlayer",
        vec![
            "clip_handle",
            "local_time",
            "playing",
            "speed",
            "reverse_direction",
            "root_entity",
            "prev_time",
        ],
    );
    register_component::<AnimationStack>(registry, "AnimationStack", vec!["layers", "root_entity"]);

    // M41 Phase 2 equip slice (#896 / be4663b). Surfaces NPC inventory
    // contents + biped-slot occupants to byro-dbg so the smoke-test
    // workflow (`cargo run -- … --bench-frames N --bench-hold` then
    // `cargo run -p byro-dbg`) can verify visible-content equipping —
    // `find Inventory` lights up every actor with a populated outfit,
    // `find EquipmentSlots` shows the biped bitmask coverage. Without
    // this, the M41 close-out QA is structural-only (counts entities
    // but can't introspect the equip state). See M41 Phase 2 close-out
    // and `docs/smoke-tests/m41-equip.sh`.
    register_component::<Inventory>(registry, "Inventory", vec!["items"]);
    register_component::<EquipmentSlots>(registry, "EquipmentSlots", vec!["occupants"]);
    register_component::<ActorValues>(registry, "ActorValues", vec!["values"]);
    register_component::<ActorVitals>(registry, "ActorVitals", vec!["health"]);
    register_component::<EquippedWeapon>(
        registry,
        "EquippedWeapon",
        vec!["inventory_index", "base_form_id", "damage", "reach", "speed"],
    );
    register_component::<Dead>(registry, "Dead", vec![]);
    register_component::<CreatureAttack>(registry, "CreatureAttack", vec!["damage"]);
    register_component::<Perks>(registry, "Perks", vec!["entries"]);
    register_faction_reputation(registry);

    // #4755 — the remainder of the inspect-carrying roster. Every core
    // component with the `inspect` serde derive is now registered; the
    // `every_inspectable_component_is_registered` roster test below fails
    // when a new one lands without a registration, so this block only
    // grows. Families kept together for scan-ability.
    register_component::<Name>(registry, "Name", vec!["0"]);
    register_component::<Parent>(registry, "Parent", vec!["0"]);
    register_component::<Children>(registry, "Children", vec!["0"]);
    // Animation-channel remainder — the visibility/alpha/color siblings
    // above predate these three.
    register_component::<AnimatedMorphWeights>(registry, "AnimatedMorphWeights", vec!["0"]);
    register_component::<AnimatedShaderFloat>(registry, "AnimatedShaderFloat", vec!["0"]);
    register_component::<AnimatedTextureFlip>(registry, "AnimatedTextureFlip", vec!["0"]);
    // `LightSource`'s flicker counterpart (LIGH FNAM decode).
    register_component::<LightFlicker>(
        registry,
        "LightFlicker",
        vec![
            "animation_flags",
            "period_secs",
            "intensity_amplitude",
            "movement_amplitude",
            "base_translation",
            "phase_offset_secs",
        ],
    );
    // Physics body + the water family (WATAL debugging: plane authoring,
    // flow vectors, per-frame submersion/contact state).
    register_component::<RigidBodyData>(
        registry,
        "RigidBodyData",
        vec![
            "motion_type",
            "mass",
            "friction",
            "restitution",
            "linear_damping",
            "angular_damping",
            "collidable",
        ],
    );
    register_component::<WaterPlane>(
        registry,
        "WaterPlane",
        vec!["kind", "material", "damage_per_second"],
    );
    register_component::<WaterFlow>(registry, "WaterFlow", vec!["direction", "speed"]);
    register_component::<WaterVolume>(registry, "WaterVolume", vec!["min", "max"]);
    register_component::<WaterCurrentVolume>(registry, "WaterCurrentVolume", vec!["volume", "flow"]);
    register_component::<SubmersionState>(
        registry,
        "SubmersionState",
        vec!["depth", "head_submerged", "surface_entity"],
    );
    register_component::<WaterContact>(
        registry,
        "WaterContact",
        vec![
            "surface_entity",
            "depth",
            "submerged_fraction",
            "head_submerged",
            "flow",
            "damage_per_second",
        ],
    );
    register_component::<CombustionState>(
        registry,
        "CombustionState",
        vec!["start_time_seconds", "lifetime_seconds"],
    );
    register_component::<SpeedTreeWind>(registry, "SpeedTreeWind", vec!["response", "stiffness"]);
    // Unit enum — serializes as its variant name, no named fields.
    register_component::<RenderLayer>(registry, "RenderLayer", vec![]);
    register_component::<TimedRestorations>(registry, "TimedRestorations", vec!["effects"]);
}

/// #4063 — the roster pin.
///
/// The AI-procedure components are enumerated in three places: the
/// `AmbientBehavior` match that attaches one at spawn, the
/// `clear_ambient_behavior` teardown that removes them all, and this
/// registry. The first two are complete; this one silently lagged six
/// procedures behind for five milestones because nothing tied it to the
/// others. Rather than pin a count (which moves with every component
/// added), derive the roster from the source of truth — the `impl
/// Component for <X>Behavior` declarations — so an eighth procedure fails
/// this test instead of landing half-wired.
#[cfg(test)]
mod roster_tests {
    /// Every `*Behavior` AI-procedure component declared in
    /// `crates/core/src/ecs/components/` must be registered above.
    ///
    /// Scans only the production half of this file: `register_component`
    /// calls written inside a test would otherwise satisfy the scan
    /// themselves, and the needle is composed at runtime for the same
    /// reason — spelling it literally here would make this module its own
    /// evidence.
    #[test]
    fn every_ai_procedure_behavior_component_is_registered() {
        let registrations = include_str!("registration.rs")
            .split_once("#[cfg(test)]")
            .expect("this file has a test module")
            .0;

        let components_dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../core/src/ecs/components");
        let mut behaviors: Vec<String> = Vec::new();
        for entry in std::fs::read_dir(components_dir).expect("components dir is readable") {
            let path = entry.expect("readable dir entry").path();
            if path.extension().and_then(|e| e.to_str()) != Some("rs") {
                continue;
            }
            let src = std::fs::read_to_string(&path).expect("readable component file");
            for line in src.lines() {
                let Some(rest) = line.strip_prefix("impl Component for ") else {
                    continue;
                };
                let name = rest.trim_end_matches(" {");
                if name.ends_with("Behavior") {
                    behaviors.push(name.to_owned());
                }
            }
        }
        behaviors.sort();

        assert!(
            behaviors.len() >= 7,
            "the scan found only {behaviors:?} — the extraction broke, not the registry"
        );

        let missing: Vec<&String> = behaviors
            .iter()
            .filter(|name| {
                let needle = format!("{}{}{}>", "register_component", "::<", name);
                !registrations.contains(&needle)
            })
            .collect();
        assert!(
            missing.is_empty(),
            "{missing:?} are AI-procedure components with no debug registration — \
             `byro-dbg` cannot inspect an actor running them (#4063). Add a \
             `register_component` call beside the others, and register the \
             matching `*State` marker too."
        );
    }

    #[test]
    fn requested_gameplay_components_are_registered() {
        let registrations = include_str!("registration.rs")
            .split_once("#[cfg(test)]")
            .expect("this file has a test module")
            .0;
        let requested = [
            "ActorValues",
            "ActorVitals",
            "CreatureAttack",
            "Dead",
            "EquippedWeapon",
            "EquipmentSlots",
            "Inventory",
            "Perks",
        ];
        let mut missing: Vec<_> = requested
            .iter()
            .filter(|name| {
                let needle = format!("{}{}{}>", "register_component", "::<", name);
                !registrations.contains(&needle)
            })
            .copied()
            .collect();
        if !registrations.contains("register_faction_reputation(registry);") {
            missing.push("FactionReputation");
        }
        assert!(
            missing.is_empty(),
            "gameplay components required by #4755 are not registered: {missing:?}"
        );
    }

    /// #4755 — the widened roster pin.
    ///
    /// The `*Behavior` guard above catches one family; this one closes
    /// the class that gap came from. Every core component carrying the
    /// `inspect` serde derive — i.e. every type that *can* go through
    /// `register_component` — must actually be registered, so the next
    /// inspectable component either lands with a debug registration or
    /// fails this test instead of silently half-wiring. `FactionReputation`
    /// is the one deliberate outsider: it keeps the inspect derive off
    /// (not part of the save surface) and registers through the
    /// hand-rolled read-only descriptor above, pinned separately by
    /// `requested_gameplay_components_are_registered`.
    #[test]
    fn every_inspectable_component_is_registered() {
        let registrations = include_str!("registration.rs")
            .split_once("#[cfg(test)]")
            .expect("this file has a test module")
            .0;

        // Recursive walk: inspect-carrying components live in
        // `ecs/components/`, `character/`, and `animation/`, and the
        // next one may add a fourth home — walk all of core/src.
        let core_src = std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../core/src"));
        let mut stack = vec![core_src.to_path_buf()];
        let mut inspectable: Vec<String> = Vec::new();
        while let Some(dir) = stack.pop() {
            for entry in std::fs::read_dir(&dir).expect("core src is readable") {
                let path = entry.expect("readable dir entry").path();
                if path.is_dir() {
                    stack.push(path);
                    continue;
                }
                if path.extension().and_then(|e| e.to_str()) != Some("rs") {
                    continue;
                }
                let src = std::fs::read_to_string(&path).expect("readable core file");
                let lines: Vec<&str> = src.lines().collect();

                let component_impls: Vec<String> = lines
                    .iter()
                    .filter_map(|line| {
                        line.strip_prefix("impl Component for ")
                            .map(|rest| rest.trim_end_matches(" {").to_string())
                    })
                    .collect();

                for (i, line) in lines.iter().enumerate() {
                    let trimmed = line.trim_start();
                    let Some(rest) = ["pub struct ", "pub enum ", "struct ", "enum "]
                        .iter()
                        .find_map(|prefix| trimmed.strip_prefix(prefix))
                    else {
                        continue;
                    };
                    let name: String = rest
                        .chars()
                        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                        .collect();
                    // The inspect derive sits in the attribute block
                    // directly above the declaration; doc comments and
                    // other derives may interleave within that window.
                    let window = lines[i.saturating_sub(4)..i].join("\n");
                    let marker = format!("cfg_attr{}", "(feature = \"inspect\"");
                    if name.is_empty()
                        || !window.contains(&marker)
                        || !component_impls.contains(&name)
                    {
                        continue;
                    }
                    inspectable.push(name);
                }
            }
        }
        inspectable.sort();

        assert!(
            inspectable.len() >= 60,
            "the scan found only {} ({inspectable:?}) — the extraction broke, not the registry",
            inspectable.len()
        );

        let missing: Vec<&String> = inspectable
            .iter()
            .filter(|name| {
                let needle = format!("{}{}{}>", "register_component", "::<", name);
                !registrations.contains(&needle)
            })
            .collect();
        assert!(
            missing.is_empty(),
            "{missing:?} carry the `inspect` derive but have no debug registration — \
             `byro-dbg` cannot inspect them (#4755). Add a `register_component` \
             call beside the others, or, if the type must stay unregistered, \
             document why beside it and extend this test's exclusion."
        );
    }

    #[test]
    fn debug_cli_component_counts_match_the_registry() {
        let production = include_str!("registration.rs")
            .split_once("#[cfg(test)]")
            .expect("this file has a test module")
            .0;
        let count = production.matches("register_component::<").count()
            + production.matches("register_faction_reputation(registry);").count();
        let docs = include_str!("../../../docs/engine/debug-cli.md");
        assert!(
            docs.contains(&format!("Currently registered ({count} components)")),
            "debug-cli.md component heading must report {count}"
        );
        // #5417 — the doc carries the count TWICE (the heading and the
        // `byro> components` example transcript near the end). A bare
        // `contains` passed vacuously off the heading while the example
        // still said 67, so require the second occurrence explicitly.
        let needle = format!("({count} components)");
        assert!(
            docs.matches(&needle).count() >= 2,
            "debug-cli.md must report {count} in the example transcript too — \
             found the heading only"
        );
    }
}
