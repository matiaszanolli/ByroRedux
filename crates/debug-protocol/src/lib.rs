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
    /// M41.0 Phase 1b.x — dump a `SkinnedMesh` component. Returns each
    /// bone's resolved entity (or null), the bind-inverse 4x4 matrix
    /// (column-major), the skeleton-root entity, and the bone count.
    /// Pairs with `WalkEntity` to let an external Python probe iterate
    /// skinning-formula variations against live engine data without
    /// rebuilding the engine.
    InspectSkinnedMesh { entity: u32 },
    /// Dump every registered component on an entity (the inspection
    /// half of the Bethesda console's `prid` + per-ref-inspection
    /// workflow). When `entity` is `None`, the evaluator reads the
    /// `SelectedRef` world resource and inspects whatever was picked
    /// with `prid <id>`. Returns the entity's `Name` (resolved through
    /// `StringPool` when present) plus an ordered list of
    /// `(component_name, JSON value)` pairs.
    Inspect { entity: Option<u32> },
    /// Sample the live runtime metrics (CPU / RAM / VRAM / GPU
    /// pass times). The engine refreshes the underlying snapshot at
    /// ~2 Hz; this request reads it without forcing a refresh, so
    /// repeated polling between sample ticks returns identical
    /// values. Drives the debug-UI overlay + the TUI dashboard.
    Metrics,
    /// Queue a NIF mesh load. Returns `Ok` immediately; the engine
    /// drains the load queue between frames where `&mut World` and
    /// `&mut VulkanContext` are both held (mirrors the existing
    /// `PendingCellTransition` pattern).
    LoadNif {
        /// NIF path — either an absolute filesystem path (loose file)
        /// or an archive-relative path like `meshes\foo.nif` resolved
        /// through the active BSA / BA2 set.
        path: String,
        /// Optional diagnostic label — surfaces in engine logs and
        /// becomes the entity's `Name` when no name resolves from
        /// the NIF. Defaults to the basename of `path` when omitted.
        label: Option<String>,
    },
    /// Queue an interior cell load by editor ID. Same async-via-queue
    /// semantics as `LoadNif`.
    LoadInteriorCell {
        esm: String,
        cell: String,
        /// Master ESMs required by `esm`, in dependency order. Empty
        /// when `esm` is a standalone master.
        masters: Vec<String>,
        /// Mesh BSA / BA2 archive paths.
        bsas: Vec<String>,
        /// Texture BSA / BA2 archive paths.
        textures_bsas: Vec<String>,
    },
    /// Queue an exterior grid load.
    LoadExteriorCell {
        esm: String,
        grid_x: i32,
        grid_y: i32,
        /// Streaming radius (clamped to `1..=7` by the engine to match
        /// the CLI `--radius` cap).
        radius: u8,
        /// Worldspace EDID override — needed for ESMs that ship
        /// multiple worldspaces (FO3 / FNV both pick wasteland by
        /// default; Skyrim SE has Tamriel).
        worldspace: Option<String>,
        masters: Vec<String>,
        bsas: Vec<String>,
        textures_bsas: Vec<String>,
    },
    /// Enumerate the configured game profiles. Profiles come from
    /// `assets/debug_profiles.toml` (engine-shipped defaults) plus
    /// the per-user override at `~/.byroredux/profiles.toml` if
    /// present. Phase-5 wiring.
    ListGameProfiles,
    /// Enumerate loaded asset handles. `kind` picks between the
    /// MeshRegistry, TextureRegistry, and NIF import cache views.
    ListLoadedAssets { kind: AssetKind },
    /// Ping / keep-alive.
    Ping,
}

