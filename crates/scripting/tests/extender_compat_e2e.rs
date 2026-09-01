//! Opt-in compatibility preflight against real, unmodified mod bytecode.
//!
//! Run with:
//! `cargo test -p byroredux-scripting --test extender_compat_e2e -- --ignored`
//! The test skips cleanly when the locally installed fixture is absent.

use byroredux_bsa::Ba2Archive;
use byroredux_pex::{CallSite, CallTarget};
use byroredux_scripting::analyze_pex_compatibility;
use byroredux_sdk::compatibility::CompatibilityDisposition;

const WORKSHOP_FRAMEWORK_ARCHIVE: &str = "/mnt/data/SteamLibrary/steamapps/common/Fallout 4/\
Data/workshopframework - main.ba2";

fn extract_script(archive: &Ba2Archive, suffix: &str) -> Option<Vec<u8>> {
    let suffix = suffix.to_ascii_lowercase();
    let path = archive
        .list_files()
        .into_iter()
        .find(|path| path.to_ascii_lowercase().ends_with(&suffix))?;
    archive.extract(path).ok()
}

fn provider(call: &CallSite) -> Option<&str> {
    match &call.target {
        CallTarget::StaticType(provider) | CallTarget::ParentType(provider) => Some(provider),
        CallTarget::Receiver(receiver) => receiver.as_deref(),
    }
}

#[test]
#[ignore = "needs the Workshop Framework mod archive on disk"]
fn workshop_framework_reports_real_f4se_dependencies_and_policy_gaps() {
    let Ok(archive) = Ba2Archive::open(WORKSHOP_FRAMEWORK_ARCHIVE) else {
        eprintln!("SKIP: Workshop Framework archive not found");
        return;
    };

    let manager = extract_script(&archive, "scripts\\workshopframework\\f4semanager.pex")
        .expect("Workshop Framework F4SE manager fixture");
    let manager = byroredux_pex::parse(&manager).expect("parse real F4SE manager PEX");
    let manager_report = analyze_pex_compatibility(&manager);
    assert!(manager_report.malformed_calls.is_empty());
    assert_eq!(manager_report.mapped_count(), 5);
    assert_eq!(manager_report.unsupported_count(), 0);
    assert!(manager_report.findings.iter().all(|finding| {
        provider(&finding.call) == Some("f4se")
            && finding.compatibility.disposition == CompatibilityDisposition::Mapped
            && finding.call.source_line.is_some()
    }));

    let utility = extract_script(&archive, "scripts\\wsfw_utility.pex")
        .expect("Workshop Framework utility fixture");
    let utility = byroredux_pex::parse(&utility).expect("parse real utility PEX");
    let utility_report = analyze_pex_compatibility(&utility);
    assert!(utility_report.malformed_calls.is_empty());
    assert_eq!(utility_report.unsupported_count(), 2);
    assert!(utility_report.findings.iter().any(|finding| {
        provider(&finding.call) == Some("ui")
            && finding
                .call
                .function
                .eq_ignore_ascii_case("IsMenuRegistered")
            && finding.compatibility.disposition == CompatibilityDisposition::Unsupported
    }));
    assert!(utility_report.findings.iter().any(|finding| {
        provider(&finding.call) == Some("input")
            && finding.call.function.eq_ignore_ascii_case("GetMappedKey")
            && finding.compatibility.disposition == CompatibilityDisposition::Unsupported
    }));
}
