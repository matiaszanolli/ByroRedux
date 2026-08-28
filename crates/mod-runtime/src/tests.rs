use crate::{
    CapabilitySet, FaultKind, InstanceStatus, LifecyclePhase, LogLevel, Principal, PrincipalId,
    SandboxConfig, SandboxError, SandboxRuntime, LOG_CAPABILITY,
};

const IMPORTS: &str = r#"
    (import "byro:mod-host/logging@0.1.0" (instance $logging
        (type $level-shape (enum "debug" "info" "warn" "error"))
        (export "level" (type $level (eq $level-shape)))
        (export "log" (func (param "level" $level) (param "message" string)))
    ))
    (import "byro:mod-host/context@0.1.0" (instance $context
        (export "principal-id" (func (result string)))
        (export "has-capability" (func (param "capability" string) (result bool)))
    ))
"#;

fn principal() -> Principal {
    Principal::new(
        PrincipalId::new("org.byroredux.tests.lifecycle").unwrap(),
        "Lifecycle test mod",
    )
    .unwrap()
}

fn runtime(config: SandboxConfig) -> SandboxRuntime {
    SandboxRuntime::new(config).unwrap()
}

fn compile_wat(runtime: &SandboxRuntime, source: &str) -> crate::CompiledMod {
    let bytes = wat::parse_str(source).unwrap();
    runtime.compile(&bytes).unwrap()
}

fn logging_component() -> String {
    format!(
        r#"(component
            {IMPORTS}
            (alias export $logging "log" (func $log))

            (core module $libc
                (memory (export "memory") 1)
                (func (export "realloc") (param i32 i32 i32 i32) (result i32)
                    unreachable)
            )
            (core instance $libc (instantiate $libc))
            (core func $log-lower
                (canon lower (func $log)
                    (memory $libc "memory")
                    (realloc (func $libc "realloc")))
            )
            (core module $guest
                (import "libc" "memory" (memory 1))
                (import "host" "log" (func $log (param i32 i32 i32)))

                (data (i32.const 0) "initialized")
                (data (i32.const 32) "shutdown")

                (func (export "initialize")
                    i32.const 1
                    i32.const 0
                    i32.const 11
                    call $log)

                (func (export "shutdown")
                    i32.const 1
                    i32.const 32
                    i32.const 8
                    call $log)
            )
            (core instance $guest-instance (instantiate $guest
                (with "libc" (instance $libc))
                (with "host" (instance (export "log" (func $log-lower))))
            ))
            (func (export "initialize")
                (canon lift (core func $guest-instance "initialize")))
            (func (export "shutdown")
                (canon lift (core func $guest-instance "shutdown")))
        )"#
    )
}

fn looping_component() -> String {
    format!(
        r#"(component
            {IMPORTS}
            (core module $guest
                (func (export "initialize")
                    (loop $forever
                        i32.const 1
                        drop
                        br $forever))
                (func (export "shutdown"))
            )
            (core instance $guest-instance (instantiate $guest))
            (func (export "initialize")
                (canon lift (core func $guest-instance "initialize")))
            (func (export "shutdown")
                (canon lift (core func $guest-instance "shutdown")))
        )"#
    )
}

fn oversized_memory_component() -> String {
    format!(
        r#"(component
            {IMPORTS}
            (core module $guest
                (memory 2)
                (func (export "initialize"))
                (func (export "shutdown"))
            )
            (core instance $guest-instance (instantiate $guest))
            (func (export "initialize")
                (canon lift (core func $guest-instance "initialize")))
            (func (export "shutdown")
                (canon lift (core func $guest-instance "shutdown")))
        )"#
    )
}

fn component_with_wasi_import() -> String {
    format!(
        r#"(component
            {IMPORTS}
            (import "wasi:random/random@0.2.0" (instance
                (export "get-random-u64" (func (result u64)))
            ))
            (core module $guest
                (func (export "initialize"))
                (func (export "shutdown"))
            )
            (core instance $guest-instance (instantiate $guest))
            (func (export "initialize")
                (canon lift (core func $guest-instance "initialize")))
            (func (export "shutdown")
                (canon lift (core func $guest-instance "shutdown")))
        )"#
    )
}

