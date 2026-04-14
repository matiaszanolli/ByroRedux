//! Expression evaluator — walks a Papyrus AST against the World.
//!
//! Translates debug expressions into ECS queries using the component
//! registry for type-erased access.

use byroredux_core::ecs::components::Name;
use byroredux_core::ecs::resources::DebugStats;
use byroredux_core::ecs::world::World;
use byroredux_core::string::StringPool;
use byroredux_debug_protocol::registry::ComponentRegistry;
use byroredux_debug_protocol::{DebugRequest, DebugResponse, EntityInfo};

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

        DebugRequest::ListComponents => {
            DebugResponse::ComponentList {
                components: registry.names().into_iter().map(String::from).collect(),
            }
        }

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

        DebugRequest::Eval { expr } => eval_expr(world, registry, expr),
    }
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
                        name: pool.as_ref().and_then(|p| p.resolve(name_comp.0).map(|s| s.to_string())),
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
        None => DebugResponse::error(format!(
            "entity {} has no {} component",
            entity, component
        )),
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
        None => {
            return DebugResponse::error(format!("unknown component '{}'", component_name))
        }
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
        _ => DebugResponse::error(format!("unknown function '{}'", func_name)),
    }
}

// ── Helpers ─────────────────────────────────────────────────────────────

/// Resolve an entity ID to its Name string (if it has one).
fn resolve_entity_name(world: &World, entity: u32) -> Option<String> {
    let name_comp = world.get::<Name>(entity)?;
    let pool = world.try_resource::<StringPool>()?;
    pool.resolve(name_comp.0).map(|s| s.to_string())
}
