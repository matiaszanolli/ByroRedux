use crate::{
    CapabilitySet, FaultKind, InstanceStatus, LifecyclePhase, LogLevel, PrincipalId, SandboxConfig,
    SandboxError, SandboxRuntime, LOG_CAPABILITY,
};
use byroredux_sdk::component::{
    ComponentFieldDeclaration, ComponentSchema, ComponentStoreLimits, ExtensionComponentStore,
    ExtensionValue, ExtensionValueType,
};
use byroredux_sdk::event::{ActivationEvent, CellLoadEvent, EquipmentEvent, HitEvent, UpdateEvent};
use byroredux_sdk::identity::{CapabilityId, ComponentId, ExtensionId, FormRef, ServiceId};
use byroredux_sdk::identity::{ComponentFieldId, ComponentSchemaId, EntityRef};
use byroredux_sdk::manifest::{
    CapabilityRequest, ComponentSchemaDeclaration, EventSubscription, ExecutableComponent,
    ExtensionManifest, EXTENSION_MANIFEST_VERSION,
};
use byroredux_sdk::projection::{EntityProjection, WorldTransform};
use byroredux_sdk::service::{
    CompatibilityError, ACTIVATE_EVENT, CELL_LOAD_EVENT, COMPONENTS_WRITE_OWN_CAPABILITY,
    EQUIPMENT_EVENT, EVENTS_SUBSCRIBE_CAPABILITY, HIT_EVENT, LOGGING_SERVICE,
    STORAGE_READ_OWN_CAPABILITY, STORAGE_WRITE_OWN_CAPABILITY, UPDATE_EVENT,
    WORLD_ENTITY_READ_CAPABILITY,
};
use byroredux_sdk::storage::{HostCommand, PrincipalStorageLimits, PrincipalStorageStore};
use semver::{Version, VersionReq};

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
    (import "byro:mod-host/state@0.1.0" (instance $state
        (type $entity-ref-shape (record
            (field "world-generation" u64)
            (field "object" u64)))
        (export "entity-ref" (type $entity-ref (eq $entity-ref-shape)))
        (type $form-ref-shape (record
            (field "source-high" u64)
            (field "source-low" u64)
            (field "local" u32)))
        (export "form-ref" (type $form-ref-in (eq $form-ref-shape)))
        (type $hit-details-shape (record
            (field "damage" f32)
            (field "power-attack" bool)
            (field "sneak-attack" bool)
            (field "bash-attack" bool)
            (field "blocked" bool)))
        (export "hit-details" (type $hit-details-in (eq $hit-details-shape)))
        (export "queue-increment-own-i64" (func
            (param "entity" $entity-ref)
            (param "schema-index" u32)
            (param "field-index" u32)
            (param "delta" s64)))
    ))
    (alias export $state "entity-ref" (type $entity-ref))
    (alias export $state "form-ref" (type $form-ref))
    (alias export $state "hit-details" (type $hit-details))
"#;

const ON_ACTIVATE_CORE: &str = r#"
                (func (export "on-activate")
                    (param i64 i64 i32 i64 i64))
"#;

const ON_ACTIVATE_LIFT: &str = r#"
            (func (export "on-activate")
                (param "subject" $entity-ref)
                (param "activator" (option $entity-ref))
                (canon lift (core func $guest-instance "on-activate")))
"#;

const ON_CELL_LOAD_CORE: &str = r#"
                (func (export "on-cell-load")
                    (param i64 i64))
"#;

const ON_CELL_LOAD_LIFT: &str = r#"
            (func (export "on-cell-load")
                (param "subject" $entity-ref)
                (canon lift (core func $guest-instance "on-cell-load")))
"#;

const ON_HIT_CORE: &str = r#"
                (func (export "on-hit")
                    (param i64 i64)
                    (param i32 i64 i64)
                    (param i32 i64 i64)
                    (param i32 i64 i64)
                    (param f32 i32 i32 i32 i32))
"#;

const ON_HIT_LIFT: &str = r#"
            (func (export "on-hit")
                (param "subject" $entity-ref)
                (param "aggressor" (option $entity-ref))
                (param "source" (option $entity-ref))
                (param "projectile" (option $entity-ref))
                (param "details" $hit-details)
                (canon lift (core func $guest-instance "on-hit")))
"#;

const ON_EQUIPMENT_CORE: &str = r#"
                (func (export "on-equipment-change")
                    (param i64 i64 i64 i64 i32 i32))
"#;

const ON_EQUIPMENT_LIFT: &str = r#"
            (func (export "on-equipment-change")
                (param "wearer" $entity-ref)
                (param "item" $form-ref)
                (param "equipped" bool)
                (canon lift (core func $guest-instance "on-equipment-change")))
"#;

const ON_UPDATE_CORE: &str = r#"
                (func (export "on-update") (param f32))
"#;

const ON_UPDATE_LIFT: &str = r#"
            (func (export "on-update")
                (param "elapsed-seconds" f32)
                (canon lift (core func $guest-instance "on-update")))
"#;

fn manifest_with_log(required: bool) -> ExtensionManifest {
    ExtensionManifest {
        manifest_version: EXTENSION_MANIFEST_VERSION,
        id: ExtensionId::new("org.byroredux.tests.lifecycle").unwrap(),
        name: "Lifecycle test mod".to_owned(),
        version: Version::new(1, 0, 0),
        sdk: VersionReq::parse("^0.1").unwrap(),
        dependencies: Vec::new(),
        components: vec![ExecutableComponent {
            id: ComponentId::new("runtime").unwrap(),
            path: "runtime.wasm".to_owned(),
            world: ServiceId::new("byro.mod-host.extension").unwrap(),
            world_version: VersionReq::parse("^0.1").unwrap(),
        }],
        capabilities: vec![CapabilityRequest {
            id: CapabilityId::new(LOG_CAPABILITY).unwrap(),
            required,
        }],
        subscriptions: Vec::new(),
        component_schemas: Vec::new(),
        principal_storage_schema: None,
    }
}

fn manifest() -> ExtensionManifest {
    manifest_with_log(false)
}

