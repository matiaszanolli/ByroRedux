//! Tests for `super::cell` (cell + worldspace + record parsing).
//!
//! Split out of the 3 329-LOC monolithic `tests.rs` into per-topic
//! sibling files. Each sibling owns its own helper builders; nothing
//! cross-cuts so `mod.rs` is just the dispatch table.
//!
//! - [`light`]       — LIGH (lights, color decode, FO4 XPWR)
//! - [`addn_stat`]   — ADDN, STAT, MODL group walk
//! - [`refr`]        — REFR placement (XESP, XTEL, XLKR, XPRM, ownership, …)
//! - [`cell`]        — CELL (water height, RCLR, Skyrim/FNV extended XCLL)
//! - [`txst`]        — TXST (8 texture slots, MNAM, DODT, DNAM flags)
//! - [`merge`]       — Plugin merge across statics + cells + worldspaces
//! - [`wrld`]        — WRLD (worldspace fields, parent link, truncated)
//! - [`integration`] — Real ESM walkers: FNV / Oblivion / FO3 / Skyrim / FO4

mod addn_stat;
mod cell;
mod integration;
mod light;
mod merge;
mod refr;
mod txst;
mod wrld;
