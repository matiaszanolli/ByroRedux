//! Expression evaluator — walks a Papyrus AST against the World.
//!
//! Translates debug expressions into ECS queries using the component
//! registry for type-erased access.

use byroredux_core::console::CommandRegistry;
use byroredux_core::ecs::components::{Material, Name, TextureHandle};
use byroredux_core::ecs::resources::DebugStats;
use byroredux_core::ecs::world::World;
use byroredux_core::string::StringPool;
use byroredux_debug_protocol::registry::ComponentRegistry;
use byroredux_debug_protocol::{DebugRequest, DebugResponse, EntityInfo};
use std::collections::HashMap;

use byroredux_papyrus::ast::{CallArg, Expr, Identifier};

use byroredux_core::ecs::resources::SystemList;

/// Evaluate a debug request against the world.
pub fn evaluate(
    world: &World,
    registry: &ComponentRegistry,
    request: &DebugRequest,
) -> DebugResponse {
    match request {
        DebugRequest::Ping => DebugResponse::Pong,

        DebugRequest::Stats => eval_stats(world),

        DebugRequest::ListComponents => DebugResponse::ComponentList {
            components: registry.names().into_iter().map(String::from).collect(),
        },

        DebugRequest::ListSystems => {
            // SystemList is stored as a resource by the engine binary.
            match world.try_resource::<SystemList>() {
                Some(list) => DebugResponse::SystemList {
                    systems: list.0.clone(),
                },
                None => DebugResponse::SystemList {
                    systems: vec!["(system list unavailable)".to_string()],
                },
            }
        }

        DebugRequest::FindEntity { name } => eval_find(world, name),

        DebugRequest::ListEntities { component } => {
            eval_list_entities(world, registry, component.as_deref())
        }

        DebugRequest::GetComponent { entity, component } => {
            eval_get_component(world, registry, *entity, component)
        }

        DebugRequest::SetField {
            entity,
            component,
            path,
            value,
        } => eval_set_field(world, registry, *entity, component, path, value.clone()),

        // Screenshot is handled by the drain system directly (spans multiple frames).
        DebugRequest::Screenshot { .. } => {
            DebugResponse::error("screenshot handled by system, not evaluator")
        }

        DebugRequest::WalkEntity { entity, max_depth } => {
            eval_walk_entity(world, *entity, *max_depth)
        }

        DebugRequest::Eval { expr } => eval_request(world, registry, expr),
    }
}

// ── Hierarchy walk (M41.0 Phase 1b.x followup) ──────────────────────────
//
// Direct World access — no per-component serde derives needed. Walks the
// `Children` chain depth-first, captures `GlobalTransform.translation` +
// `Transform.translation` per visited node, plus boolean markers for
// `SkinnedMesh` and `MeshHandle` (the two component types that decide
// whether the entity actually contributes draw calls).