fn activation_manifest() -> ExtensionManifest {
    let mut manifest = manifest();
    manifest.capabilities = vec![
        CapabilityRequest {
            id: CapabilityId::new(EVENTS_SUBSCRIBE_CAPABILITY).unwrap(),
            required: true,
        },
        CapabilityRequest {
            id: CapabilityId::new(COMPONENTS_WRITE_OWN_CAPABILITY).unwrap(),
            required: true,
        },
    ];
    manifest.subscriptions = vec![EventSubscription {
        event: byroredux_sdk::identity::EventId::new(ACTIVATE_EVENT).unwrap(),
        filters: Vec::new(),
        interval_millis: None,
    }];
    manifest.component_schemas = vec![ComponentSchemaDeclaration {
        id: ComponentSchemaId::new("org.byroredux.tests.activation-count").unwrap(),
        version: 1,
        fields: vec![ComponentFieldDeclaration {
            id: ComponentFieldId::new("count").unwrap(),
            value_type: ExtensionValueType::I64,
        }],
    }];
    manifest
}

fn cell_load_manifest() -> ExtensionManifest {
    let mut manifest = activation_manifest();
    manifest.subscriptions = vec![EventSubscription {
        event: byroredux_sdk::identity::EventId::new(CELL_LOAD_EVENT).unwrap(),
        filters: Vec::new(),
        interval_millis: None,
    }];
    manifest
}

fn hit_manifest() -> ExtensionManifest {
    let mut manifest = activation_manifest();
    manifest.subscriptions = vec![EventSubscription {
        event: byroredux_sdk::identity::EventId::new(HIT_EVENT).unwrap(),
        filters: Vec::new(),
        interval_millis: None,
    }];
    manifest
}

fn update_manifest() -> ExtensionManifest {
    let mut manifest = principal_storage_manifest();
    manifest.subscriptions = vec![EventSubscription {
        event: byroredux_sdk::identity::EventId::new(UPDATE_EVENT).unwrap(),
        filters: Vec::new(),
        interval_millis: Some(100),
    }];
    manifest
}

fn equipment_manifest() -> ExtensionManifest {
    let mut manifest = principal_storage_manifest();
    manifest.subscriptions = vec![EventSubscription {
        event: byroredux_sdk::identity::EventId::new(EQUIPMENT_EVENT).unwrap(),
        filters: Vec::new(),
        interval_millis: None,
    }];
    manifest
}

fn principal_storage_manifest() -> ExtensionManifest {
    let mut manifest = manifest();
    manifest.capabilities = vec![
        CapabilityRequest {
            id: CapabilityId::new(EVENTS_SUBSCRIBE_CAPABILITY).unwrap(),
            required: true,
        },
        CapabilityRequest {
            id: CapabilityId::new(STORAGE_READ_OWN_CAPABILITY).unwrap(),
            required: true,
        },
        CapabilityRequest {
            id: CapabilityId::new(STORAGE_WRITE_OWN_CAPABILITY).unwrap(),
            required: true,
        },
    ];
    manifest.subscriptions = vec![EventSubscription {
        event: byroredux_sdk::identity::EventId::new(ACTIVATE_EVENT).unwrap(),
        filters: Vec::new(),
        interval_millis: None,
    }];
    manifest.principal_storage_schema = Some(1);
    manifest
}

fn principal_storage_increment_component() -> String {
    r#"(component
            (import "byro:mod-host/state@0.1.0" (instance $state
                (type $entity-ref-shape (record
                    (field "world-generation" u64)
                    (field "object" u64)))
                (export "entity-ref" (type $entity-ref-in (eq $entity-ref-shape)))
                (type $form-ref-shape (record
                    (field "source-high" u64)
                    (field "source-low" u64)
                    (field "local" u32)))
                (export "form-ref" (type $form-ref-in (eq $form-ref-shape)))
                (type $hit-details-shape (record
                    (field "damage" f32)
                    (field "power-attack" bool)
                    (field "sneak-attack" bool)
                    (field "bash-attack" bool)
                    (field "blocked" bool)))
                (export "hit-details" (type $hit-details-in (eq $hit-details-shape)))
            ))
            (import "byro:mod-host/storage@0.1.0" (instance $storage
                (export "queue-increment-i64" (func
                    (param "key" string)
                    (param "delta" s64)))
            ))
            (alias export $state "entity-ref" (type $entity-ref))
            (alias export $state "form-ref" (type $form-ref))
            (alias export $state "hit-details" (type $hit-details))
            (alias export $storage "queue-increment-i64" (func $increment))
            (core module $libc
                (memory (export "memory") 1)
                (func (export "realloc") (param i32 i32 i32 i32) (result i32)
                    unreachable)
            )
            (core instance $libc (instantiate $libc))
            (core func $increment-lower
                (canon lower (func $increment)
                    (memory $libc "memory")
                    (realloc (func $libc "realloc")))
            )
            (core module $guest
                (import "libc" "memory" (memory 1))
                (import "host" "increment" (func $increment (param i32 i32 i64)))
                (data (i32.const 0) "activation-count")
                (func (export "initialize"))
                (func (export "shutdown"))
                (func (export "on-cell-load") (param i64 i64))
                {ON_HIT_CORE}
                (func (export "on-equipment-change")
                    (param i64 i64)
                    (param $source-high i64) (param $source-low i64)
                    (param $local i32) (param $equipped i32)
                    local.get $source-high
                    i64.const 72623859790382856
                    i64.ne
                    local.get $source-low
                    i64.const 651345242494996240
                    i64.ne
                    i32.or
                    local.get $local
                    i32.const 4660
                    i32.ne
                    i32.or
                    local.get $equipped
                    i32.const 1
                    i32.ne
                    i32.or
                    if
                        unreachable
                    end
                    i32.const 0
                    i32.const 16
                    i64.const 1
                    call $increment)
                (func (export "on-update") (param f32)
                    i32.const 0
                    i32.const 16
                    i64.const 1
                    call $increment)
                (func (export "on-activate")
                    (param i64 i64 i32 i64 i64)
                    i32.const 0
                    i32.const 16
                    i64.const 1
                    call $increment)
            )
            (core instance $guest-instance (instantiate $guest
                (with "libc" (instance $libc))
                (with "host" (instance (export "increment" (func $increment-lower))))
            ))
            (func (export "initialize")
                (canon lift (core func $guest-instance "initialize")))
            (func (export "shutdown")
                (canon lift (core func $guest-instance "shutdown")))
            (func (export "on-cell-load")
                (param "subject" $entity-ref)
                (canon lift (core func $guest-instance "on-cell-load")))
            {ON_HIT_LIFT}
            {ON_EQUIPMENT_LIFT}
            {ON_UPDATE_LIFT}
            (func (export "on-activate")
                (param "subject" $entity-ref)
                (param "activator" (option $entity-ref))
                (canon lift (core func $guest-instance "on-activate")))
        )"#
    .replace("{ON_HIT_CORE}", ON_HIT_CORE)
    .replace("{ON_HIT_LIFT}", ON_HIT_LIFT)
    .replace("{ON_EQUIPMENT_CORE}", ON_EQUIPMENT_CORE)
    .replace("{ON_EQUIPMENT_LIFT}", ON_EQUIPMENT_LIFT)
    .replace("{ON_UPDATE_CORE}", ON_UPDATE_CORE)
    .replace("{ON_UPDATE_LIFT}", ON_UPDATE_LIFT)
}

