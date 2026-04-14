//! Wire protocol for the ByroRedux debug server.
//!
//! Shared between engine-side server and standalone CLI client.
//! Length-prefixed JSON over TCP: 4-byte big-endian length, then UTF-8 JSON.

pub mod registry;
pub mod wire;

use serde::{Deserialize, Serialize};

/// Default TCP port for the debug server.
pub const DEFAULT_PORT: u16 = 9876;

// ── Request ─────────────────────────────────────────────────────────────

/// A command sent from the CLI client to the engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum DebugRequest {
    /// Evaluate a Papyrus-style expression and return the result.
    Eval { expr: String },
    /// List entities, optionally filtered by component type name.
    ListEntities { component: Option<String> },
    /// Get all fields of a component on an entity.
    GetComponent { entity: u32, component: String },
    /// Set a field on a component. `path` is dot-separated (e.g. "translation.x").
    SetField {
        entity: u32,
        component: String,
        path: String,
        value: serde_json::Value,
    },
    /// List all registered inspectable component type names.
    ListComponents,
    /// List registered ECS systems in execution order.
    ListSystems,
    /// Get engine stats (FPS, frame time, entity/mesh/texture/draw counts).
    Stats,
    /// Find an entity by name (searches the Name component).
    FindEntity { name: String },
    /// Ping / keep-alive.
    Ping,
}

// ── Response ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DebugResponse {
    /// A JSON value result (component data, field value, expression result).
    Value { data: serde_json::Value },
    /// A list of entities with optional name.
    EntityList { entities: Vec<EntityInfo> },
    /// A list of component type names.
    ComponentList { components: Vec<String> },
    /// A list of system names grouped by stage.
    SystemList { systems: Vec<String> },
    /// Engine stats snapshot.
    Stats {
        fps: f32,
        avg_fps: f32,
        frame_time_ms: f32,
        entity_count: u32,
        mesh_count: u32,
        texture_count: u32,
        draw_call_count: u32,
    },
    /// Successful mutation with no return value.
    Ok,
    /// Pong response to Ping.
    Pong,
    /// An error message.
    Error { message: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityInfo {
    pub id: u32,
    pub name: Option<String>,
}

impl DebugResponse {
    pub fn error(msg: impl Into<String>) -> Self {
        Self::Error {
            message: msg.into(),
        }
    }

    pub fn value(data: serde_json::Value) -> Self {
        Self::Value { data }
    }
}