#[test]
fn lifecycle_calls_are_capability_gated_and_attributed() {
    let runtime = runtime(SandboxConfig::default());
    let compiled = compile_wat(&runtime, &logging_component());
    let mut grants = CapabilitySet::new();
    grants.grant(LOG_CAPABILITY).unwrap();
    let mut instance = runtime.instantiate(&compiled, principal(), grants).unwrap();

    assert_eq!(instance.status(), &InstanceStatus::Ready);
    instance.initialize().unwrap();
    assert_eq!(instance.status(), &InstanceStatus::Active);
    instance.shutdown().unwrap();
    assert_eq!(instance.status(), &InstanceStatus::Stopped);

    assert_eq!(instance.logs().len(), 2);
    assert_eq!(instance.logs()[0].level, LogLevel::Info);
    assert_eq!(instance.logs()[0].message, "initialized");
    assert_eq!(instance.logs()[1].message, "shutdown");
    assert!(instance
        .logs()
        .iter()
        .all(|entry| entry.principal == *principal().id()));
}

#[test]
fn denied_host_call_quarantines_only_its_instance() {
    let runtime = runtime(SandboxConfig::default());
    let compiled = compile_wat(&runtime, &logging_component());
    let mut denied = runtime
        .instantiate(&compiled, principal(), CapabilitySet::new())
        .unwrap();

    let error = denied.initialize().unwrap_err();
    assert!(matches!(
        error,
        SandboxError::GuestFault {
            phase: LifecyclePhase::Initialize,
            ..
        }
    ));
    assert!(matches!(
        denied.status(),
        InstanceStatus::Quarantined(fault)
            if fault.phase == LifecyclePhase::Initialize
                && fault.message.contains(LOG_CAPABILITY)
    ));

    let mut grants = CapabilitySet::new();
    grants.grant(LOG_CAPABILITY).unwrap();
    let mut unrelated = runtime.instantiate(&compiled, principal(), grants).unwrap();
    unrelated.initialize().unwrap();
    assert_eq!(unrelated.status(), &InstanceStatus::Active);
}

#[test]
fn fuel_exhaustion_quarantines_runaway_guest() {
    let config = SandboxConfig {
        fuel_per_entry: 1_000,
        ..SandboxConfig::default()
    };
    let runtime = runtime(config);
    let compiled = compile_wat(&runtime, &looping_component());
    let mut instance = runtime
        .instantiate(&compiled, principal(), CapabilitySet::new())
        .unwrap();

    let error = instance.initialize().unwrap_err();
    assert!(matches!(error, SandboxError::GuestFault { .. }));
    assert!(matches!(instance.status(), InstanceStatus::Quarantined(_)));
    assert_eq!(instance.fuel_remaining(), 0);
}

#[test]
fn memory_ceiling_is_enforced_during_instantiation() {
    let config = SandboxConfig {
        max_memory_bytes: 64 * 1024,
        ..SandboxConfig::default()
    };
    let runtime = runtime(config);
    let compiled = compile_wat(&runtime, &oversized_memory_component());
    let result = runtime.instantiate(&compiled, principal(), CapabilitySet::new());

    assert!(matches!(result, Err(SandboxError::Instantiate(_))));
}

#[test]
fn log_size_limit_is_enforced_at_the_host_boundary() {
    let config = SandboxConfig {
        max_log_message_bytes: 4,
        max_log_bytes: 64,
        ..SandboxConfig::default()
    };
    let runtime = runtime(config);
    let compiled = compile_wat(&runtime, &logging_component());
    let mut grants = CapabilitySet::new();
    grants.grant(LOG_CAPABILITY).unwrap();
    let mut instance = runtime.instantiate(&compiled, principal(), grants).unwrap();

    assert!(matches!(
        instance.initialize(),
        Err(SandboxError::GuestFault { .. })
    ));
    assert!(matches!(instance.status(), InstanceStatus::Quarantined(_)));
    assert!(instance.logs().is_empty());
}