fn entity_projection_component() -> String {
    r#"(component
            (type $entity-ref-shape (record
                (field "world-generation" u64)
                (field "object" u64)))
            (import "byro:mod-host/state@0.1.0" (instance $state
                (export "entity-ref" (type $entity-ref-in (eq $entity-ref-shape)))
                (type $form-ref-shape (record
                    (field "source-high" u64)
                    (field "source-low" u64)
                    (field "local" u32)))
                (export "form-ref" (type $form-ref-in (eq $form-ref-shape)))
                (type $hit-details-shape (record
                    (field "damage" f32)
                    (field "power-attack" bool)
                    (field "sneak-attack" bool)
                    (field "bash-attack" bool)
                    (field "blocked" bool)))
                (export "hit-details" (type $hit-details-in (eq $hit-details-shape)))
            ))
            (import "byro:mod-host/world-state@0.1.0" (instance $world
                (export "entity-ref" (type $entity-ref-world (eq $entity-ref-shape)))
                (export "contains-entity" (func
                    (param "entity" $entity-ref-world)
                    (result bool)))
            ))
            (alias export $state "entity-ref" (type $entity-ref))
            (alias export $state "form-ref" (type $form-ref))
            (alias export $state "hit-details" (type $hit-details))
            (alias export $world "contains-entity" (func $contains))
            (core func $contains-lower (canon lower (func $contains)))
            (core module $guest
                (import "host" "contains" (func $contains (param i64 i64) (result i32)))
                (func (export "initialize"))
                (func (export "shutdown"))
                (func (export "on-cell-load") (param i64 i64))
                {ON_HIT_CORE}
                {ON_EQUIPMENT_CORE}
                {ON_UPDATE_CORE}
                (func (export "on-activate")
                    (param $world i64) (param $object i64) (param i32 i64 i64)
                    local.get $world
                    local.get $object
                    call $contains
                    i32.eqz
                    if
                        unreachable
                    end)
            )
            (core instance $guest-instance (instantiate $guest
                (with "host" (instance (export "contains" (func $contains-lower))))
            ))
            (func (export "initialize")
                (canon lift (core func $guest-instance "initialize")))
            (func (export "shutdown")
                (canon lift (core func $guest-instance "shutdown")))
            (func (export "on-cell-load")
                (param "subject" $entity-ref)
                (canon lift (core func $guest-instance "on-cell-load")))
            {ON_HIT_LIFT}
            {ON_EQUIPMENT_LIFT}
            {ON_UPDATE_LIFT}
            (func (export "on-activate")
                (param "subject" $entity-ref)
                (param "activator" (option $entity-ref))
                (canon lift (core func $guest-instance "on-activate")))
        )"#
    .replace("{ON_HIT_CORE}", ON_HIT_CORE)
    .replace("{ON_HIT_LIFT}", ON_HIT_LIFT)
    .replace("{ON_EQUIPMENT_CORE}", ON_EQUIPMENT_CORE)
    .replace("{ON_EQUIPMENT_LIFT}", ON_EQUIPMENT_LIFT)
    .replace("{ON_UPDATE_CORE}", ON_UPDATE_CORE)
    .replace("{ON_UPDATE_LIFT}", ON_UPDATE_LIFT)
}

fn entity_projection_manifest(required: bool) -> ExtensionManifest {
    let mut manifest = activation_manifest();
    manifest.component_schemas.clear();
    manifest.capabilities = vec![
        CapabilityRequest {
            id: CapabilityId::new(EVENTS_SUBSCRIBE_CAPABILITY).unwrap(),
            required: true,
        },
        CapabilityRequest {
            id: CapabilityId::new(WORLD_ENTITY_READ_CAPABILITY).unwrap(),
            required,
        },
    ];
    manifest
}

fn runtime(config: SandboxConfig) -> SandboxRuntime {
    SandboxRuntime::new(config).unwrap()
}

fn component_id() -> ComponentId {
    ComponentId::new("runtime").unwrap()
}

fn compile_wat(runtime: &SandboxRuntime, source: &str) -> crate::CompiledMod {
    let bytes = wat::parse_str(source).unwrap();
    runtime
        .compile(&manifest(), &component_id(), &bytes)
        .unwrap()
}

