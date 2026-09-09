//! Starfield `materialsbeta.cdb` (Component Database) reader.
//!
//! Vanilla Starfield ships ALL of its material data inside a single binary
//! component-database file (`materials\materialsbeta.cdb`) packaged inside
//! `Starfield - Materials.ba2`. This crate parses that container — it does
//! NOT yet handle loose `.mat` JSON files (those are Bethesda CK / mod
//! pack output, a future Stage A).
//!
//! Format reference: [gibbed/Gibbed.Starfield](
//! https://github.com/gibbed/Gibbed.Starfield) — cloned to
//! `/mnt/data/src/reference/Gibbed.Starfield/`. Specifically
//! `projects/Gibbed.Starfield.FileFormats/ComponentDatabaseFile.cs` and
//! the `ComponentDatabase/` sibling types.
//!
//! # Format summary
//!
//! - 16-byte header: magic `BETH` (0x48544542) + headerSize=8 +
//!   fileVersion=4 + chunkCount.
//! - Body: a flat sequence of typed chunks. Each chunk is `(u32 type,
//!   u32 size, [u8; size] payload)`. The first reader pass indexes the
//!   chunks; the second pass consumes them in queue order.
//! - Chunk types: `STRT` (string table), `TYPE` (one u32 type count),
//!   `CLAS` × N (one per declared class), then a stream of `OBJT` /
//!   `USER` / `DIFF` / `USRD` / `MAPC` / `LIST` chunks carrying the
//!   actual object payloads.
//! - Each declared `Class` has a name, a u32 type id, ClassFlags
//!   (`IsUser` / `IsStruct`), and a list of `Field { name, type ref,
//!   offset, size }`.
//! - `BuiltinType` enum covers primitives + `List` + `Map` + `Ref`;
//!   class-instance fields whose type is `IsUser` are spilled into
//!   later `OBJT` chunks via a queue (the format is structurally
//!   flat for streaming).
//!
//! # Example
//!
//! ```no_run
//! use byroredux_sfmaterial::ComponentDatabaseFile;
//!
//! let bytes = std::fs::read("materialsbeta.cdb").unwrap();
//! let cdb = ComponentDatabaseFile::parse(&bytes).unwrap();
//! println!("classes: {} instances: {}", cdb.classes.len(), cdb.instances.len());
//! ```
//!
//! # Scope (Stage B per audit #762)
//!
//! This crate parses the binary CDB into a generic `Value` tree. The
//! consumer side lives in `byroredux/src/asset_provider/material/` —
//! `discover_starfield_cdbs` finds the databases and
//! `apply_cdb_pbr_fallback` is the Starfield arm, both in
//! `byroredux/src/asset_provider/material/cdb.rs`; the entry point
//! `merge_external_material` is in
//! `byroredux/src/asset_provider/material/merge.rs`. That is a separate
//! concern from the format parsing here.
//!
//! #3932 — this used to point at the pre-Session-34 `asset_provider` *file*,
//! which that refactor turned into the directory above, and to describe the
//! mapping as "material → `ImportedMesh` fields". Both were overstatements
//! of what exists: the mapping returns `MergeOutcome::PresenceOnly` today —
//! one routing flag (`is_pbr`, so the mesh takes the Disney lobe) and no
//! authored fields at all. Per-field extraction from the Component
//! Database is the deferred Phase 2 (#3398). This is the only pointer from
//! the parser to its consumer, so it is the first place someone tracing
//! the boundary looks.

mod chunk;
mod error;
mod reader;
mod string_table;
mod types;
mod value;

pub use chunk::ChunkType;
pub use error::{Error, Result};
pub use reader::{CdbHeaderInfo, ComponentDatabaseFile, ParseLimits};
pub use value::Value;

#[cfg(test)]
mod module_doc_tests {
    /// #3932 — the crate doc's pointer at its consumer named the
    /// pre-Session-34 `asset_provider` *file*, which that refactor turned
    /// into a directory. It is the only signpost from this parser to the
    /// code that consumes what it produces, so a reader tracing the boundary
    /// hits a missing path first. (Spelled without the dead path, because
    /// the scan below takes no exceptions — which is the point of it.)
    ///
    /// Every workspace path this crate's docs name must exist. A moved file
    /// then fails here instead of ageing quietly into the one pointer nobody
    /// re-checks.
    #[test]
    fn every_workspace_path_named_in_the_crate_doc_exists() {
        const LIB_RS: &str = include_str!("lib.rs");
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");

        // Prefixes composed at runtime so this test's own text is not what
        // the scan matches on.
        let prefixes = [
            format!("{}{}", "byroredux/", "src/"),
            format!("{}{}", "crates/", ""),
            format!("{}{}", "docs/", ""),
        ];
        let mut missing = Vec::new();
        for token in LIB_RS.split(|c: char| c.is_whitespace() || c == '`' || c == '(' || c == ')') {
            let token = token.trim_end_matches([',', '.', ';', ':']);
            if !prefixes
                .iter()
                .any(|prefix| token.starts_with(prefix.as_str()))
            {
                continue;
            }
            if root.join(token).exists() {
                continue;
            }
            let entry = token.to_owned();
            if !missing.contains(&entry) {
                missing.push(entry);
            }
        }
        assert!(
            missing.is_empty(),
            "the crate doc names workspace paths that do not exist: {missing:?} \
             — this crate's only pointer at its consumer is a path, so a stale \
             one is a dead end for the next reader (#3932)"
        );
    }
}
