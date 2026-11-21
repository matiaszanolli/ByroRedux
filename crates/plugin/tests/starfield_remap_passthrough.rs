//! #7096 — `Starfield.esm` parsed through the engine's load-order path must
//! not route non-FormID words into `remap_starfield`.
//!
//! `Starfield.esm` lists no masters, so any top byte other than `0xFE`/`0xFD`
//! names a master it does not list: `remap_starfield` passes the value
//! through and logs a warning. Before the per-decoder fixes (#7098 `DOOR`,
//! #7118 `LCTN`, #7119 `NPC_` `PRPS`, #7120 CTDA sentinels) one load logged
//! 62,576 of them, burying the genuine out-of-range case. Bare `parse_esm`
//! passes no remap, so only this path sees the flood.
//!
//! Real data, so `#[ignore]`d. It parses one master (≈4 GB peak): run it alone,
//! `cargo test --release -p byroredux-plugin --test starfield_remap_passthrough -- --ignored`.

use std::sync::atomic::{AtomicUsize, Ordering};

use byroredux_plugin::esm::reader::{FormIdRemap, FormIdScheme, GlobalSlot};
use byroredux_plugin::esm::records::parse_esm_with_load_order;
use byroredux_plugin::esm::test_paths;

static PASS_THROUGHS: AtomicUsize = AtomicUsize::new(0);

/// Counts the `remap_starfield` pass-through warning; drops everything else.
struct PassThroughCounter;

impl log::Log for PassThroughCounter {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        metadata.level() <= log::Level::Warn
    }

    fn log(&self, record: &log::Record) {
        if record.level() == log::Level::Warn
            && record.args().to_string().contains("names a master this plugin does not list")
        {
            PASS_THROUGHS.fetch_add(1, Ordering::Relaxed);
        }
    }

    fn flush(&self) {}
}

#[test]
#[ignore = "parses the real Starfield.esm (~4 GB peak); run alone"]
fn starfield_esm_load_order_parse_passes_no_word_through_the_remap() {
    let path = test_paths::starfield_esm();
    let Ok(data) = std::fs::read(&path) else {
        test_paths::require_or_skip(&format!("Starfield.esm ({})", path.display()));
        return;
    };
    log::set_logger(&PassThroughCounter).expect("the only logger in this test binary");
    log::set_max_level(log::LevelFilter::Warn);

    // The engine's remap for a master-less Starfield master at slot 0.
    let remap = FormIdRemap {
        plugin_slot: GlobalSlot::Regular(0),
        master_slots: Vec::new(),
        scheme: FormIdScheme::Starfield,
    };
    parse_esm_with_load_order(&data, Some(remap)).expect("Starfield.esm parses");

    assert_eq!(
        PASS_THROUGHS.load(Ordering::Relaxed),
        0,
        "a decoder fed a non-FormID word to remap_starfield (#7096)"
    );
}