#[test]
fn wasi_imports_are_absent_by_default() {
    let runtime = runtime(SandboxConfig::default());
    let compiled = compile_wat(&runtime, &component_with_wasi_import());
    let result = runtime.instantiate(&compiled, principal(), CapabilitySet::new());

    assert!(matches!(
        result,
        Err(SandboxError::Instantiate(message))
            if message.contains("wasi:random/random@0.2.0")
    ));
}

/// #3050 — the log budget bounds what the host is *holding*, not what the
/// guest may say over its life. A consumer that drains gives the budget back,
/// so a well-behaved mod cannot be quarantined for running long enough.
#[test]
fn draining_logs_returns_budget_and_keeps_the_guest_healthy() {
    // One retained entry, and only enough bytes for one message: both budgets
    // are exhausted by `initialize` alone.
    let config = SandboxConfig {
        max_log_entries: 1,
        max_log_bytes: 15,
        max_log_message_bytes: 15,
        ..SandboxConfig::default()
    };
    let runtime = runtime(config);
    let compiled = compile_wat(&runtime, &logging_component());
    let mut grants = CapabilitySet::new();
    grants.grant(LOG_CAPABILITY).unwrap();
    let mut instance = runtime.instantiate(&compiled, principal(), grants).unwrap();

    instance.initialize().unwrap();
    assert_eq!(instance.logs().len(), 1);

    // Draining hands the entries over AND returns the budget.
    let drained = instance.take_logs();
    assert_eq!(drained.len(), 1);
    assert_eq!(drained[0].message, "initialized");
    assert!(instance.logs().is_empty());

    // The second lifecycle call logs again and the guest stays healthy —
    // pre-fix this was `GuestFault` / `Quarantined`, purely because the first
    // message was still being retained.
    instance.shutdown().unwrap();
    assert_eq!(instance.status(), &InstanceStatus::Stopped);
    let drained = instance.take_logs();
    assert_eq!(drained.len(), 1);
    assert_eq!(drained[0].message, "shutdown");
}

/// The backstop is intact: an owner that never drains still cannot let the
/// retained set grow without bound, and the quarantine that results says so.
#[test]
fn an_undrained_log_budget_still_quarantines_but_names_itself() {
    let config = SandboxConfig {
        max_log_entries: 1,
        ..SandboxConfig::default()
    };
    let runtime = runtime(config);
    let compiled = compile_wat(&runtime, &logging_component());
    let mut grants = CapabilitySet::new();
    grants.grant(LOG_CAPABILITY).unwrap();
    let mut instance = runtime.instantiate(&compiled, principal(), grants).unwrap();

    instance.initialize().unwrap();
    assert!(matches!(
        instance.shutdown(),
        Err(SandboxError::GuestFault { .. })
    ));
    // #3050 DISTINGUISHABLE — a budget overrun is not a guest fault, and the
    // retained `FaultInfo` has to say which one an operator is looking at.
    match instance.status() {
        InstanceStatus::Quarantined(fault) => {
            assert_eq!(fault.kind, FaultKind::LogBudgetExhausted);
            assert_eq!(fault.phase, LifecyclePhase::Shutdown);
            assert!(
                fault.message.contains("take_logs"),
                "the fault should point at the drain: {}",
                fault.message
            );
        }
        other => panic!("expected a quarantine, got {other}"),
    }
}

/// A real guest fault must keep reporting as one — the flag set on the budget
/// path must not leak into the next failure.
#[test]
fn a_genuine_fault_is_not_labelled_a_budget_overrun() {
    let runtime = runtime(SandboxConfig::default());
    let compiled = compile_wat(&runtime, &looping_component());
    let mut instance = runtime
        .instantiate(&compiled, principal(), CapabilitySet::new())
        .unwrap();

    assert!(matches!(
        instance.initialize(),
        Err(SandboxError::GuestFault { .. })
    ));
    match instance.status() {
        InstanceStatus::Quarantined(fault) => assert_eq!(fault.kind, FaultKind::Guest),
        other => panic!("expected a quarantine, got {other}"),
    }
}