fn eval_walk_entity(world: &World, root: u32, max_depth: u32) -> DebugResponse {
    use byroredux_core::ecs::components::{Children, MeshHandle, Name, Parent};
    use byroredux_core::ecs::{GlobalTransform, SkinnedMesh, Transform};
    use byroredux_debug_protocol::HierarchyNode;

    let parent_q = world.query::<Parent>();
    let children_q = world.query::<Children>();
    let gt_q = world.query::<GlobalTransform>();
    let t_q = world.query::<Transform>();
    let name_q = world.query::<Name>();
    let skin_q = world.query::<SkinnedMesh>();
    let mesh_q = world.query::<MeshHandle>();
    let pool = world.try_resource::<byroredux_core::string::StringPool>();

    let mut nodes: Vec<HierarchyNode> = Vec::new();
    let mut stack: Vec<(u32, u32)> = vec![(root, 0)];

    while let Some((entity, depth)) = stack.pop() {
        if depth > max_depth {
            continue;
        }

        let parent = parent_q.as_ref().and_then(|q| q.get(entity)).map(|p| p.0);
        let kids: Vec<u32> = children_q
            .as_ref()
            .and_then(|q| q.get(entity))
            .map(|c| c.0.iter().take(32).copied().collect())
            .unwrap_or_default();
        let gt_t = gt_q
            .as_ref()
            .and_then(|q| q.get(entity))
            .map(|gt| [gt.translation.x, gt.translation.y, gt.translation.z]);
        let local_t = t_q
            .as_ref()
            .and_then(|q| q.get(entity))
            .map(|t| [t.translation.x, t.translation.y, t.translation.z]);
        let name = name_q.as_ref().and_then(|q| q.get(entity)).and_then(|n| {
            pool.as_ref()
                .and_then(|p| p.resolve(n.0).map(|s| s.to_string()))
        });
        let has_skin = skin_q.as_ref().is_some_and(|q| q.get(entity).is_some());
        let has_mesh = mesh_q.as_ref().is_some_and(|q| q.get(entity).is_some());

        nodes.push(HierarchyNode {
            id: entity,
            depth,
            parent,
            name,
            children: kids.clone(),
            gt_translation: gt_t,
            local_translation: local_t,
            has_skinned_mesh: has_skin,
            has_mesh_handle: has_mesh,
        });

        // Depth-first: push children in reverse so first child pops next.
        for &child in kids.iter().rev() {
            stack.push((child, depth + 1));
        }
    }

    DebugResponse::Hierarchy { nodes }
}

/// Resolve a free-form evaluation request. Pre-#518 this fell straight
/// through to the Papyrus expression parser, which meant every
/// dot-separated command name registered in the engine's
/// `CommandRegistry` (`tex.missing`, `tex.loaded`, `mesh.info`,
/// `mesh.cache`, …) parsed as `Ident("tex") . member("missing")`,
/// triggered `find_by_name("tex")`, and returned
/// `"no entity named 'tex'"`. Now the first whitespace-delimited
/// token is looked up in the registry first; a match dispatches
/// through the in-engine command, a miss falls back to Papyrus
/// evaluation (so `42.Transform.translation.x` still works).
fn eval_request(world: &World, registry: &ComponentRegistry, expr: &str) -> DebugResponse {
    let first_word = expr.trim().split_whitespace().next().unwrap_or("");
    if !first_word.is_empty() {
        if let Some(reg) = world.try_resource::<CommandRegistry>() {
            if reg.list().iter().any(|(name, _)| *name == first_word) {
                let output = reg.execute(world, expr);
                return DebugResponse::value(serde_json::Value::String(output.lines.join("\n")));
            }
        }
    }
    eval_expr(world, registry, expr)
}

// ── Stats ───────────────────────────────────────────────────────────────

fn eval_stats(world: &World) -> DebugResponse {
    match world.try_resource::<DebugStats>() {
        Some(stats) => DebugResponse::Stats {
            fps: stats.fps,
            avg_fps: stats.avg_fps(),
            frame_time_ms: stats.frame_time_ms,
            entity_count: stats.entity_count,
            mesh_count: stats.mesh_count,
            texture_count: stats.texture_count,
            draw_call_count: stats.draw_call_count,
        },
        None => DebugResponse::error("DebugStats resource not available"),
    }
}

// ── Find entity by name ─────────────────────────────────────────────────

fn eval_find(world: &World, name: &str) -> DebugResponse {
    match world.find_by_name(name) {
        Some(entity_id) => {
            let info = EntityInfo {
                id: entity_id,
                name: Some(name.to_string()),
            };
            DebugResponse::EntityList {
                entities: vec![info],
            }
        }
        None => DebugResponse::error(format!("no entity named '{}'", name)),
    }
}

// ── List entities (optionally filtered by component) ────────────────────