fn compile_wat_for(
    runtime: &SandboxRuntime,
    manifest: &ExtensionManifest,
    source: &str,
) -> crate::CompiledMod {
    let bytes = wat::parse_str(source).unwrap();
    runtime.compile(manifest, &component_id(), &bytes).unwrap()
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
                {ON_ACTIVATE_CORE}
                {ON_CELL_LOAD_CORE}
                {ON_HIT_CORE}
                {ON_EQUIPMENT_CORE}
                {ON_UPDATE_CORE}
            )
            (core instance $guest-instance (instantiate $guest
                (with "libc" (instance $libc))
                (with "host" (instance (export "log" (func $log-lower))))
            ))
            (func (export "initialize")
                (canon lift (core func $guest-instance "initialize")))
            (func (export "shutdown")
                (canon lift (core func $guest-instance "shutdown")))
            {ON_ACTIVATE_LIFT}
            {ON_CELL_LOAD_LIFT}
            {ON_HIT_LIFT}
            {ON_EQUIPMENT_LIFT}
            {ON_UPDATE_LIFT}
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
                {ON_ACTIVATE_CORE}
                {ON_CELL_LOAD_CORE}
                {ON_HIT_CORE}
                {ON_EQUIPMENT_CORE}
                {ON_UPDATE_CORE}
            )
            (core instance $guest-instance (instantiate $guest))
            (func (export "initialize")
                (canon lift (core func $guest-instance "initialize")))
            (func (export "shutdown")
                (canon lift (core func $guest-instance "shutdown")))
            {ON_ACTIVATE_LIFT}
            {ON_CELL_LOAD_LIFT}
            {ON_HIT_LIFT}
            {ON_EQUIPMENT_LIFT}
            {ON_UPDATE_LIFT}
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
                {ON_ACTIVATE_CORE}
                {ON_CELL_LOAD_CORE}
                {ON_HIT_CORE}
                {ON_EQUIPMENT_CORE}
                {ON_UPDATE_CORE}
            )
            (core instance $guest-instance (instantiate $guest))
            (func (export "initialize")
                (canon lift (core func $guest-instance "initialize")))
            (func (export "shutdown")
                (canon lift (core func $guest-instance "shutdown")))
            {ON_ACTIVATE_LIFT}
            {ON_CELL_LOAD_LIFT}
            {ON_HIT_LIFT}
            {ON_EQUIPMENT_LIFT}
            {ON_UPDATE_LIFT}
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
                {ON_ACTIVATE_CORE}
                {ON_CELL_LOAD_CORE}
                {ON_HIT_CORE}
                {ON_EQUIPMENT_CORE}
                {ON_UPDATE_CORE}
            )
            (core instance $guest-instance (instantiate $guest))
            (func (export "initialize")
                (canon lift (core func $guest-instance "initialize")))
            (func (export "shutdown")
                (canon lift (core func $guest-instance "shutdown")))
            {ON_ACTIVATE_LIFT}
            {ON_CELL_LOAD_LIFT}
            {ON_HIT_LIFT}
            {ON_EQUIPMENT_LIFT}
            {ON_UPDATE_LIFT}
        )"#
    )
}

fn activation_counter_component(queue_count: usize, trap_after_queue: bool) -> String {
    let tail = if trap_after_queue { "unreachable" } else { "" };
    let queue_calls = r#"
                    local.get $world
                    local.get $object
                    i32.const 0
                    i32.const 0
                    i64.const 1
                    call $increment
"#
    .repeat(queue_count);
    format!(
        r#"(component
            {IMPORTS}
            (alias export $state "queue-increment-own-i64" (func $increment))
            (core func $increment-lower (canon lower (func $increment)))
            (core module $guest
                (import "host" "increment" (func $increment
                    (param i64 i64 i32 i32 i64)))
                (func (export "initialize"))
                (func (export "shutdown"))
                {ON_CELL_LOAD_CORE}
                {ON_HIT_CORE}
                {ON_EQUIPMENT_CORE}
                {ON_UPDATE_CORE}
                (func (export "on-activate")
                    (param $world i64) (param $object i64)
                    (param i32 i64 i64)
                    {queue_calls}
                    {tail})
            )
            (core instance $guest-instance (instantiate $guest
                (with "host" (instance
                    (export "increment" (func $increment-lower))))
            ))
            (func (export "initialize")
                (canon lift (core func $guest-instance "initialize")))
            (func (export "shutdown")
                (canon lift (core func $guest-instance "shutdown")))
            {ON_ACTIVATE_LIFT}
            {ON_CELL_LOAD_LIFT}
            {ON_HIT_LIFT}
            {ON_EQUIPMENT_LIFT}
            {ON_UPDATE_LIFT}
        )"#
    )
}

fn cell_load_counter_component() -> String {
    format!(
        r#"(component
            {IMPORTS}
            (alias export $state "queue-increment-own-i64" (func $increment))
            (core func $increment-lower (canon lower (func $increment)))
            (core module $guest
                (import "host" "increment" (func $increment
                    (param i64 i64 i32 i32 i64)))
                (func (export "initialize"))
                (func (export "shutdown"))
                {ON_ACTIVATE_CORE}
                {ON_HIT_CORE}
                {ON_EQUIPMENT_CORE}
                {ON_UPDATE_CORE}
                (func (export "on-cell-load")
                    (param $world i64) (param $object i64)
                    local.get $world
                    local.get $object
                    i32.const 0
                    i32.const 0
                    i64.const 1
                    call $increment)
            )
            (core instance $guest-instance (instantiate $guest
                (with "host" (instance
                    (export "increment" (func $increment-lower))))
            ))
            (func (export "initialize")
                (canon lift (core func $guest-instance "initialize")))
            (func (export "shutdown")
                (canon lift (core func $guest-instance "shutdown")))
            {ON_ACTIVATE_LIFT}
            {ON_CELL_LOAD_LIFT}
            {ON_HIT_LIFT}
            {ON_EQUIPMENT_LIFT}
            {ON_UPDATE_LIFT}
        )"#
    )
}

fn hit_counter_component() -> String {
    format!(
        r#"(component
            {IMPORTS}
            (alias export $state "queue-increment-own-i64" (func $increment))
            (core func $increment-lower (canon lower (func $increment)))
            (core module $guest
                (import "host" "increment" (func $increment
                    (param i64 i64 i32 i32 i64)))
                (func (export "initialize"))
                (func (export "shutdown"))
                {ON_ACTIVATE_CORE}
                {ON_CELL_LOAD_CORE}
                (func (export "on-hit")
                    (param $world i64) (param $object i64)
                    (param $aggressor-tag i32) (param i64 i64)
                    (param $source-tag i32) (param i64 i64)
                    (param $projectile-tag i32) (param i64 i64)
                    (param $damage f32)
                    (param $power i32) (param $sneak i32)
                    (param $bash i32) (param $blocked i32)
                    local.get $aggressor-tag
                    i32.const 1
                    i32.ne
                    local.get $source-tag
                    i32.const 0
                    i32.ne
                    i32.or
                    local.get $projectile-tag
                    i32.const 0
                    i32.ne
                    i32.or
                    local.get $damage
                    f32.const 12.5
                    f32.ne
                    i32.or
                    local.get $power
                    i32.const 1
                    i32.ne
                    i32.or
                    local.get $sneak
                    i32.const 0
                    i32.ne
                    i32.or
                    local.get $bash
                    i32.const 1
                    i32.ne
                    i32.or
                    local.get $blocked
                    i32.const 0
                    i32.ne
                    i32.or
                    if
                        unreachable
                    end
                    local.get $world
                    local.get $object
                    i32.const 0
                    i32.const 0
                    i64.const 1
                    call $increment)
                {ON_EQUIPMENT_CORE}
                {ON_UPDATE_CORE}
            )
            (core instance $guest-instance (instantiate $guest
                (with "host" (instance
                    (export "increment" (func $increment-lower))))
            ))
            (func (export "initialize")
                (canon lift (core func $guest-instance "initialize")))
            (func (export "shutdown")
                (canon lift (core func $guest-instance "shutdown")))
            {ON_ACTIVATE_LIFT}
            {ON_CELL_LOAD_LIFT}
            {ON_HIT_LIFT}
            {ON_EQUIPMENT_LIFT}
            {ON_UPDATE_LIFT}
        )"#
    )
}

