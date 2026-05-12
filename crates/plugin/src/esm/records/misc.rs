//! Stub parsers for ~40 record types that were previously falling
//! through the `parse_esm` catch-all and getting skipped wholesale
//! (#458 / audit FO3-3-07). Each parser extracts enough data for
//! *references* into the record to resolve — typically EDID + a
//! handful of form refs + a couple of scalar fields — without doing
//! deep sub-record decoding. Full parsing of each can be tightened
//! up per-type when the consuming system lands.
//!
//! Split across themed submodules (each owns a handful of related
//! records + their regression tests):
//!
//! - [`water`] — `WATR`
//! - [`character`] — `HDPT` / `EYES` / `HAIR`
//! - [`world`] — `NAVI` / `NAVM` / `REGN` / `ECZN` / `LGTM` / `IMGS`
//!   / `ACTI` / `TERM`
//! - [`ai`] — `PACK` / `QUST` / `DIAL` / `INFO` / `MESG` / `IDLE`
//!   / `CSTY`
//! - [`magic`] — `PERK` / `SPEL` / `MGEF` / `ENCH`
//! - [`effects`] — `AVIF` / `PROJ` / `EFSH` / `IMOD` / `EXPL` / `IPCT`
//!   / `IPDS` / `REPU`
//! - [`equipment`] — `ARMA` / `BPTD` / `COBJ` / `SLGM` /
//!   `MinimalEsmRecord`
//!
//! Per-game bit layouts vary on the LGTM + DATA / HDPT / EYES / HAIR
//! records past Skyrim; the stubs parse the FO3/FNV byte layout and
//! gracefully return defaults on short buffers — Skyrim+ re-parsing
//! lands alongside the consuming system.

mod ai;
mod character;
mod effects;
mod equipment;
mod magic;
mod water;
mod world;

pub use ai::{
    parse_csty, parse_dial, parse_idle, parse_info, parse_mesg, parse_pack, parse_qust, CstyRecord,
    DialRecord, IdleRecord, InfoRecord, MesgRecord, PackRecord, QustRecord,
};
pub use character::{parse_eyes, parse_hair, parse_hdpt, EyesRecord, HairRecord, HdptRecord};
pub use effects::{
    parse_avif, parse_efsh, parse_expl, parse_imod, parse_ipct, parse_ipds, parse_proj, parse_repu,
    AvifRecord, EfshRecord, ExplRecord, ImodRecord, IpctRecord, IpdsRecord, ProjRecord, RepuRecord,
};
pub use equipment::{
    parse_arma, parse_bptd, parse_cobj, parse_minimal_esm_record, parse_slgm, ArmaRecord,
    BptdRecord, CobjRecord, MinimalEsmRecord, SlgmRecord,
};
pub use magic::{
    parse_ench, parse_mgef, parse_perk, parse_spel, EnchRecord, MgefRecord, PerkRecord, SpelRecord,
};
pub use water::{parse_watr, watr_to_params, WaterParams, WatrRecord};
pub use world::{
    parse_acti, parse_eczn, parse_imgs, parse_lgtm, parse_navi, parse_navm, parse_regn, parse_term,
    ActiRecord, EcznRecord, ImgsRecord, LgtmRecord, NaviRecord, NavmRecord, RegnRecord, TermRecord,
};
