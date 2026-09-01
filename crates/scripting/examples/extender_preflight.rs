//! Report extender-era calls in loose Papyrus source, compiled scripts, or
//! game/mod script archives.
//!
//! Usage:
//! `cargo run -p byroredux-scripting --example extender_preflight -- script.psc script.pex mod.ba2`

use std::{env, fs, path::Path, process::ExitCode};

use byroredux_bsa::{Ba2Archive, BsaArchive};
use byroredux_pex::{CallScope, CallTarget};
use byroredux_scripting::{analyze_pex_compatibility, analyze_source_compatibility};
use byroredux_sdk::compatibility::{CompatibilityDisposition, CompatibilityMatch};

#[derive(Default)]
struct Totals {
    native: usize,
    mapped: usize,
    unsupported: usize,
    malformed: usize,
    input_errors: usize,
}

fn main() -> ExitCode {
    let paths: Vec<_> = env::args_os().skip(1).collect();
    if paths.is_empty() {
        eprintln!("usage: extender_preflight <script.psc|script.pex> [...]");
        return ExitCode::from(1);
    }

    let mut totals = Totals::default();
    for path in paths {
        let path = Path::new(&path);
        match path
            .extension()
            .and_then(|extension| extension.to_str())
            .map(str::to_ascii_lowercase)
            .as_deref()
        {
            Some("psc") => scan_source(path, &mut totals),
            Some("pex") => scan_compiled(path, &mut totals),
            Some("bsa") => scan_bsa(path, &mut totals),
            Some("ba2") => scan_ba2(path, &mut totals),
            _ => {
                eprintln!(
                    "{}: expected a .psc, .pex, .bsa, or .ba2 input",
                    path.display()
                );
                totals.input_errors += 1;
            }
        }
    }

    println!(
        "summary: native={} mapped={} unsupported={} malformed={} input-errors={}",
        totals.native, totals.mapped, totals.unsupported, totals.malformed, totals.input_errors
    );
    if totals.input_errors != 0 {
        ExitCode::from(1)
    } else if totals.unsupported != 0 || totals.malformed != 0 {
        ExitCode::from(2)
    } else {
        ExitCode::SUCCESS
    }
}

fn scan_source(path: &Path, totals: &mut Totals) {
    let source = match fs::read_to_string(path) {
        Ok(source) => source,
        Err(error) => {
            eprintln!("{}: {error}", path.display());
            totals.input_errors += 1;
            return;
        }
    };
    let (script, errors) = match byroredux_papyrus::parse_script(&source) {
        Ok(parsed) => parsed,
        Err(errors) => {
            for error in errors {
                eprintln!("{}: parse error: {error}", path.display());
            }
            totals.input_errors += 1;
            return;
        }
    };
    if !errors.is_empty() {
        for error in errors {
            eprintln!("{}: parse error: {error}", path.display());
        }
        totals.input_errors += 1;
    }

    let report = analyze_source_compatibility(&script, &source);
    totals.native += report.native_count();
    totals.mapped += report.mapped_count();
    totals.unsupported += report.unsupported_count();
    for finding in report.findings {
        print_finding(
            &format!(
                "{}:{} [{}]",
                path.display(),
                finding.source_line,
                finding.scope
            ),
            finding.provider.as_deref(),
            &finding.function,
            finding.argument_count,
            finding.compatibility,
        );
    }
}

fn scan_compiled(path: &Path, totals: &mut Totals) {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) => {
            eprintln!("{}: {error}", path.display());
            totals.input_errors += 1;
            return;
        }
    };
    scan_compiled_bytes(&path.display().to_string(), &bytes, totals);
}

fn scan_bsa(path: &Path, totals: &mut Totals) {
    match BsaArchive::open(path) {
        Ok(archive) => scan_archive(
            path,
            archive.list_files(),
            |name| archive.extract(name),
            totals,
        ),
        Err(error) => archive_error(path, error, totals),
    }
}

fn scan_ba2(path: &Path, totals: &mut Totals) {
    match Ba2Archive::open(path) {
        Ok(archive) => scan_archive(
            path,
            archive.list_files(),
            |name| archive.extract(name),
            totals,
        ),
        Err(error) => archive_error(path, error, totals),
    }
}

fn scan_archive(
    path: &Path,
    files: Vec<&str>,
    mut extract: impl FnMut(&str) -> std::io::Result<Vec<u8>>,
    totals: &mut Totals,
) {
    for name in files
        .into_iter()
        .filter(|name| name.to_ascii_lowercase().ends_with(".pex"))
    {
        match extract(name) {
            Ok(bytes) => {
                scan_compiled_bytes(&format!("{}::{name}", path.display()), &bytes, totals)
            }
            Err(error) => {
                eprintln!("{}::{name}: {error}", path.display());
                totals.input_errors += 1;
            }
        }
    }
}

fn archive_error(path: &Path, error: std::io::Error, totals: &mut Totals) {
    eprintln!("{}: {error}", path.display());
    totals.input_errors += 1;
}

fn scan_compiled_bytes(label: &str, bytes: &[u8], totals: &mut Totals) {
    let pex = match byroredux_pex::parse(bytes) {
        Ok(pex) => pex,
        Err(error) => {
            eprintln!("{label}: {error}");
            totals.input_errors += 1;
            return;
        }
    };
    let report = analyze_pex_compatibility(&pex);
    totals.native += report.native_count();
    totals.mapped += report.mapped_count();
    totals.unsupported += report.unsupported_count();
    totals.malformed += report.malformed_calls.len();
    for finding in report.findings {
        let line = finding.call.source_line.map_or_else(
            || format!("instruction-{}", finding.call.instruction_index),
            |line| line.to_string(),
        );
        let scope = scope_label(&finding.call.scope);
        let provider = match &finding.call.target {
            CallTarget::StaticType(provider) | CallTarget::ParentType(provider) => {
                Some(provider.as_str())
            }
            CallTarget::Receiver(receiver) => receiver.as_deref(),
        };
        print_finding(
            &format!("{label}:{line} [{scope}]"),
            provider,
            &finding.call.function,
            finding.call.argument_count,
            finding.compatibility,
        );
    }
    for diagnostic in report.malformed_calls {
        let line = diagnostic.source_line.map_or_else(
            || format!("instruction-{}", diagnostic.instruction_index),
            |line| line.to_string(),
        );
        eprintln!(
            "{label}:{line} [{}]: malformed: {}",
            scope_label(&diagnostic.scope),
            diagnostic.message
        );
    }
}

fn print_finding(
    location: &str,
    provider: Option<&str>,
    function: &str,
    argument_count: usize,
    compatibility: CompatibilityMatch,
) {
    let disposition = match compatibility.disposition {
        CompatibilityDisposition::Native => "native",
        CompatibilityDisposition::Mapped => "mapped",
        CompatibilityDisposition::Unsupported => "unsupported",
    };
    let call = provider.map_or_else(
        || function.to_owned(),
        |provider| format!("{provider}.{function}"),
    );
    let service = compatibility
        .service
        .map_or(String::new(), |service| format!(" -> {service}"));
    println!(
        "{location}: {disposition}: {call}/{argument_count}{service}: {}",
        compatibility.guidance
    );
}

fn scope_label(scope: &CallScope) -> String {
    match scope {
        CallScope::StateFunction { state, function } if state.is_empty() => function.clone(),
        CallScope::StateFunction { state, function } => format!("{state}::{function}"),
        CallScope::PropertyGetter { property } => format!("get:{property}"),
        CallScope::PropertySetter { property } => format!("set:{property}"),
    }
}
