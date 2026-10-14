//! #5010 — BGSM/BGEM parse diagnostics reach the log.
//!
//! The chain resolver used to parse every leaf and template with the
//! non-diagnostic `parse_bgsm`, so layout drift, post-v22 content and lossy
//! strings decoded silently on the production path (the warnings lived only
//! in the dormant depth-limit recovery arm). The resolver hook itself is
//! pinned in `crates/bgsm` (`resolve_reports_per_file_parse_diagnostics`);
//! this pins what the provider says about what it is handed.

use super::super::*;
use byroredux_bgsm::ParseDiagnostics;

#[test]
fn clean_parse_is_silent() {
    assert!(
        material::parse_diagnostic_messages("BGSM", "materials\\a.bgsm", &ParseDiagnostics::default())
            .is_empty()
    );
}

#[test]
fn each_drift_signal_produces_one_named_warning() {
    let diag = ParseDiagnostics {
        lossy_strings: true,
        unconsumed_bytes: 7,
        version_over_ceiling: Some(23),
    };
    let msgs = material::parse_diagnostic_messages("BGSM", "materials\\a.bgsm", &diag);
    assert_eq!(msgs.len(), 3);
    assert!(msgs.iter().all(|m| m.starts_with("BGSM 'materials\\a.bgsm'")));
    assert!(msgs[0].contains("non-UTF-8"));
    assert!(msgs[1].contains("7 byte(s) left unconsumed"));
    assert!(msgs[2].contains("version 23"));
}

#[test]
fn bgem_kind_is_named_in_the_message() {
    let diag = ParseDiagnostics {
        unconsumed_bytes: 1,
        ..ParseDiagnostics::default()
    };
    let msgs = material::parse_diagnostic_messages("BGEM", "x.bgem", &diag);
    assert_eq!(msgs.len(), 1);
    assert!(msgs[0].starts_with("BGEM 'x.bgem'"));
}