#[test]
fn canonical_cell_load_queues_owned_state_only_for_declared_subscriber() {
    let runtime = runtime(SandboxConfig::default());
    let manifest = cell_load_manifest();
    let compiled = compile_wat_for(&runtime, &manifest, &cell_load_counter_component());
    let mut grants = CapabilitySet::new();
    grants.grant(EVENTS_SUBSCRIBE_CAPABILITY).unwrap();
    grants.grant(COMPONENTS_WRITE_OWN_CAPABILITY).unwrap();
    let mut instance = runtime.instantiate(&compiled, &manifest, grants).unwrap();
    instance.initialize().unwrap();
    let subject = EntityRef::new(1, 41).unwrap();

    let commands = instance.on_cell_load(CellLoadEvent { subject }).unwrap();
    let component_commands = commands
        .into_iter()
        .map(|command| match command {
            HostCommand::Component(command) => command,
            HostCommand::PrincipalStorage(_) => panic!("unexpected principal-storage command"),
        })
        .collect::<Vec<_>>();
    let principal = PrincipalId::from(&manifest.id);
    let mut store = ExtensionComponentStore::new(ComponentStoreLimits::default()).unwrap();
    store
        .register_schema(
            &principal,
            ComponentSchema {
                id: manifest.component_schemas[0].id.clone(),
                version: manifest.component_schemas[0].version,
                fields: manifest.component_schemas[0].fields.clone(),
            },
        )
        .unwrap();
    store.apply_batch(&principal, &component_commands).unwrap();

    let schema = &manifest.component_schemas[0].id;
    assert_eq!(
        store
            .row(&principal, schema, subject)
            .and_then(|row| row.get("count")),
        Some(&ExtensionValue::I64(1))
    );
    assert!(matches!(
        instance.on_activate(ActivationEvent {
            subject,
            activator: None,
        }),
        Err(SandboxError::EventNotSubscribed { .. })
    ));
}

#[test]
fn canonical_hit_preserves_combat_payload_and_queues_owned_state() {
    let runtime = runtime(SandboxConfig::default());
    let manifest = hit_manifest();
    let compiled = compile_wat_for(&runtime, &manifest, &hit_counter_component());
    let mut grants = CapabilitySet::new();
    grants.grant(EVENTS_SUBSCRIBE_CAPABILITY).unwrap();
    grants.grant(COMPONENTS_WRITE_OWN_CAPABILITY).unwrap();
    let mut instance = runtime.instantiate(&compiled, &manifest, grants).unwrap();
    instance.initialize().unwrap();
    let subject = EntityRef::new(1, 41).unwrap();
    let aggressor = EntityRef::new(1, 7).unwrap();

    let commands = instance
        .on_hit(HitEvent {
            subject,
            aggressor: Some(aggressor),
            source: None,
            projectile: None,
            damage: 12.5,
            power_attack: true,
            sneak_attack: false,
            bash_attack: true,
            blocked: false,
        })
        .unwrap();
    let component_commands = commands
        .into_iter()
        .map(|command| match command {
            HostCommand::Component(command) => command,
            HostCommand::PrincipalStorage(_) => panic!("unexpected principal-storage command"),
        })
        .collect::<Vec<_>>();
    let principal = PrincipalId::from(&manifest.id);
    let declaration = &manifest.component_schemas[0];
    let mut store = ExtensionComponentStore::new(ComponentStoreLimits::default()).unwrap();
    store
        .register_schema(
            &principal,
            ComponentSchema {
                id: declaration.id.clone(),
                version: declaration.version,
                fields: declaration.fields.clone(),
            },
        )
        .unwrap();
    store.apply_batch(&principal, &component_commands).unwrap();
    assert_eq!(
        store
            .row(&principal, &declaration.id, subject)
            .and_then(|row| row.get("count")),
        Some(&ExtensionValue::I64(1))
    );
    assert!(matches!(
        instance.on_hit(HitEvent {
            subject,
            aggressor: Some(aggressor),
            source: None,
            projectile: None,
            damage: f32::INFINITY,
            power_attack: false,
            sneak_attack: false,
            bash_attack: false,
            blocked: false,
        }),
        Err(SandboxError::InvalidEventPayload { .. })
    ));
    assert_eq!(instance.status(), &InstanceStatus::Active);
}

#[test]
fn canonical_recurring_update_queues_private_state_and_validates_elapsed_time() {
    let runtime = runtime(SandboxConfig::default());
    let manifest = update_manifest();
    let compiled = compile_wat_for(
        &runtime,
        &manifest,
        &principal_storage_increment_component(),
    );
    let mut grants = CapabilitySet::new();
    grants.grant(EVENTS_SUBSCRIBE_CAPABILITY).unwrap();
    grants.grant(STORAGE_READ_OWN_CAPABILITY).unwrap();
    grants.grant(STORAGE_WRITE_OWN_CAPABILITY).unwrap();
    let mut instance = runtime.instantiate(&compiled, &manifest, grants).unwrap();
    instance.initialize().unwrap();

    let commands = instance
        .on_update(UpdateEvent {
            elapsed_seconds: 0.12,
        })
        .unwrap();
    assert_eq!(commands.len(), 1);
    let principal = PrincipalId::from(&manifest.id);
    let mut storage = PrincipalStorageStore::new(PrincipalStorageLimits::default()).unwrap();
    storage.register_schema(principal.clone(), 1).unwrap();
    let storage_commands = commands
        .into_iter()
        .map(|command| match command {
            HostCommand::PrincipalStorage(command) => command,
            HostCommand::Component(_) => panic!("unexpected component command"),
        })
        .collect::<Vec<_>>();
    storage.apply_batch(&principal, &storage_commands).unwrap();
    assert_eq!(
        storage.values(&principal).and_then(|values| values
            .get(&byroredux_sdk::identity::StorageKey::new("activation-count").unwrap())),
        Some(&ExtensionValue::I64(1))
    );
    assert!(matches!(
        instance.on_update(UpdateEvent {
            elapsed_seconds: f32::NAN,
        }),
        Err(SandboxError::InvalidEventPayload { .. })
    ));
    assert_eq!(instance.status(), &InstanceStatus::Active);
}

