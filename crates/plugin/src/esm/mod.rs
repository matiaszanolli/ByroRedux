//! ESM/ESP binary file parser — reads Bethesda plugin records.
//!
//! Provides a low-level binary reader for the TES4 record format used by
//! Oblivion, Fallout 3, Fallout New Vegas, Skyrim, and Fallout 4.
//! Higher-level record extraction (CELL, REFR, STAT, etc.) builds on top.

pub mod cell;
pub mod reader;
pub mod records;
pub mod sub_reader;

pub use cell::{CellData, EsmCellIndex, PlacedRef, StaticObject};
pub use reader::{EsmReader, GroupHeader, RecordHeader, SubRecord};
pub use sub_reader::SubReader;
pub use records::{
    parse_esm, ClassRecord, ContainerRecord, EsmIndex, FactionRecord, GameSetting, GlobalRecord,
    InventoryEntry, ItemKind, ItemRecord, LeveledEntry, LeveledList, NpcRecord, RaceRecord,
    SettingValue,
};
