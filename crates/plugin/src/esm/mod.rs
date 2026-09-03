//! ESM/ESP binary file parser — reads Bethesda plugin records.
//!
//! Provides a low-level binary reader for the TES4 record format used by
//! Oblivion, Fallout 3, Fallout New Vegas, Skyrim, and Fallout 4.
//! Higher-level record extraction (CELL, REFR, STAT, etc.) builds on top.

pub mod cell;
pub mod reader;
pub mod records;
pub mod strings_table;
pub mod sub_reader;

// #3741 (TD2-2026-08-30-01) — deliberately NOT `#[cfg(test)]`: that gate
// only applies within this crate's own `cargo test` compilation unit,
// not when this crate is built as a normal dependency for an
// integration test binary under `tests/` (Cargo builds the library in
// non-test mode there) — which is exactly why `crates/plugin/tests/
// parse_real_esm.rs` structurally could not reach this module before,
// `pub(crate)` visibility aside. The module is a handful of trivial
// `std::env`/`PathBuf` helpers with no test-only dependencies, so
// compiling it unconditionally costs nothing.
pub mod test_paths;

pub use cell::{CellData, EsmCellIndex, PlacedRef, StaticObject};
pub use reader::{EsmReader, GroupHeader, RecordHeader, SubRecord};
pub use records::{
    parse_esm, ClassRecord, ContainerRecord, EsmIndex, FactionRecord, GameSetting, GlobalRecord,
    InventoryEntry, ItemKind, ItemRecord, LeveledEntry, LeveledList, NpcRecord, RaceRecord,
    SettingValue, StringsTableGuard,
};
pub use strings_table::{StringTableSet, StringsTable};
pub use sub_reader::SubReader;