#[test]
fn canonical_equipment_change_preserves_portable_item_identity() {
    let runtime = runtime(SandboxConfig::default());
    let manifest = equipment_manifest();
    let compiled = compile_wat_for(
        &runtime,
        &manifest,
        &principal_storage_increment_component(),
    );
    let mut grants = CapabilitySet::new();
    grants.grant(EVENTS_SUBSCRIBE_CAPABILITY).unwrap();
    grants.grant(STORAGE_READ_OWN_CAPABILITY).unwrap();
    grants.grant(STORAGE_WRITE_OWN_CAPABILITY).unwrap();
    let mut instance = runtime.instantiate(&compiled, &manifest, grants).unwrap();
    instance.initialize().unwrap();

    let commands = instance
        .on_equipment_change(EquipmentEvent {
            wearer: EntityRef::new(7, 42).unwrap(),
            item: FormRef::new(
                [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16],
                0x1234,
            ),
            equipped: true,
        })
        .unwrap();
    assert_eq!(commands.len(), 1);
    assert_eq!(instance.status(), &InstanceStatus::Active);

    let mut unsubscribed = equipment_manifest();
    unsubscribed.subscriptions.clear();
    let compiled = compile_wat_for(
        &runtime,
        &unsubscribed,
        &principal_storage_increment_component(),
    );
    let mut grants = CapabilitySet::new();
    grants.grant(EVENTS_SUBSCRIBE_CAPABILITY).unwrap();
    grants.grant(STORAGE_READ_OWN_CAPABILITY).unwrap();
    grants.grant(STORAGE_WRITE_OWN_CAPABILITY).unwrap();
    let mut instance = runtime
        .instantiate(&compiled, &unsubscribed, grants)
        .unwrap();
    instance.initialize().unwrap();
    assert!(matches!(
        instance.on_equipment_change(EquipmentEvent {
            wearer: EntityRef::new(7, 42).unwrap(),
            item: FormRef::new([0; 16], 1),
            equipped: false,
        }),
        Err(SandboxError::EventNotSubscribed(_))
    ));
    assert_eq!(instance.status(), &InstanceStatus::Active);
}

#[test]
fn runtime_catalog_exposes_versioned_services_and_enforceable_capabilities() {
    let runtime = runtime(SandboxConfig::default());
    assert_eq!(runtime.catalog().sdk_version(), &Version::new(0, 1, 0));
    assert_eq!(
        runtime.catalog().service_version(LOGGING_SERVICE),
        Some(&Version::new(0, 1, 0))
    );
    assert!(runtime.catalog().supports_capability(LOG_CAPABILITY));
}

#[test]
fn incompatible_sdk_is_rejected_before_component_bytes_are_compiled() {
    let runtime = runtime(SandboxConfig::default());
    let mut incompatible = manifest();
    incompatible.sdk = VersionReq::parse(">=1.0").unwrap();

    let error = runtime
        .compile(&incompatible, &component_id(), b"not wasm")
        .unwrap_err();
    assert!(matches!(
        error,
        SandboxError::ExtensionContract(CompatibilityError::UnsupportedSdk { .. })
    ));
}

#[test]
fn effective_grants_cannot_exceed_manifest_or_host_authority() {
    let runtime = runtime(SandboxConfig::default());
    let declared = manifest();
    let compiled = compile_wat(&runtime, &looping_component());
    let mut grants = CapabilitySet::new();
    grants.grant("byro.world.raw-memory").unwrap();

    let result = runtime.instantiate(&compiled, &declared, grants);
    assert!(matches!(
        result,
        Err(SandboxError::ExtensionContract(
            CompatibilityError::UndeclaredGrant(_)
        ))
    ));

    let required = manifest_with_log(true);
    let compiled = runtime
        .compile(
            &required,
            &component_id(),
            &wat::parse_str(looping_component()).unwrap(),
        )
        .unwrap();
    let result = runtime.instantiate(&compiled, &required, CapabilitySet::new());
    assert!(matches!(
        result,
        Err(SandboxError::ExtensionContract(
            CompatibilityError::MissingRequiredGrant(_)
        ))
    ));
}

#[test]
fn compiled_artifacts_are_bound_to_declared_component_and_manifest_version() {
    let runtime = runtime(SandboxConfig::default());
    let declared = manifest();
    let undeclared = ComponentId::new("other").unwrap();
    assert!(matches!(
        runtime.compile(&declared, &undeclared, b"not wasm"),
        Err(SandboxError::UndeclaredComponent { .. })
    ));

    let compiled = compile_wat(&runtime, &looping_component());
    let mut changed = declared.clone();
    changed.version = Version::new(2, 0, 0);
    assert!(matches!(
        runtime.instantiate(&compiled, &changed, CapabilitySet::new()),
        Err(SandboxError::ManifestMismatch { .. })
    ));

    let mut same_version_changed_contract = declared.clone();
    same_version_changed_contract.principal_storage_schema = Some(1);
    assert!(matches!(
        runtime.instantiate(
            &compiled,
            &same_version_changed_contract,
            CapabilitySet::new()
        ),
        Err(SandboxError::ManifestMismatch { .. })
    ));
}

#[test]
fn lifecycle_calls_are_capability_gated_and_attributed() {
    let runtime = runtime(SandboxConfig::default());
    let compiled = compile_wat(&runtime, &logging_component());
    let mut grants = CapabilitySet::new();
    grants.grant(LOG_CAPABILITY).unwrap();
    let mut instance = runtime.instantiate(&compiled, &manifest(), grants).unwrap();

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
        .all(|entry| entry.principal == PrincipalId::from(&manifest().id)));
}