fn eval_list_entities(
    world: &World,
    registry: &ComponentRegistry,
    component: Option<&str>,
) -> DebugResponse {
    match component {
        Some(comp_name) => {
            let desc = match registry.get(comp_name) {
                Some(d) => d,
                None => {
                    return DebugResponse::error(format!(
                        "unknown component '{}'. Use 'components' to list available types.",
                        comp_name
                    ))
                }
            };
            let ids = (desc.list_entities)(world as &dyn std::any::Any);
            let entities = ids
                .into_iter()
                .map(|id| EntityInfo {
                    id,
                    name: resolve_entity_name(world, id),
                })
                .collect();
            DebugResponse::EntityList { entities }
        }
        None => {
            // List all entities that have a Name component.
            if let Some(names) = world.query::<Name>() {
                let pool = world.try_resource::<StringPool>();
                let entities = names
                    .iter()
                    .map(|(id, name_comp)| EntityInfo {
                        id,
                        name: pool
                            .as_ref()
                            .and_then(|p| p.resolve(name_comp.0).map(|s| s.to_string())),
                    })
                    .collect();
                DebugResponse::EntityList { entities }
            } else {
                DebugResponse::EntityList {
                    entities: Vec::new(),
                }
            }
        }
    }
}

// ── Get component data ──────────────────────────────────────────────────

fn eval_get_component(
    world: &World,
    registry: &ComponentRegistry,
    entity: u32,
    component: &str,
) -> DebugResponse {
    let desc = match registry.get(component) {
        Some(d) => d,
        None => return DebugResponse::error(format!("unknown component '{}'", component)),
    };
    match (desc.get_json)(world as &dyn std::any::Any, entity) {
        Some(value) => DebugResponse::value(value),
        None => DebugResponse::error(format!("entity {} has no {} component", entity, component)),
    }
}

// ── Set field ───────────────────────────────────────────────────────────

fn eval_set_field(
    world: &World,
    registry: &ComponentRegistry,
    entity: u32,
    component: &str,
    path: &str,
    value: serde_json::Value,
) -> DebugResponse {
    let desc = match registry.get(component) {
        Some(d) => d,
        None => return DebugResponse::error(format!("unknown component '{}'", component)),
    };
    match (desc.set_field)(world as &dyn std::any::Any, entity, path, value) {
        Ok(()) => DebugResponse::Ok,
        Err(e) => DebugResponse::error(e),
    }
}

// ── Expression evaluation ───────────────────────────────────────────────

/// Evaluate a Papyrus expression string.
///
/// Supports:
/// - `42.Transform` → GetComponent { entity: 42, component: "Transform" }
/// - `42.Transform.translation` → GetComponent then drill into JSON field
/// - `find("name")` → FindEntity
/// - `entities(Component)` → ListEntities
/// - Integer literals → entity ID info
/// - String literals → find by name
fn eval_expr(world: &World, registry: &ComponentRegistry, expr_str: &str) -> DebugResponse {
    // Parse the expression using the Papyrus parser.
    let parsed = match byroredux_papyrus::parse_expr(expr_str) {
        Ok(spanned) => spanned.node,
        Err(errors) => {
            let msg = errors
                .iter()
                .map(|e| format!("{:?}", e))
                .collect::<Vec<_>>()
                .join("; ");
            return DebugResponse::error(format!("parse error: {}", msg));
        }
    };

    eval_ast(world, registry, &parsed)
}

/// Recursively evaluate an AST node.
fn eval_ast(world: &World, registry: &ComponentRegistry, expr: &Expr) -> DebugResponse {
    match expr {
        // Integer literal → treat as entity ID
        Expr::IntLit(id) => {
            let entity = *id as u32;
            let name = resolve_entity_name(world, entity);
            DebugResponse::EntityList {
                entities: vec![EntityInfo { id: entity, name }],
            }
        }

        // Float literal → treat as entity ID when whole number.
        // The Papyrus lexer greedily parses `42.Transform` as
        // `FloatLit(42.0).MemberAccess("Transform")`, so member access
        // chains on integer entity IDs route through here.
        Expr::FloatLit(f) => {
            let entity = *f as u32;
            let name = resolve_entity_name(world, entity);
            DebugResponse::EntityList {
                entities: vec![EntityInfo { id: entity, name }],
            }
        }

        // String literal → find by name
        Expr::StringLit(name) => eval_find(world, name),

        // Identifier → try as entity name
        Expr::Ident(ident) => eval_find(world, &ident.0),

        // Member access: object.member
        Expr::MemberAccess { object, member } => {
            eval_member_access(world, registry, &object.node, &member.node)
        }

        // Function call
        Expr::Call { callee, args } => eval_call(world, registry, &callee.node, args),

        _ => DebugResponse::error(format!(
            "unsupported expression type: {:?}",
            std::mem::discriminant(expr)
        )),
    }
}

