//! M45 Save/Load — full-ECS-snapshot save format.
//!
//! The save format is a **full snapshot of the live ECS World**, not a
//! delta log against a baseline. Bethesda's save-corruption tail (slow
//! buildup of invisible inconsistencies, save-bloat from change-form
//! deltas) is structural; ECS-as-truth removes it by construction —
//! save size scales with *loaded-cell entity count*, never with
//! playthrough length, and there is no baseline to drift against.
//!
//! ## Shape
//!
//! - [`SaveRegistry`] holds, per game-state component / resource type, a
//!   pair of type-erased closures: one that serialises every
//!   `(entity, component)` pair via `World::query`, one that restores
//!   them via `World::insert_batch`. The binary populates it with the
//!   full component set (only it sees every crate), mirroring how the
//!   debug-server component registry is wired.
//! - [`Snapshot`] is the serialised container: `next_entity`, the
//!   [`StringPool`](byroredux_core::string::StringPool) dump (in symbol
//!   order, so `FixedString` symbols round-trip exactly), one JSON column
//!   per component type, and one blob per saved resource.
//! - [`encode`]/[`decode`] wrap a `Snapshot` in a versioned binary
//!   container with a CRC32 over the payload — power-cut / partial-write
//!   detection on load.
//! - [`save_world`]/[`restore_world`] are the drivers.
//! - [`disk`] adds atomic write (`tmp` → fsync → re-read+verify →
//!   rename) and a slot ring.
//! - [`validate`] is the pre-save referential-integrity pass — refuse to
//!   write a poisoned save rather than persist the slow-corruption seed.
//!
//! ## Load runs off-frame
//!
//! Save needs only `&World` and fits as a Late-stage exclusive system.
//! Load needs `&mut World` (it clears + repopulates storages) which a
//! system can't get — so the binary drains a load request between frames,
//! exactly like the existing `PendingDebugLoadSlot` path.

mod driver;
mod registry;
mod snapshot;
pub mod disk;
pub mod validate;

pub use driver::{
    apply_deltas, build_form_id_remap, restore_resources, restore_world, save_world,
};
pub use registry::SaveRegistry;
pub use snapshot::{decode, encode, Snapshot, FORMAT_MAGIC, FORMAT_MAJOR, FORMAT_MINOR};

/// Errors raised while saving, encoding, decoding, or restoring a snapshot.
#[derive(Debug, thiserror::Error)]
pub enum SaveError {
    /// A component/resource column failed to (de)serialise to/from JSON.
    #[error("serde error in column '{column}': {source}")]
    Serde {
        column: String,
        #[source]
        source: serde_json::Error,
    },

    /// The container's magic bytes don't match — not a ByroRedux save.
    #[error("not a ByroRedux save file (bad magic)")]
    BadMagic,

    /// The container is shorter than its fixed-size header.
    #[error("save file truncated: {0} bytes, need at least {1} for the header")]
    Truncated(usize, usize),

    /// The format major version is newer/older than this engine supports.
    #[error("unsupported save format major version {found} (engine supports {supported})")]
    UnsupportedVersion { found: u16, supported: u16 },

    /// The stored CRC doesn't match the payload — corrupt or partial write.
    #[error("save file CRC mismatch: stored {stored:#010x}, computed {computed:#010x}")]
    CrcMismatch { stored: u32, computed: u32 },

    /// The registered component/resource set differs from the one the
    /// save was written with (a type was added/removed/renamed) and no
    /// migrator is registered. Refused rather than silently dropping data.
    #[error("save schema fingerprint mismatch: file {file:#018x}, engine {engine:#018x}")]
    SchemaMismatch { file: u64, engine: u64 },

    /// I/O error reading or writing a save file on disk.
    #[error("save I/O error: {0}")]
    Io(#[from] std::io::Error),
}