#[test]
fn denied_host_call_quarantines_only_its_instance() {
    let runtime = runtime(SandboxConfig::default());
    let compiled = compile_wat(&runtime, &logging_component());
    let mut denied = runtime
        .instantiate(&compiled, &manifest(), CapabilitySet::new())
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
    let mut unrelated = runtime.instantiate(&compiled, &manifest(), grants).unwrap();
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
        .instantiate(&compiled, &manifest(), CapabilitySet::new())
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
    let result = runtime.instantiate(&compiled, &manifest(), CapabilitySet::new());

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
    let mut instance = runtime.instantiate(&compiled, &manifest(), grants).unwrap();

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
    let result = runtime.instantiate(&compiled, &manifest(), CapabilitySet::new());

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
    let mut instance = runtime.instantiate(&compiled, &manifest(), grants).unwrap();

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
    let mut instance = runtime.instantiate(&compiled, &manifest(), grants).unwrap();

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
        .instantiate(&compiled, &manifest(), CapabilitySet::new())
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
    let mut instance = runtime.instantiate(&compiled, &manifest(), grants).unwrap();

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
    let valid = wat::parse_str(logging_component()).unwrap();
    assert!(
        runtime
            .compile(&manifest(), &component_id(), &valid)
            .is_ok(),
        "the fixture must compile"
    );
    let mut rejected = 0usize;
    for cut in 0..valid.len() {
        // Calling at all is half the assertion: a panic here fails the test.
        if runtime
            .compile(&manifest(), &component_id(), &valid[..cut])
            .is_err()
        {
            rejected += 1;
        }
    }
    // A bare 8-byte component header and prefixes ending exactly at component
    // section boundaries are valid smaller components. `instantiate` rejects
    // them for missing lifecycle exports; arbitrary cuts into a section must
    // still be refused. Keep the allowance above the fixture's section count
    // so adding one canonical callback does not make this fuzz smoke brittle.
    assert!(
        rejected > valid.len() - 64,
        "only {rejected} of {} truncations were rejected",
        valid.len()
    );
    assert!(runtime
        .compile(&manifest(), &component_id(), &valid[..8])
        .is_ok());

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
            runtime
                .compile(&manifest(), &component_id(), &bytes)
                .is_err(),
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

    let error = runtime
        .compile(&manifest(), &component_id(), &core)
        .unwrap_err();
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
    let error = runtime
        .compile(&manifest(), &component_id(), b"not wasm")
        .unwrap_err();

    assert!(matches!(
        error,
        SandboxError::ComponentTooLarge {
            actual: 8,
            maximum: 4
        }
    ));
}

#[test]
fn activation_fixture_increments_principal_owned_state_via_deferred_batch() {
    let runtime = runtime(SandboxConfig::default());
    let manifest = activation_manifest();
    let compiled = compile_wat_for(&runtime, &manifest, &activation_counter_component(1, false));
    let mut grants = CapabilitySet::new();
    grants.grant(EVENTS_SUBSCRIBE_CAPABILITY).unwrap();
    grants.grant(COMPONENTS_WRITE_OWN_CAPABILITY).unwrap();
    let mut instance = runtime.instantiate(&compiled, &manifest, grants).unwrap();
    instance.initialize().unwrap();

    let subject = EntityRef::new(3, 41).unwrap();
    let commands = instance
        .on_activate(ActivationEvent {
            subject,
            activator: Some(EntityRef::new(3, 1).unwrap()),
        })
        .unwrap();
    assert_eq!(commands.len(), 1);

    let owner = instance.principal().id().clone();
    let declaration = &manifest.component_schemas[0];
    let mut state = ExtensionComponentStore::new(ComponentStoreLimits::default()).unwrap();
    state
        .register_schema(
            &owner,
            ComponentSchema {
                id: declaration.id.clone(),
                version: declaration.version,
                fields: declaration.fields.clone(),
            },
        )
        .unwrap();
    let component_commands: Vec<_> = commands
        .into_iter()
        .map(|command| match command {
            HostCommand::Component(command) => command,
            HostCommand::PrincipalStorage(_) => {
                panic!("fixture emitted an unexpected storage command")
            }
        })
        .collect();
    state.apply_batch(&owner, &component_commands).unwrap();

    assert_eq!(
        state
            .row(&owner, &declaration.id, subject)
            .and_then(|row| row.get("count")),
        Some(&ExtensionValue::I64(1))
    );
}

#[test]
fn principal_storage_mutation_is_deferred_and_principal_attributed() {
    let runtime = runtime(SandboxConfig::default());
    let manifest = principal_storage_manifest();
    let compiled = compile_wat_for(
        &runtime,
        &manifest,
        &principal_storage_increment_component(),
    );
    let mut grants = CapabilitySet::new();
    grants.grant(EVENTS_SUBSCRIBE_CAPABILITY).unwrap();
    grants.grant(STORAGE_READ_OWN_CAPABILITY).unwrap();
    grants.grant(STORAGE_WRITE_OWN_CAPABILITY).unwrap();
    let mut instance = runtime.instantiate(&compiled, &manifest, grants).unwrap();
    instance.initialize().unwrap();

    let commands = instance
        .on_activate(ActivationEvent {
            subject: EntityRef::new(1, 1).unwrap(),
            activator: None,
        })
        .unwrap();
    assert_eq!(commands.len(), 1);
    let storage_commands: Vec<_> = commands
        .into_iter()
        .map(|command| match command {
            HostCommand::PrincipalStorage(command) => command,
            HostCommand::Component(_) => panic!("fixture emitted an unexpected component command"),
        })
        .collect();

    let owner = instance.principal().id().clone();
    let mut storage = PrincipalStorageStore::new(PrincipalStorageLimits::default()).unwrap();
    storage.register_schema(owner.clone(), 1).unwrap();
    storage.apply_batch(&owner, &storage_commands).unwrap();
    assert_eq!(
        storage
            .values(&owner)
            .unwrap()
            .get(&byroredux_sdk::identity::StorageKey::new("activation-count").unwrap()),
        Some(&ExtensionValue::I64(1))
    );
}