/// Evaluate a member access chain like `42.Transform.translation.x`.
///
/// Flattens the nested MemberAccess AST into a chain: [root, member1, member2, ...],
/// then resolves: root → entity ID, member1 → component, member2+ → JSON field drilling.
fn eval_member_access(
    world: &World,
    registry: &ComponentRegistry,
    object: &Expr,
    member: &Identifier,
) -> DebugResponse {
    // Flatten the member access chain: collect all member names from right to left.
    let mut chain = vec![member.0.as_str()];
    let mut root = object;

    while let Expr::MemberAccess {
        object: inner_obj,
        member: inner_member,
    } = root
    {
        chain.push(inner_member.node.0.as_str());
        root = &inner_obj.node;
    }

    chain.reverse();
    // Now chain is [member1, member2, ...] and root is the base expression.

    // Resolve root to an entity ID.
    let entity_id = match root {
        Expr::IntLit(id) => *id as u32,
        Expr::FloatLit(f) => *f as u32,
        Expr::StringLit(name) => match world.find_by_name(name) {
            Some(id) => id,
            None => return DebugResponse::error(format!("no entity named '{}'", name)),
        },
        Expr::Ident(ident) => match world.find_by_name(&ident.0) {
            Some(id) => id,
            None => return DebugResponse::error(format!("no entity named '{}'", ident.0)),
        },
        _ => return DebugResponse::error("expected entity ID, name, or string".to_string()),
    };

    // chain[0] should be a component name.
    if chain.is_empty() {
        return DebugResponse::error("empty member access chain".to_string());
    }

    let component_name = chain[0];
    let desc = match registry.get(component_name) {
        Some(d) => d,
        None => return DebugResponse::error(format!("unknown component '{}'", component_name)),
    };

    // Get the full component data as JSON.
    let component_json = match (desc.get_json)(world as &dyn std::any::Any, entity_id) {
        Some(v) => v,
        None => {
            return DebugResponse::error(format!(
                "entity {} has no {} component",
                entity_id, component_name
            ))
        }
    };

    // If there are no further members, return the full component.
    if chain.len() == 1 {
        return DebugResponse::value(component_json);
    }

    // Drill into JSON fields for the remaining chain members.
    let mut current = &component_json;
    for (i, field_name) in chain[1..].iter().enumerate() {
        // Try named field access first.
        if let Some(val) = current.get(*field_name) {
            current = val;
            continue;
        }

        // Try x/y/z/w aliases for glam array serialization.
        let idx = match *field_name {
            "x" | "r" => Some(0),
            "y" | "g" => Some(1),
            "z" | "b" => Some(2),
            "w" | "a" => Some(3),
            _ => None,
        };
        if let Some(idx) = idx {
            if let Some(val) = current.get(idx) {
                current = val;
                continue;
            }
        }

        // Field not found.
        let path_so_far = chain[..=i + 1].join(".");
        return DebugResponse::error(format!(
            "no field '{}' at {}.{}",
            field_name, entity_id, path_so_far
        ));
    }

    DebugResponse::value(current.clone())
}

