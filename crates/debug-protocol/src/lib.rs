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
    /// Capture a screenshot of the current frame.
    /// Optionally save to a file path; if None, returns raw PNG bytes.
    Screenshot { path: Option<String> },
    /// Walk the scene hierarchy from a root entity. Returns each visited
    /// node's id, name, parent, children, and world translation. Used to
    /// inspect runtime entity trees (e.g. NPC spawn chains) without
    /// needing per-component serde derives.
    WalkEntity { entity: u32, max_depth: u32 },
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
    /// Screenshot captured — PNG bytes (base64-encoded for JSON transport).
    Screenshot {
        png_base64: String,
        width: u32,
        height: u32,
    },
    /// Screenshot saved to a file path.
    ScreenshotSaved { path: String },
    /// Successful mutation with no return value.
    Ok,
    /// Pong response to Ping.
    Pong,
    /// Hierarchy walk result — flat list, depth-first order from the root.
    Hierarchy { nodes: Vec<HierarchyNode> },
    /// An error message.
    Error { message: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityInfo {
    pub id: u32,
    pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HierarchyNode {
    pub id: u32,
    pub depth: u32,
    pub parent: Option<u32>,
    pub name: Option<String>,
    /// Children entity IDs (truncated to first 32 if more).
    pub children: Vec<u32>,
    /// World-space translation from `GlobalTransform`, or `None` if missing.
    pub gt_translation: Option<[f32; 3]>,
    /// World-space rotation quaternion `[x, y, z, w]` from `GlobalTransform`.
    pub gt_rotation: Option<[f32; 4]>,
    /// Local-space translation from `Transform`, or `None` if missing.
    pub local_translation: Option<[f32; 3]>,
    /// Local-space rotation quaternion `[x, y, z, w]` from `Transform`.
    pub local_rotation: Option<[f32; 4]>,
    /// Marker fields the renderer cares about.
    pub has_skinned_mesh: bool,
    pub has_mesh_handle: bool,
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