#[test]
fn entity_projection_snapshot_is_callback_local_and_cleared_after_delivery() {
    let runtime = runtime(SandboxConfig::default());
    let manifest = entity_projection_manifest(true);
    let compiled = compile_wat_for(&runtime, &manifest, &entity_projection_component());
    let mut grants = CapabilitySet::new();
    grants.grant(EVENTS_SUBSCRIBE_CAPABILITY).unwrap();
    grants.grant(WORLD_ENTITY_READ_CAPABILITY).unwrap();
    let mut instance = runtime.instantiate(&compiled, &manifest, grants).unwrap();
    instance.initialize().unwrap();

    let subject = EntityRef::new(4, 8).unwrap();
    let transform = WorldTransform::new([1.0, 2.0, 3.0], [0.0, 0.0, 0.0, 1.0], 1.0).unwrap();
    instance.set_entity_projections([EntityProjection::new(
        subject,
        None,
        Some("Subject".to_owned()),
        Some(transform),
    )
    .unwrap()]);
    assert!(instance
        .on_activate(ActivationEvent {
            subject,
            activator: None,
        })
        .unwrap()
        .is_empty());

    let error = instance
        .on_activate(ActivationEvent {
            subject,
            activator: None,
        })
        .unwrap_err();
    assert!(matches!(error, SandboxError::GuestFault { .. }));
    assert!(matches!(instance.status(), InstanceStatus::Quarantined(_)));

    let mut grants = CapabilitySet::new();
    grants.grant(EVENTS_SUBSCRIBE_CAPABILITY).unwrap();
    grants.grant(WORLD_ENTITY_READ_CAPABILITY).unwrap();
    let mut mismatched = runtime.instantiate(&compiled, &manifest, grants).unwrap();
    mismatched.initialize().unwrap();
    mismatched.set_entity_projections([EntityProjection::new(subject, None, None, None).unwrap()]);
    let error = mismatched
        .on_activate(ActivationEvent {
            subject: EntityRef::new(4, 9).unwrap(),
            activator: None,
        })
        .unwrap_err();
    assert!(matches!(error, SandboxError::GuestFault { .. }));
    assert!(matches!(
        mismatched.status(),
        InstanceStatus::Quarantined(_)
    ));
}

#[test]
fn entity_projection_host_call_requires_its_explicit_capability() {
    let runtime = runtime(SandboxConfig::default());
    let manifest = entity_projection_manifest(false);
    let compiled = compile_wat_for(&runtime, &manifest, &entity_projection_component());
    let mut grants = CapabilitySet::new();
    grants.grant(EVENTS_SUBSCRIBE_CAPABILITY).unwrap();
    let mut instance = runtime.instantiate(&compiled, &manifest, grants).unwrap();
    instance.initialize().unwrap();
    let subject = EntityRef::new(1, 1).unwrap();
    instance.set_entity_projections([EntityProjection::new(subject, None, None, None).unwrap()]);

    let error = instance
        .on_activate(ActivationEvent {
            subject,
            activator: None,
        })
        .unwrap_err();
    assert!(matches!(error, SandboxError::GuestFault { .. }));
    assert!(matches!(instance.status(), InstanceStatus::Quarantined(_)));
}

#[test]
fn a_trapping_activation_discards_every_queued_command() {
    let runtime = runtime(SandboxConfig::default());
    let manifest = activation_manifest();
    let compiled = compile_wat_for(&runtime, &manifest, &activation_counter_component(1, true));
    let mut grants = CapabilitySet::new();
    grants.grant(EVENTS_SUBSCRIBE_CAPABILITY).unwrap();
    grants.grant(COMPONENTS_WRITE_OWN_CAPABILITY).unwrap();
    let mut instance = runtime.instantiate(&compiled, &manifest, grants).unwrap();
    instance.initialize().unwrap();

    assert!(matches!(
        instance.on_activate(ActivationEvent {
            subject: EntityRef::new(1, 1).unwrap(),
            activator: None,
        }),
        Err(SandboxError::GuestFault {
            phase: LifecyclePhase::Activate,
            ..
        })
    ));
    assert!(matches!(
        instance.status(),
        InstanceStatus::Quarantined(fault)
            if fault.phase == LifecyclePhase::Activate && fault.kind == FaultKind::Guest
    ));
}

#[test]
fn activation_command_budget_quarantines_only_the_producing_instance() {
    let runtime = runtime(SandboxConfig {
        max_commands_per_entry: 1,
        ..SandboxConfig::default()
    });
    let manifest = activation_manifest();
    let compiled = compile_wat_for(&runtime, &manifest, &activation_counter_component(2, false));
    let mut grants = CapabilitySet::new();
    grants.grant(EVENTS_SUBSCRIBE_CAPABILITY).unwrap();
    grants.grant(COMPONENTS_WRITE_OWN_CAPABILITY).unwrap();
    let mut instance = runtime.instantiate(&compiled, &manifest, grants).unwrap();
    instance.initialize().unwrap();

    assert!(matches!(
        instance.on_activate(ActivationEvent {
            subject: EntityRef::new(1, 1).unwrap(),
            activator: None,
        }),
        Err(SandboxError::GuestFault {
            phase: LifecyclePhase::Activate,
            ..
        })
    ));
    assert!(matches!(
        instance.status(),
        InstanceStatus::Quarantined(fault)
            if fault.kind == FaultKind::CommandBudgetExhausted
    ));
}

#[test]
fn event_delivery_requires_both_subscription_and_capability() {
    let runtime = runtime(SandboxConfig::default());
    let mut declared = activation_manifest();
    for capability in &mut declared.capabilities {
        capability.required = false;
    }
    let compiled = compile_wat_for(&runtime, &declared, &activation_counter_component(1, false));
    let mut instance = runtime
        .instantiate(&compiled, &declared, CapabilitySet::new())
        .unwrap();
    instance.initialize().unwrap();
    assert!(matches!(
        instance.on_activate(ActivationEvent {
            subject: EntityRef::new(1, 1).unwrap(),
            activator: None,
        }),
        Err(SandboxError::EventDeliveryDenied(_))
    ));
    assert_eq!(instance.status(), &InstanceStatus::Active);

    let mut unsubscribed = declared.clone();
    unsubscribed.subscriptions.clear();
    let compiled = compile_wat_for(
        &runtime,
        &unsubscribed,
        &activation_counter_component(1, false),
    );
    let mut grants = CapabilitySet::new();
    grants.grant(EVENTS_SUBSCRIBE_CAPABILITY).unwrap();
    grants.grant(COMPONENTS_WRITE_OWN_CAPABILITY).unwrap();
    let mut instance = runtime
        .instantiate(&compiled, &unsubscribed, grants)
        .unwrap();
    instance.initialize().unwrap();
    assert!(matches!(
        instance.on_activate(ActivationEvent {
            subject: EntityRef::new(1, 1).unwrap(),
            activator: None,
        }),
        Err(SandboxError::EventNotSubscribed(_))
    ));
}