/// Evaluate function calls.
fn eval_call(
    world: &World,
    registry: &ComponentRegistry,
    callee: &Expr,
    args: &[CallArg],
) -> DebugResponse {
    let func_name = match callee {
        Expr::Ident(id) => id.0.to_ascii_lowercase(),
        _ => return DebugResponse::error("expected function name"),
    };

    match func_name.as_str() {
        "find" => {
            if let Some(arg) = args.first() {
                match &arg.value.node {
                    Expr::StringLit(name) => eval_find(world, name),
                    Expr::Ident(id) => eval_find(world, &id.0),
                    _ => DebugResponse::error("find() expects a string argument"),
                }
            } else {
                DebugResponse::error("find() requires one argument")
            }
        }
        "entities" => {
            let component = args.first().and_then(|a| match &a.value.node {
                Expr::Ident(id) => Some(id.0.as_str()),
                Expr::StringLit(s) => Some(s.as_str()),
                _ => None,
            });
            eval_list_entities(world, registry, component)
        }
        "stats" => eval_stats(world),
        "components" => DebugResponse::ComponentList {
            components: registry.names().into_iter().map(String::from).collect(),
        },
        "count" => {
            if let Some(arg) = args.first() {
                if let Expr::Ident(id) = &arg.value.node {
                    match registry.get(&id.0) {
                        Some(desc) => {
                            let ids = (desc.list_entities)(world as &dyn std::any::Any);
                            DebugResponse::value(serde_json::Value::Number(ids.len().into()))
                        }
                        None => DebugResponse::error(format!("unknown component '{}'", id.0)),
                    }
                } else {
                    DebugResponse::error("count() expects a component name")
                }
            } else {
                // No arg → total named entity count
                let count = world.query::<Name>().map_or(0, |q| q.len());
                DebugResponse::value(serde_json::Value::Number(count.into()))
            }
        }
        "tex_missing" => eval_tex_missing(world),
        "tex_loaded" => eval_tex_loaded(world),
        _ => DebugResponse::error(format!("unknown function '{}'", func_name)),
    }
}

// ── Texture debug ──────────────────────────────────────────────────────

fn eval_tex_missing(world: &World) -> DebugResponse {
    let (Some(tex_q), Some(mat_q)) = (world.query::<TextureHandle>(), world.query::<Material>())
    else {
        return DebugResponse::error("No TextureHandle or Material components");
    };
    let mut missing: HashMap<String, u32> = HashMap::new();
    for (entity, tex) in tex_q.iter() {
        if tex.0 != 0 {
            continue;
        }
        let mat = mat_q.get(entity);
        let path = mat
            .and_then(|m| m.texture_path.as_deref())
            .or_else(|| mat.and_then(|m| m.material_path.as_deref()))
            .unwrap_or("<no path, no material>");
        *missing.entry(path.to_string()).or_insert(0) += 1;
    }
    let mut sorted: Vec<_> = missing.into_iter().collect();
    sorted.sort_by(|a, b| b.1.cmp(&a.1));
    let lines: Vec<String> =
        std::iter::once(format!("{} unique missing texture paths:", sorted.len()))
            .chain(
                sorted
                    .iter()
                    .take(80)
                    .map(|(p, c)| format!("  {:4}x  {}", c, p)),
            )
            .collect();
    DebugResponse::value(serde_json::Value::String(lines.join("\n")))
}

fn eval_tex_loaded(world: &World) -> DebugResponse {
    let (Some(tex_q), Some(mat_q)) = (world.query::<TextureHandle>(), world.query::<Material>())
    else {
        return DebugResponse::error("No TextureHandle or Material components");
    };
    let mut loaded: HashMap<String, u32> = HashMap::new();
    let mut fallback = 0u32;
    for (entity, tex) in tex_q.iter() {
        if tex.0 == 0 {
            fallback += 1;
            continue;
        }
        let path = mat_q
            .get(entity)
            .and_then(|m| m.texture_path.as_deref())
            .unwrap_or("<no path>");
        *loaded.entry(path.to_string()).or_insert(0) += 1;
    }
    let mut sorted: Vec<_> = loaded.into_iter().collect();
    sorted.sort_by(|a, b| b.1.cmp(&a.1));
    let lines: Vec<String> = std::iter::once(format!(
        "{} unique loaded, {} fallback entities",
        sorted.len(),
        fallback
    ))
    .chain(
        sorted
            .iter()
            .take(50)
            .map(|(p, c)| format!("  {:4}x  {}", c, p)),
    )
    .collect();
    DebugResponse::value(serde_json::Value::String(lines.join("\n")))
}