/// Which asset registry [`DebugRequest::ListLoadedAssets`] enumerates.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AssetKind {
    /// GPU mesh handles in the MeshRegistry.
    Meshes,
    /// GPU texture handles in the TextureRegistry.
    Textures,
    /// Parsed NIF scenes in the `NifImportRegistry` cache.
    NifCache,
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
        /// Registry-wide MeshRegistry size (never drops on cell unload).
        /// See #637 / FNV-D5-02.
        mesh_count: u32,
        /// Registry-wide TextureRegistry size. See #637 / FNV-D5-02.
        texture_count: u32,
        /// Distinct non-zero `MeshHandle` values held by live ECS
        /// entities. Scene-scoped — pairs with [`mesh_count`] to spot
        /// retain-past-unload regressions. See #637 / FNV-D5-02.
        meshes_in_use: u32,
        /// Distinct non-zero `TextureHandle` values held by live ECS
        /// entities. Scene-scoped — pairs with [`texture_count`].
        /// See #637 / FNV-D5-02.
        textures_in_use: u32,
        /// Pre-batch `DrawCommand` count input to the batcher. Renamed
        /// from `draw_call_count` in #1258 / PERF-D3-NEW-03 — the old
        /// name was misleading because the field stored input-to-batcher
        /// not actual GPU draw calls. See `batch_count` +
        /// `indirect_call_count` below for the full pipeline view.
        draw_command_count: u32,
        /// Post-merge `DrawBatch` count from the main raster pass.
        /// Upper bound on GPU draw calls; indirect grouping further
        /// compresses runs (see `indirect_call_count`). #1258.
        batch_count: u32,
        /// Actual `cmd_draw_indexed` + `cmd_draw_indexed_indirect`
        /// invocations recorded — the real "draws" cost number. #1258.
        indirect_call_count: u32,
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
    /// SkinnedMesh component inspection — lets external probes verify
    /// skinning formulas live against engine data.
    SkinnedMesh {
        skeleton_root: Option<u32>,
        bones: Vec<Option<u32>>,
        bone_names: Vec<Option<String>>,
        /// Each bone's bind-inverse as a 16-float column-major mat4.
        bind_inverses: Vec<[f32; 16]>,
        /// Per-skin global transform (`NiSkinData::skinTransform` after
        /// Y-up conversion). Identity if the source NIF didn't carry
        /// one (FO4+ BSSkin paths). Stored on `SkinnedMesh` for the
        /// Phase 1b.x palette-formula investigation.
        global_skin_transform: [f32; 16],
        /// Per-bone resolved `GlobalTransform.to_matrix()` at the moment
        /// of the dump. `None` when the bone resolved to no entity at
        /// scene-import time, or when the bone entity carries no
        /// `GlobalTransform` (which on a populated cell means the
        /// transform-propagation BFS skipped it — itself a bug). Pairs
        /// 1:1 with `bones` and `bind_inverses`.
        bone_world_matrices: Vec<Option<[f32; 16]>>,
        /// `palette[i] = bone_world × bind_inverses[i]`, or identity for
        /// dropout slots (matches `compute_palette_into` exactly). This
        /// is the matrix the renderer actually pushes to the GPU; an
        /// external probe diffs this against the M29 standalone path
        /// to localize the spike-artifact divergence (#841).
        palette: Vec<[f32; 16]>,
    },
    /// Per-entity component dump produced by [`DebugRequest::Inspect`].
    /// `components` is the ordered list of `(type_name, JSON value)`
    /// pairs for every registered component the entity currently
    /// carries. `name` is the entity's resolved `Name` (or `None`).
    Inspect {
        entity: u32,
        name: Option<String>,
        components: Vec<(String, serde_json::Value)>,
    },
    /// Metrics snapshot — wire twin of
    /// `byroredux_core::ecs::MetricsSnapshot`. The protocol crate
    /// doesn't depend on core so the fields are inlined here. Stays
    /// in lockstep with the core type by hand; the
    /// `metrics_response_mirrors_core_snapshot` test in the server
    /// pins the field set so a one-sided drift breaks the build.
    Metrics {
        /// Unix-epoch seconds at which the engine refreshed the
        /// underlying snapshot. Zero before the first 2 Hz tick.
        sampled_at_secs: u64,
        /// Whole-process CPU usage in percent (0..N*100 across N cores).
        cpu_pct: f32,
        /// System-wide RAM used / total in MB.
        ram_used_mb: u64,
        ram_total_mb: u64,
        /// Engine process RSS in MB.
        process_ram_mb: u64,
        /// GPU memory allocated / reserved by gpu-allocator in MB.
        vram_used_mb: u64,
        vram_reserved_mb: u64,
        /// Sum of `DEVICE_LOCAL` heap capacities, in MB. Constant
        /// after device pick.
        vram_budget_mb: u64,
        /// Per-pass GPU elapsed time in milliseconds, ordered by
        /// pass name. Surfaces `SkinCoverageStats::gpu_*_ms` today
        /// (`"skin"`, `"skin_blas_refit"`, `"taa"`); extensible
        /// without a wire bump.
        gpu_pass_ms: Vec<(String, f32)>,
    },
    /// Configured game profiles — populated from
    /// `assets/debug_profiles.toml` (engine defaults) merged with
    /// `~/.byroredux/profiles.toml` (per-user overrides). Phase 5.
    GameProfiles { profiles: Vec<GameProfile> },
    /// Loaded-asset enumeration. `asset_kind` echoes the request so
    /// a pipelined caller can tell which list it's looking at.
    /// (Field is `asset_kind` rather than `kind` so it doesn't
    /// collide with serde's enum-discriminator tag.)
    AssetList {
        asset_kind: AssetKind,
        items: Vec<AssetItem>,
    },
    /// An error message.
    Error { message: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityInfo {
    pub id: u32,
    pub name: Option<String>,
}

/// One configured game profile — describes where a known game's
/// data lives on disk and the conventional archives + sample cells
/// the loader picks up by default. Mirrors `assets/debug_profiles.toml`
/// (Phase 5).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameProfile {
    /// Stable short key — `"fnv"`, `"skyrim_se"`, etc. Used by the
    /// debug UI to refer back to a profile when issuing a
    /// `LoadInteriorCell` request.
    pub key: String,
    /// Human-readable display name.
    pub name: String,
    /// Absolute path to the game's data directory (the dir that
    /// holds the ESMs).
    pub root: String,
    /// Main ESM filename inside `root` — `FalloutNV.esm` for FNV,
    /// `Skyrim.esm` for Skyrim SE.
    pub esm: String,
    /// Default mesh BSA / BA2 archive filenames (relative to
    /// `root`).
    pub default_bsas: Vec<String>,
    /// Default texture archive filenames.
    pub default_textures_bsas: Vec<String>,
    /// Curated cell editor IDs the debug UI offers as one-click
    /// quick-loads.
    pub sample_cells: Vec<String>,
}

/// One asset item in a [`DebugResponse::AssetList`]. Fields are
/// optional because different kinds expose different attributes —
/// textures carry bytes + path, the NIF cache carries a parse-stat
/// summary, meshes carry vertex / index totals via `summary`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetItem {
    pub handle: u32,
    /// Source path (BSA-relative for archive-resolved assets, loose
    /// absolute path otherwise). `None` for synthetic placeholders.
    pub path: Option<String>,
    /// On-disk / on-GPU size in bytes when meaningful (textures).
    pub bytes: Option<u64>,
    /// Human-readable one-line summary — vertex / index counts for
    /// meshes, "parsed N blocks" for cache entries, etc.
    pub summary: Option<String>,
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
