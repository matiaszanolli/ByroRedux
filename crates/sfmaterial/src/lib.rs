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
//! consumer-side mapping (Starfield-specific material → `ImportedMesh`
//! fields) happens in `byroredux/src/asset_provider.rs` and is a
//! separate concern from the format parsing here.

mod chunk;
mod error;
mod reader;
mod string_table;
mod types;
mod value;

pub use chunk::ChunkType;
pub use error::{Error, Result};
pub use reader::ComponentDatabaseFile;
pub use value::Value;