// ── Helpers ─────────────────────────────────────────────────────────────

/// Resolve an entity ID to its Name string (if it has one).
fn resolve_entity_name(world: &World, entity: u32) -> Option<String> {
    let name_comp = world.get::<Name>(entity)?;
    let pool = world.try_resource::<StringPool>()?;
    pool.resolve(name_comp.0).map(|s| s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use byroredux_core::console::{CommandOutput, ConsoleCommand};
    use byroredux_debug_protocol::registry::ComponentRegistry;

    /// A test ConsoleCommand whose name mirrors the `tex.missing` shape —
    /// exercises the dot-in-name code path, since that's the form the
    /// Papyrus parser chokes on.
    struct DotNameCommand;
    impl ConsoleCommand for DotNameCommand {
        fn name(&self) -> &str {
            "tex.missing"
        }
        fn description(&self) -> &str {
            "test command"
        }
        fn execute(&self, _world: &World, args: &str) -> CommandOutput {
            CommandOutput::lines(vec!["header".to_string(), format!("args={}", args)])
        }
    }

    /// Regression for #518: dot-separated command names must dispatch
    /// through the `CommandRegistry` resource and return their output
    /// lines joined by newlines in a `Value` response. Pre-fix the
    /// same input fell through to `eval_expr`, which parsed
    /// `tex.missing` as `Ident("tex") . member("missing")` and
    /// returned `"no entity named 'tex'"`.
    #[test]
    fn eval_request_dispatches_dotted_command_names() {
        let mut world = World::new();
        let mut registry = CommandRegistry::new();
        registry.register(DotNameCommand);
        world.insert_resource(registry);

        let component_registry = ComponentRegistry::new();
        let request = DebugRequest::Eval {
            expr: "tex.missing".to_string(),
        };
        let response = evaluate(&world, &component_registry, &request);
        match response {
            DebugResponse::Value { data } => {
                let s = data.as_str().expect("expected String value");
                assert_eq!(s, "header\nargs=");
            }
            other => panic!("expected Value response, got {:?}", other),
        }
    }

    /// Args after the command name survive intact through the pre-
    /// dispatch path. Exercises the `mesh.info <entity_id>` shape.
    #[test]
    fn eval_request_forwards_args_to_registered_command() {
        let mut world = World::new();
        let mut registry = CommandRegistry::new();
        registry.register(DotNameCommand);
        world.insert_resource(registry);

        let component_registry = ComponentRegistry::new();
        let request = DebugRequest::Eval {
            expr: "tex.missing 42 arg2".to_string(),
        };
        let response = evaluate(&world, &component_registry, &request);
        match response {
            DebugResponse::Value { data } => {
                assert_eq!(data.as_str(), Some("header\nargs=42 arg2"));
            }
            other => panic!("expected Value response, got {:?}", other),
        }
    }

    /// Expressions whose first token is NOT a registered command still
    /// route to the Papyrus evaluator — `42.Transform.translation.x`
    /// drilling must keep working.
    #[test]
    fn eval_request_falls_back_to_papyrus_for_unregistered_input() {
        let mut world = World::new();
        // Empty CommandRegistry — nothing to match, so the
        // unregistered expression must fall through to eval_expr.
        world.insert_resource(CommandRegistry::new());

        let component_registry = ComponentRegistry::new();
        let request = DebugRequest::Eval {
            expr: "42".to_string(),
        };
        let response = evaluate(&world, &component_registry, &request);
        // `42` parses as an IntLit and evaluates to an EntityList
        // (an unnamed entity id). The exact content isn't the point —
        // we only need to verify the response is NOT a CommandRegistry
        // dispatch (no Value with newline-joined output).
        match response {
            DebugResponse::EntityList { entities } => {
                assert_eq!(entities.len(), 1);
                assert_eq!(entities[0].id, 42);
            }
            other => panic!("expected EntityList (Papyrus fallback), got {:?}", other),
        }
    }
}