/// #3050 — an oversized single message is the guest breaking a per-call
/// contract, not a budget it could get back by draining. It must stay a guest
/// fault however much budget is free.
#[test]
fn an_oversized_message_is_a_guest_fault_not_a_budget_overrun() {
    let config = SandboxConfig {
        max_log_message_bytes: 4,
        ..SandboxConfig::default()
    };
    let runtime = runtime(config);
    let compiled = compile_wat(&runtime, &logging_component());
    let mut grants = CapabilitySet::new();
    grants.grant(LOG_CAPABILITY).unwrap();
    let mut instance = runtime.instantiate(&compiled, principal(), grants).unwrap();

    assert!(matches!(
        instance.initialize(),
        Err(SandboxError::GuestFault { .. })
    ));
    match instance.status() {
        InstanceStatus::Quarantined(fault) => assert_eq!(fault.kind, FaultKind::Guest),
        other => panic!("expected a quarantine, got {other}"),
    }
}

/// #3051 — `compile` is the first thing untrusted bytes touch, and nothing
/// asserted that hostile input produces a clean `Err` rather than a panic.
/// Every case here is a rejection the caller can handle; a panic would cross
/// the trust boundary and take the host down with the mod.
#[test]
fn compile_rejects_hostile_input_without_panicking() {
    let runtime = runtime(SandboxConfig::default());

    // A valid component, truncated at every prefix length. Each is a
    // plausible-but-malformed input of exactly the shape a partial download or
    // a deliberately-clipped file produces.
    let valid = wat::parse_str(&logging_component()).unwrap();
    assert!(runtime.compile(&valid).is_ok(), "the fixture must compile");
    let mut rejected = 0usize;
    for cut in 0..valid.len() {
        // Calling at all is half the assertion: a panic here fails the test.
        if runtime.compile(&valid[..cut]).is_err() {
            rejected += 1;
        }
    }
    // A bare 8-byte component header is a *valid empty component*, so a
    // handful of short prefixes legitimately compile — it is `instantiate`
    // that rejects them for exporting no lifecycle functions. Everything
    // that cuts into real content must be refused.
    assert!(
        rejected > valid.len() - 32,
        "only {rejected} of {} truncations were rejected",
        valid.len()
    );
    assert!(runtime.compile(&valid[..8]).is_ok());

    for (label, bytes) in [
        ("empty", Vec::new()),
        (
            "ascii garbage",
            b"this is not a wasm component at all".to_vec(),
        ),
        ("nul bytes", vec![0u8; 256]),
        ("high bytes", vec![0xFFu8; 256]),
        // Correct magic + version, nothing after it.
        ("bare core header", b"\0asm\x01\0\0\0".to_vec()),
        // A section id with a length that runs past the end.
        (
            "oversized section length",
            b"\0asm\x0d\0\x01\0\x01\xff\xff\xff\x7f".to_vec(),
        ),
    ] {
        assert!(
            runtime.compile(&bytes).is_err(),
            "{label} compiled instead of being rejected"
        );
    }
}

/// A *core* module is valid wasm and not a component. Rejecting it is the
/// least-obvious of the negative cases — the bytes parse, the magic is right,
/// and only the component-model layer check separates them (#3051).
#[test]
fn compile_rejects_a_valid_core_module_that_is_not_a_component() {
    let runtime = runtime(SandboxConfig::default());
    let core = wat::parse_str(r#"(module (func (export "initialize")))"#).unwrap();
    assert!(core.starts_with(b"\0asm"), "fixture must be real wasm");

    let error = runtime.compile(&core).unwrap_err();
    assert!(
        matches!(error, SandboxError::Compile(_)),
        "expected a compile rejection, got {error:?}"
    );
}

#[test]
fn component_byte_limit_is_checked_before_compilation() {
    let runtime = runtime(SandboxConfig {
        max_component_bytes: 4,
        ..SandboxConfig::default()
    });
    let error = runtime.compile(b"not wasm").unwrap_err();

    assert!(matches!(
        error,
        SandboxError::ComponentTooLarge {
            actual: 8,
            maximum: 4
        }
    ));
}
