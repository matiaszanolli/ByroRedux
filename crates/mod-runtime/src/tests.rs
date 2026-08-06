use crate::{
    CapabilitySet, InstanceStatus, LifecyclePhase, LogLevel, Principal, PrincipalId, SandboxConfig,
    SandboxError, SandboxRuntime, LOG_CAPABILITY,
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
