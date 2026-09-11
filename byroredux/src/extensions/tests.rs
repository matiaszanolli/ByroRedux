use semver::{Version, VersionReq};

use super::commands::apply_pending_actor_value_writes;
use super::*;
use byroredux_core::form_id::{LocalFormId, PluginId};
use byroredux_core::math::{Quat, Vec3};
use byroredux_plugin::esm::records::{ScriptLocalVar, ScriptRecord};
use byroredux_sdk::compatibility::{
    PAPYRUS_STORAGE_UTIL_FORM_FILTER_BY_TYPES_ROUTE, PAPYRUS_STORAGE_UTIL_FORM_FILTER_BY_TYPE_ROUTE,
};
use byroredux_sdk::component::{ComponentFieldDeclaration, ExtensionValue, ExtensionValueType};
use byroredux_sdk::identity::{
    CapabilityId, ComponentFieldId, ComponentSchemaId, EventId, ScriptFunctionId,
    ScriptParameterId, ServiceId, StorageKey,
};
use byroredux_sdk::manifest::{
    CapabilityRequest, ComponentSchemaDeclaration, EventFilter, EventSubscription,
    ExecutableComponent, EXTENSION_MANIFEST_VERSION,
};
use byroredux_sdk::script_function::{
    PapyrusFunctionAlias, ScriptParameterDeclaration, ScriptResultDeclaration, ScriptValueType,
};
use byroredux_sdk::service::{
    COMPONENTS_WRITE_OWN_CAPABILITY, EXTENSION_WORLD_SERVICE, STORAGE_READ_OWN_CAPABILITY,
    STORAGE_WRITE_OWN_CAPABILITY, WORLD_ENTITY_READ_CAPABILITY, WORLD_TRANSFORM_READ_CAPABILITY,
};
use byroredux_sdk::storage::{PrincipalStorageCommand, PrincipalStorageValue};

const COMPONENT: &str = r#"
(component
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
    (type $input-action-shape (enum
      "move-forward" "move-backward" "strafe-left" "strafe-right"
      "jump" "sprint" "activate" "attack" "block" "inventory"
      "quicksave" "quickload" "pause"))
    (export "input-action" (type $input-action-in (eq $input-action-shape)))
    (type $input-phase-shape (enum "pressed" "released"))
    (export "input-phase" (type $input-phase-in (eq $input-phase-shape)))
    (type $session-phase-shape (enum "new-game" "save-complete" "load-complete"))
    (export "session-phase" (type $session-phase-in (eq $session-phase-shape)))
    (export "queue-increment-own-i64" (func
      (param "entity" $entity-ref-in)
      (param "schema-index" u32)
      (param "field-index" u32)
      (param "delta" s64)))
  ))
  (alias export $state "entity-ref" (type $entity-ref))
  (alias export $state "form-ref" (type $form-ref))
  (alias export $state "hit-details" (type $hit-details))
  (alias export $state "input-action" (type $input-action))
  (alias export $state "input-phase" (type $input-phase))
  (alias export $state "session-phase" (type $session-phase))
  (alias export $state "queue-increment-own-i64" (func $increment))
  (core func $increment-lower (canon lower (func $increment)))
  (core module $guest
    (import "host" "increment" (func $increment (param i64 i64 i32 i32 i64)))
    (func (export "initialize"))
    (func (export "shutdown"))
    (func (export "on-activate")
      (param $world i64) (param $object i64) (param i32 i64 i64)
      local.get $world
      local.get $object
      i32.const 0
      i32.const 0
      i64.const 1
      call $increment)
    (func (export "on-cell-load") (param $world i64) (param $object i64)
      local.get $world
      local.get $object
      i32.const 0
      i32.const 0
      i64.const 1
      call $increment)
    (func (export "on-hit")
      (param $world i64) (param $object i64)
      (param i32 i64 i64) (param i32 i64 i64) (param i32 i64 i64)
      (param f32 i32 i32 i32 i32)
      local.get $world
      local.get $object
      i32.const 0
      i32.const 0
      i64.const 1
      call $increment)
    (func (export "on-equipment-change")
      (param $world i64) (param $object i64)
      (param i64 i64 i32 i32)
      local.get $world
      local.get $object
      i32.const 0
      i32.const 0
      i64.const 1
      call $increment)
    (func (export "on-input-action") (param i32 i32))
    (func (export "on-session-event") (param i32 i32 i32))
    (func (export "on-custom-event") (param i32))
    (func (export "on-update") (param f32))
    (func (export "on-console-command") (param i32))
    (func (export "on-script-function") (param i32))
  )
  (core instance $guest-instance (instantiate $guest
    (with "host" (instance (export "increment" (func $increment-lower))))
  ))
  (func (export "initialize") (canon lift (core func $guest-instance "initialize")))
  (func (export "shutdown") (canon lift (core func $guest-instance "shutdown")))
  (func (export "on-activate")
    (param "subject" $entity-ref)
    (param "activator" (option $entity-ref))
    (canon lift (core func $guest-instance "on-activate")))
  (func (export "on-cell-load")
    (param "subject" $entity-ref)
    (canon lift (core func $guest-instance "on-cell-load")))
  (func (export "on-hit")
    (param "subject" $entity-ref)
    (param "aggressor" (option $entity-ref))
    (param "source" (option $entity-ref))
    (param "projectile" (option $entity-ref))
    (param "details" $hit-details)
    (canon lift (core func $guest-instance "on-hit")))
  (func (export "on-equipment-change")
    (param "wearer" $entity-ref)
    (param "item" $form-ref)
    (param "equipped" bool)
    (canon lift (core func $guest-instance "on-equipment-change")))
  (func (export "on-input-action")
    (param "action" $input-action)
    (param "phase" $input-phase)
    (canon lift (core func $guest-instance "on-input-action")))
  (func (export "on-session-event")
    (param "phase" $session-phase)
    (param "slot" (option u32))
    (canon lift (core func $guest-instance "on-session-event")))
  (func (export "on-custom-event")
    (param "subscription-index" u32)
    (canon lift (core func $guest-instance "on-custom-event")))
  (func (export "on-update")
    (param "elapsed-seconds" f32)
    (canon lift (core func $guest-instance "on-update")))
  (func (export "on-console-command")
    (param "command-index" u32)
    (canon lift (core func $guest-instance "on-console-command")))
  (func (export "on-script-function")
    (param "function-index" u32)
    (canon lift (core func $guest-instance "on-script-function")))
)
"#;

const STORAGE_COMPONENT: &str = r#"
(component
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
    (type $input-action-shape (enum
      "move-forward" "move-backward" "strafe-left" "strafe-right"
      "jump" "sprint" "activate" "attack" "block" "inventory"
      "quicksave" "quickload" "pause"))
    (export "input-action" (type $input-action-in (eq $input-action-shape)))
    (type $input-phase-shape (enum "pressed" "released"))
    (export "input-phase" (type $input-phase-in (eq $input-phase-shape)))
    (type $session-phase-shape (enum "new-game" "save-complete" "load-complete"))
    (export "session-phase" (type $session-phase-in (eq $session-phase-shape)))
  ))
  (import "byro:mod-host/storage@0.1.0" (instance $storage
    (export "queue-increment-i64" (func
      (param "key" string)
      (param "delta" s64)))
  ))
  (alias export $state "entity-ref" (type $entity-ref))
  (alias export $state "form-ref" (type $form-ref))
  (alias export $state "hit-details" (type $hit-details))
  (alias export $state "input-action" (type $input-action))
  (alias export $state "input-phase" (type $input-phase))
  (alias export $state "session-phase" (type $session-phase))
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
    (func (export "on-activate") (param i64 i64 i32 i64 i64)
      i32.const 0
      i32.const 16
      i64.const 1
      call $increment)
    (func (export "on-cell-load") (param i64 i64))
    (func (export "on-hit")
      (param i64 i64)
      (param i32 i64 i64) (param i32 i64 i64) (param i32 i64 i64)
      (param f32 i32 i32 i32 i32))
    (func (export "on-equipment-change")
      (param i64 i64 i64 i64 i32 i32)
      i32.const 0
      i32.const 16
      i64.const 1
      call $increment)
    (func (export "on-input-action")
      (param $action i32) (param $phase i32)
      local.get $action
      i32.const 6
      i32.ne
      if
        unreachable
      end
      i32.const 0
      i32.const 16
      i64.const 1
      call $increment)
    (func (export "on-session-event")
      (param $phase i32) (param $slot-tag i32) (param $slot i32)
      local.get $phase
      i32.const 2
      i32.ne
      local.get $slot-tag
      i32.const 1
      i32.ne
      i32.or
      local.get $slot
      i32.const 7
      i32.ne
      i32.or
      if
        unreachable
      end
      i32.const 0
      i32.const 16
      i64.const 1
      call $increment)
    (func (export "on-custom-event") (param $subscription-index i32)
      local.get $subscription-index
      i32.eqz
      if
        i32.const 0
        i32.const 16
        i64.const 1
        call $increment
      else
        unreachable
      end)
    (func (export "on-update") (param f32)
      i32.const 0
      i32.const 16
      i64.const 1
      call $increment)
    (func (export "on-console-command") (param i32)
      i32.const 0
      i32.const 16
      i64.const 1
      call $increment)
    (func (export "on-script-function") (param i32))
  )
  (core instance $guest-instance (instantiate $guest
    (with "libc" (instance $libc))
    (with "host" (instance (export "increment" (func $increment-lower))))
  ))
  (func (export "initialize") (canon lift (core func $guest-instance "initialize")))
  (func (export "shutdown") (canon lift (core func $guest-instance "shutdown")))
  (func (export "on-activate")
    (param "subject" $entity-ref)
    (param "activator" (option $entity-ref))
    (canon lift (core func $guest-instance "on-activate")))
  (func (export "on-cell-load")
    (param "subject" $entity-ref)
    (canon lift (core func $guest-instance "on-cell-load")))
  (func (export "on-hit")
    (param "subject" $entity-ref)
    (param "aggressor" (option $entity-ref))
    (param "source" (option $entity-ref))
    (param "projectile" (option $entity-ref))
    (param "details" $hit-details)
    (canon lift (core func $guest-instance "on-hit")))
  (func (export "on-equipment-change")
    (param "wearer" $entity-ref)
    (param "item" $form-ref)
    (param "equipped" bool)
    (canon lift (core func $guest-instance "on-equipment-change")))
  (func (export "on-input-action")
    (param "action" $input-action)
    (param "phase" $input-phase)
    (canon lift (core func $guest-instance "on-input-action")))
  (func (export "on-session-event")
    (param "phase" $session-phase)
    (param "slot" (option u32))
    (canon lift (core func $guest-instance "on-session-event")))
  (func (export "on-custom-event")
    (param "subscription-index" u32)
    (canon lift (core func $guest-instance "on-custom-event")))
  (func (export "on-update")
    (param "elapsed-seconds" f32)
    (canon lift (core func $guest-instance "on-update")))
  (func (export "on-console-command")
    (param "command-index" u32)
    (canon lift (core func $guest-instance "on-console-command")))
  (func (export "on-script-function")
    (param "function-index" u32)
    (canon lift (core func $guest-instance "on-script-function")))
)
"#;

fn script_function_component() -> String {
    STORAGE_COMPONENT
            .replace(
                "(component\n  (import \"byro:mod-host/state@0.1.0\"",
                "(component\n  (import \"byro:mod-host/script-functions@0.1.0\" (instance $functions\n    (export \"set-result-integer\" (func (param \"value\" s64)))\n  ))\n  (import \"byro:mod-host/state@0.1.0\"",
            )
            .replace(
                "  (alias export $state \"entity-ref\" (type $entity-ref))",
                "  (alias export $functions \"set-result-integer\" (func $set-result))\n  (core func $set-result-lower (canon lower (func $set-result)))\n  (alias export $state \"entity-ref\" (type $entity-ref))",
            )
            .replace(
                "    (import \"libc\" \"memory\" (memory 1))",
                "    (import \"libc\" \"memory\" (memory 1))\n    (import \"host\" \"set-result\" (func $set-result (param i64)))",
            )
            .replace(
                "    (func (export \"on-script-function\") (param i32))",
                "    (func (export \"on-script-function\") (param $index i32)\n      local.get $index\n      i32.eqz\n      i32.eqz\n      if unreachable end\n      i32.const 0\n      i32.const 16\n      i64.const 1\n      call $increment\n      i64.const 42\n      call $set-result)",
            )
            .replace(
                "    (with \"host\" (instance (export \"increment\" (func $increment-lower))))",
                "    (with \"host\" (instance\n      (export \"increment\" (func $increment-lower))\n      (export \"set-result\" (func $set-result-lower))))",
            )
}

const PROJECTION_COMPONENT: &str = r#"
(component
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
    (type $input-action-shape (enum
      "move-forward" "move-backward" "strafe-left" "strafe-right"
      "jump" "sprint" "activate" "attack" "block" "inventory"
      "quicksave" "quickload" "pause"))
    (export "input-action" (type $input-action-in (eq $input-action-shape)))
    (type $input-phase-shape (enum "pressed" "released"))
    (export "input-phase" (type $input-phase-in (eq $input-phase-shape)))
    (type $session-phase-shape (enum "new-game" "save-complete" "load-complete"))
    (export "session-phase" (type $session-phase-in (eq $session-phase-shape)))
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
  (alias export $state "input-action" (type $input-action))
  (alias export $state "input-phase" (type $input-phase))
  (alias export $state "session-phase" (type $session-phase))
  (alias export $world "contains-entity" (func $contains))
  (core func $contains-lower (canon lower (func $contains)))
  (core module $guest
    (import "host" "contains" (func $contains (param i64 i64) (result i32)))
    (func (export "initialize"))
    (func (export "shutdown"))
    (func (export "on-activate")
      (param $world i64) (param $object i64) (param i32 i64 i64)
      local.get $world
      local.get $object
      call $contains
      i32.eqz
      if
        unreachable
      end)
    (func (export "on-cell-load") (param i64 i64))
    (func (export "on-hit")
      (param i64 i64)
      (param i32 i64 i64) (param i32 i64 i64) (param i32 i64 i64)
      (param f32 i32 i32 i32 i32))
    (func (export "on-equipment-change")
      (param $world i64) (param $object i64)
      (param i64 i64 i32 i32)
      local.get $world
      local.get $object
      call $contains
      i32.eqz
      if
        unreachable
      end)
    (func (export "on-input-action") (param i32 i32))
    (func (export "on-session-event") (param i32 i32 i32))
    (func (export "on-custom-event") (param i32))
    (func (export "on-update") (param f32))
    (func (export "on-console-command") (param i32))
    (func (export "on-script-function") (param i32))
  )
  (core instance $guest-instance (instantiate $guest
    (with "host" (instance (export "contains" (func $contains-lower))))
  ))
  (func (export "initialize") (canon lift (core func $guest-instance "initialize")))
  (func (export "shutdown") (canon lift (core func $guest-instance "shutdown")))
  (func (export "on-activate")
    (param "subject" $entity-ref)
    (param "activator" (option $entity-ref))
    (canon lift (core func $guest-instance "on-activate")))
  (func (export "on-cell-load")
    (param "subject" $entity-ref)
    (canon lift (core func $guest-instance "on-cell-load")))
  (func (export "on-hit")
    (param "subject" $entity-ref)
    (param "aggressor" (option $entity-ref))
    (param "source" (option $entity-ref))
    (param "projectile" (option $entity-ref))
    (param "details" $hit-details)
    (canon lift (core func $guest-instance "on-hit")))
  (func (export "on-equipment-change")
    (param "wearer" $entity-ref)
    (param "item" $form-ref)
    (param "equipped" bool)
    (canon lift (core func $guest-instance "on-equipment-change")))
  (func (export "on-input-action")
    (param "action" $input-action)
    (param "phase" $input-phase)
    (canon lift (core func $guest-instance "on-input-action")))
  (func (export "on-session-event")
    (param "phase" $session-phase)
    (param "slot" (option u32))
    (canon lift (core func $guest-instance "on-session-event")))
  (func (export "on-custom-event")
    (param "subscription-index" u32)
    (canon lift (core func $guest-instance "on-custom-event")))
  (func (export "on-update")
    (param "elapsed-seconds" f32)
    (canon lift (core func $guest-instance "on-update")))
  (func (export "on-console-command")
    (param "command-index" u32)
    (canon lift (core func $guest-instance "on-console-command")))
  (func (export "on-script-function")
    (param "function-index" u32)
    (canon lift (core func $guest-instance "on-script-function")))
)
"#;

fn manifest(id: &str) -> ExtensionManifest {
    ExtensionManifest {
        manifest_version: EXTENSION_MANIFEST_VERSION,
        id: ExtensionId::new(id).unwrap(),
        name: id.to_owned(),
        version: Version::new(1, 0, 0),
        sdk: VersionReq::parse("^0.1").unwrap(),
        dependencies: Vec::new(),
        components: vec![ExecutableComponent {
            id: ComponentId::new("runtime").unwrap(),
            path: "runtime.wasm".to_owned(),
            world: ServiceId::new(EXTENSION_WORLD_SERVICE).unwrap(),
            world_version: VersionReq::parse("^0.1").unwrap(),
        }],
        capabilities: vec![
            CapabilityRequest {
                id: CapabilityId::new(EVENTS_SUBSCRIBE_CAPABILITY).unwrap(),
                required: true,
            },
            CapabilityRequest {
                id: CapabilityId::new(COMPONENTS_WRITE_OWN_CAPABILITY).unwrap(),
                required: true,
            },
        ],
        subscriptions: vec![EventSubscription {
            event: EventId::new(ACTIVATE_EVENT).unwrap(),
            filters: Vec::new(),
            interval_millis: None,
        }],
        component_schemas: vec![ComponentSchemaDeclaration {
            id: ComponentSchemaId::new("example.activation-count").unwrap(),
            version: 1,
            fields: vec![ComponentFieldDeclaration {
                id: ComponentFieldId::new("count").unwrap(),
                value_type: ExtensionValueType::I64,
            }],
        }],
        console_commands: Vec::new(),
        script_functions: Vec::new(),
        settings: Vec::new(),
        principal_storage_schema: None,
    }
}

fn grants() -> CapabilitySet {
    let mut grants = CapabilitySet::new();
    grants.grant(EVENTS_SUBSCRIBE_CAPABILITY).unwrap();
    grants.grant(COMPONENTS_WRITE_OWN_CAPABILITY).unwrap();
    grants
}

fn cell_load_manifest(id: &str) -> ExtensionManifest {
    let mut manifest = manifest(id);
    manifest.subscriptions = vec![EventSubscription {
        event: EventId::new(CELL_LOAD_EVENT).unwrap(),
        filters: Vec::new(),
        interval_millis: None,
    }];
    manifest
}

fn hit_manifest(id: &str) -> ExtensionManifest {
    let mut manifest = manifest(id);
    manifest.subscriptions = vec![EventSubscription {
        event: EventId::new(HIT_EVENT).unwrap(),
        filters: Vec::new(),
        interval_millis: None,
    }];
    manifest
}

fn equipment_manifest(id: &str) -> ExtensionManifest {
    let mut manifest = manifest(id);
    manifest.subscriptions = vec![EventSubscription {
        event: EventId::new(EQUIPMENT_EVENT).unwrap(),
        filters: Vec::new(),
        interval_millis: None,
    }];
    manifest
}

fn input_manifest(id: &str) -> ExtensionManifest {
    let mut manifest = storage_manifest(id);
    manifest.capabilities.push(CapabilityRequest {
        id: CapabilityId::new(INPUT_ACTIONS_SUBSCRIBE_CAPABILITY).unwrap(),
        required: true,
    });
    manifest.subscriptions = vec![EventSubscription {
        event: EventId::new(INPUT_ACTION_EVENT).unwrap(),
        filters: vec![EventFilter {
            field: ServiceId::new(byroredux_sdk::service::INPUT_ACTION_FILTER_FIELD).unwrap(),
            equals: "activate".to_owned(),
        }],
        interval_millis: None,
    }];
    manifest
}

fn session_manifest(id: &str) -> ExtensionManifest {
    let mut manifest = storage_manifest(id);
    manifest.subscriptions = vec![EventSubscription {
        event: EventId::new(SESSION_EVENT).unwrap(),
        filters: vec![EventFilter {
            field: ServiceId::new(byroredux_sdk::service::SESSION_PHASE_FILTER_FIELD).unwrap(),
            equals: "load-complete".to_owned(),
        }],
        interval_millis: None,
    }];
    manifest
}

fn custom_event_manifest(id: &str, event: &str) -> ExtensionManifest {
    let mut manifest = storage_manifest(id);
    manifest.subscriptions = vec![EventSubscription {
        event: EventId::new(event).unwrap(),
        filters: Vec::new(),
        interval_millis: None,
    }];
    manifest
}

fn storage_manifest(id: &str) -> ExtensionManifest {
    let mut manifest = manifest(id);
    manifest.component_schemas.clear();
    manifest.principal_storage_schema = Some(1);
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
    manifest
}

fn console_manifest(id: &str) -> ExtensionManifest {
    let mut manifest = storage_manifest(id);
    manifest.capabilities.push(CapabilityRequest {
        id: CapabilityId::new(CONSOLE_REGISTER_CAPABILITY).unwrap(),
        required: true,
    });
    manifest.console_commands = vec![byroredux_sdk::console::ConsoleCommandDeclaration {
        id: byroredux_sdk::identity::ConsoleCommandId::new("bump").unwrap(),
        component: ComponentId::new("runtime").unwrap(),
        description: "Increment the test counter".to_owned(),
    }];
    manifest
}

fn script_function_manifest(id: &str) -> ExtensionManifest {
    let mut manifest = storage_manifest(id);
    manifest.capabilities.push(CapabilityRequest {
        id: CapabilityId::new(SCRIPT_FUNCTIONS_REGISTER_CAPABILITY).unwrap(),
        required: true,
    });
    manifest.script_functions = vec![ScriptFunctionDeclaration {
        id: ScriptFunctionId::new("answer").unwrap(),
        component: ComponentId::new("runtime").unwrap(),
        parameters: vec![ScriptParameterDeclaration {
            id: ScriptParameterId::new("input").unwrap(),
            value_type: ScriptValueType::Integer,
            optional: false,
        }],
        result: Some(ScriptResultDeclaration {
            value_type: ScriptValueType::Integer,
            optional: false,
        }),
        papyrus: Some(PapyrusFunctionAlias {
            provider: "ByroFixture".to_owned(),
            function: "Answer".to_owned(),
        }),
        description: "Return the test answer".to_owned(),
    }];
    manifest
}

fn update_manifest(id: &str) -> ExtensionManifest {
    let mut manifest = storage_manifest(id);
    manifest.subscriptions = vec![EventSubscription {
        event: EventId::new(UPDATE_EVENT).unwrap(),
        filters: Vec::new(),
        interval_millis: Some(100),
    }];
    manifest
}

fn storage_grants() -> CapabilitySet {
    let mut grants = CapabilitySet::new();
    grants.grant(EVENTS_SUBSCRIBE_CAPABILITY).unwrap();
    grants.grant(STORAGE_READ_OWN_CAPABILITY).unwrap();
    grants.grant(STORAGE_WRITE_OWN_CAPABILITY).unwrap();
    grants
}

fn console_grants() -> CapabilitySet {
    let mut grants = storage_grants();
    grants.grant(CONSOLE_REGISTER_CAPABILITY).unwrap();
    grants
}

fn script_function_grants() -> CapabilitySet {
    let mut grants = storage_grants();
    grants.grant(SCRIPT_FUNCTIONS_REGISTER_CAPABILITY).unwrap();
    grants
}

fn install_storage_package(host: &mut ExtensionHost, id: &str) {
    let manifest = storage_manifest(id);
    let mut artifacts = ExtensionArtifacts::new();
    artifacts.insert(
        ComponentId::new("runtime").unwrap(),
        wat::parse_str(STORAGE_COMPONENT).unwrap(),
    );
    host.install_package(&manifest, &artifacts, storage_grants())
        .unwrap();
}

fn host_with_storage_package(id: &str) -> ExtensionHost {
    let mut host =
        ExtensionHost::new(SandboxConfig::default(), ComponentStoreLimits::default()).unwrap();
    install_storage_package(&mut host, id);
    host
}

fn host_with_update_package(id: &str) -> ExtensionHost {
    let mut host =
        ExtensionHost::new(SandboxConfig::default(), ComponentStoreLimits::default()).unwrap();
    let mut artifacts = ExtensionArtifacts::new();
    artifacts.insert(
        ComponentId::new("runtime").unwrap(),
        wat::parse_str(STORAGE_COMPONENT).unwrap(),
    );
    host.install_package(&update_manifest(id), &artifacts, storage_grants())
        .unwrap();
    host
}

fn host_with_projection_package(id: &str) -> ExtensionHost {
    let mut manifest = manifest(id);
    manifest.component_schemas.clear();
    manifest.capabilities = vec![
        CapabilityRequest {
            id: CapabilityId::new(EVENTS_SUBSCRIBE_CAPABILITY).unwrap(),
            required: true,
        },
        CapabilityRequest {
            id: CapabilityId::new(WORLD_ENTITY_READ_CAPABILITY).unwrap(),
            required: true,
        },
        CapabilityRequest {
            id: CapabilityId::new(WORLD_TRANSFORM_READ_CAPABILITY).unwrap(),
            required: true,
        },
    ];
    let mut grants = CapabilitySet::new();
    grants.grant(EVENTS_SUBSCRIBE_CAPABILITY).unwrap();
    grants.grant(WORLD_ENTITY_READ_CAPABILITY).unwrap();
    grants.grant(WORLD_TRANSFORM_READ_CAPABILITY).unwrap();
    let mut artifacts = ExtensionArtifacts::new();
    artifacts.insert(
        ComponentId::new("runtime").unwrap(),
        wat::parse_str(PROJECTION_COMPONENT).unwrap(),
    );
    let mut host =
        ExtensionHost::new(SandboxConfig::default(), ComponentStoreLimits::default()).unwrap();
    host.install_package(&manifest, &artifacts, grants).unwrap();
    host
}

fn host_with_package(id: &str) -> ExtensionHost {
    host_with_manifest(manifest(id))
}

fn host_with_cell_load_package(id: &str) -> ExtensionHost {
    host_with_manifest(cell_load_manifest(id))
}

fn host_with_hit_package(id: &str) -> ExtensionHost {
    host_with_manifest(hit_manifest(id))
}

fn host_with_equipment_package(id: &str) -> ExtensionHost {
    host_with_manifest(equipment_manifest(id))
}

fn host_with_input_package(id: &str) -> ExtensionHost {
    let mut host =
        ExtensionHost::new(SandboxConfig::default(), ComponentStoreLimits::default()).unwrap();
    let mut artifacts = ExtensionArtifacts::new();
    artifacts.insert(
        ComponentId::new("runtime").unwrap(),
        wat::parse_str(STORAGE_COMPONENT).unwrap(),
    );
    let mut grants = storage_grants();
    grants.grant(INPUT_ACTIONS_SUBSCRIBE_CAPABILITY).unwrap();
    host.install_package(&input_manifest(id), &artifacts, grants)
        .unwrap();
    host
}

fn host_with_session_package(id: &str) -> ExtensionHost {
    let mut host =
        ExtensionHost::new(SandboxConfig::default(), ComponentStoreLimits::default()).unwrap();
    let mut artifacts = ExtensionArtifacts::new();
    artifacts.insert(
        ComponentId::new("runtime").unwrap(),
        wat::parse_str(STORAGE_COMPONENT).unwrap(),
    );
    host.install_package(&session_manifest(id), &artifacts, storage_grants())
        .unwrap();
    host
}

fn host_with_custom_event_package(id: &str, event: &str) -> ExtensionHost {
    let mut host =
        ExtensionHost::new(SandboxConfig::default(), ComponentStoreLimits::default()).unwrap();
    let mut artifacts = ExtensionArtifacts::new();
    artifacts.insert(
        ComponentId::new("runtime").unwrap(),
        wat::parse_str(STORAGE_COMPONENT).unwrap(),
    );
    host.install_package(
        &custom_event_manifest(id, event),
        &artifacts,
        storage_grants(),
    )
    .unwrap();
    host
}

fn host_with_manifest(manifest: ExtensionManifest) -> ExtensionHost {
    let mut host =
        ExtensionHost::new(SandboxConfig::default(), ComponentStoreLimits::default()).unwrap();
    let mut artifacts = ExtensionArtifacts::new();
    artifacts.insert(
        ComponentId::new("runtime").unwrap(),
        wat::parse_str(COMPONENT).unwrap(),
    );
    host.install_package(&manifest, &artifacts, grants())
        .unwrap();
    host
}

fn empty_snapshot() -> byroredux_save::Snapshot {
    byroredux_save::Snapshot {
        next_entity: 0,
        strings: Vec::new(),
        components: BTreeMap::new(),
        resources: BTreeMap::new(),
    }
}

fn form_pair_for_test() -> FormIdPair {
    FormIdPair {
        plugin: PluginId(0x0123_4567_89ab_cdef_fedc_ba98_7654_3210),
        local: LocalFormId(0x123456),
    }
}

fn world_with_form_and_host(slot: ExtensionHostSlot, pair: FormIdPair) -> (World, EntityId) {
    let mut world = World::new();
    world.register::<FormIdComponent>();
    let entity = world.spawn();
    let mut pool = FormIdPool::new();
    let form = pool.intern(pair);
    world.insert(entity, FormIdComponent(form));
    world.insert_resource(pool);
    world.insert_resource(slot);
    (world, entity)
}

#[test]
fn live_host_delivers_activation_and_commits_owned_state() {
    let mut host = host_with_package("org.example.live");
    let stats = host.dispatch_activations([RawActivation {
        subject: 41,
        subject_form: None,
        activator: Some(1),
        activator_form: None,
    }]);
    assert_eq!(
        stats,
        ExtensionDispatchStats {
            events: 1,
            deliveries: 1,
            commands_applied: 1,
            faults: 0,
        }
    );

    let subject = host.handles.by_entity[&41];
    let owner = PrincipalId::new("org.example.live").unwrap();
    let schema = ComponentSchemaId::new("example.activation-count").unwrap();
    assert_eq!(
        host.state()
            .row(&owner, &schema, subject)
            .and_then(|row| row.get("count")),
        Some(&ExtensionValue::I64(1))
    );
    assert_eq!(host.package_count(), 1);
    assert_eq!(host.component_count(), 1);
}

#[test]
fn live_host_delivers_cell_load_only_to_its_declared_subscriber() {
    let mut host = host_with_cell_load_package("org.example.live-cell-load");
    let stats = host.dispatch_cell_loads([RawCellLoad {
        subject: 41,
        subject_form: None,
    }]);
    assert_eq!(
        stats,
        ExtensionDispatchStats {
            events: 1,
            deliveries: 1,
            commands_applied: 1,
            faults: 0,
        }
    );

    let subject = host.handles.by_entity[&41];
    let owner = PrincipalId::new("org.example.live-cell-load").unwrap();
    let schema = ComponentSchemaId::new("example.activation-count").unwrap();
    assert_eq!(
        host.state()
            .row(&owner, &schema, subject)
            .and_then(|row| row.get("count")),
        Some(&ExtensionValue::I64(1))
    );
    assert_eq!(
        host.dispatch_activations([RawActivation {
            subject: 41,
            subject_form: None,
            activator: None,
            activator_form: None,
        }]),
        ExtensionDispatchStats {
            events: 1,
            ..ExtensionDispatchStats::default()
        }
    );
}

#[test]
fn live_host_delivers_hit_payload_to_declared_subscriber() {
    let mut host = host_with_hit_package("org.example.live-hit");
    let stats = host.dispatch_hits([RawHit {
        subject: 41,
        subject_form: None,
        aggressor: Some(7),
        aggressor_form: None,
        source: Some(7),
        source_form: None,
        projectile: None,
        projectile_form: None,
        damage: 12.5,
        power_attack: true,
        sneak_attack: false,
        bash_attack: true,
        blocked: false,
    }]);
    assert_eq!(
        stats,
        ExtensionDispatchStats {
            events: 1,
            deliveries: 1,
            commands_applied: 1,
            faults: 0,
        }
    );
    let subject = host.handles.by_entity[&41];
    let owner = PrincipalId::new("org.example.live-hit").unwrap();
    let schema = ComponentSchemaId::new("example.activation-count").unwrap();
    assert_eq!(
        host.state()
            .row(&owner, &schema, subject)
            .and_then(|row| row.get("count")),
        Some(&ExtensionValue::I64(1))
    );
}

#[test]
fn live_host_delivers_equipment_changes_only_to_declared_subscriber() {
    let mut host = host_with_equipment_package("org.example.live-equipment");
    let item = FormRef::new([0x5A; 16], 0x1234);
    let stats = host.dispatch_equipment_changes([RawEquipmentChange {
        wearer: 41,
        wearer_form: None,
        item,
        equipped: true,
    }]);
    assert_eq!(
        stats,
        ExtensionDispatchStats {
            events: 1,
            deliveries: 1,
            commands_applied: 1,
            faults: 0,
        }
    );
    let wearer = host.handles.by_entity[&41];
    let owner = PrincipalId::new("org.example.live-equipment").unwrap();
    let schema = ComponentSchemaId::new("example.activation-count").unwrap();
    assert_eq!(
        host.state()
            .row(&owner, &schema, wearer)
            .and_then(|row| row.get("count")),
        Some(&ExtensionValue::I64(1))
    );

    let mut unsubscribed = host_with_package("org.example.no-equipment");
    assert_eq!(
        unsubscribed.dispatch_equipment_changes([RawEquipmentChange {
            wearer: 41,
            wearer_form: None,
            item,
            equipped: false,
        }]),
        ExtensionDispatchStats {
            events: 1,
            ..ExtensionDispatchStats::default()
        }
    );
}

#[test]
fn live_host_applies_normalized_input_action_filters_before_guest_delivery() {
    let principal = PrincipalId::new("org.example.live-input").unwrap();
    let key = StorageKey::new("activation-count").unwrap();
    let mut host = host_with_input_package(principal.as_str());
    let stats = host.dispatch_input_actions([
        InputActionEvent {
            action: SdkInputAction::Inventory,
            phase: InputPhase::Pressed,
        },
        InputActionEvent {
            action: SdkInputAction::Activate,
            phase: InputPhase::Pressed,
        },
        InputActionEvent {
            action: SdkInputAction::Activate,
            phase: InputPhase::Released,
        },
    ]);
    assert_eq!(stats.events, 3);
    assert_eq!(stats.deliveries, 2);
    assert_eq!(stats.commands_applied, 2);
    assert_eq!(stats.faults, 0);
    assert_eq!(
        host.principal_storage
            .values(&principal)
            .and_then(|values| values.get(&key)),
        Some(&PrincipalStorageValue::I64(2))
    );
}

#[test]
fn live_host_applies_session_phase_filters_before_guest_delivery() {
    let principal = PrincipalId::new("org.example.live-session").unwrap();
    let key = StorageKey::new("activation-count").unwrap();
    let mut host = host_with_session_package(principal.as_str());
    let stats = host.dispatch_session_events([
        SessionEvent {
            phase: SessionPhase::SaveComplete,
            slot: Some(7),
        },
        SessionEvent {
            phase: SessionPhase::LoadComplete,
            slot: Some(7),
        },
    ]);
    assert_eq!(stats.events, 2);
    assert_eq!(stats.deliveries, 1);
    assert_eq!(stats.commands_applied, 1);
    assert_eq!(stats.faults, 0);
    assert_eq!(
        host.principal_storage
            .values(&principal)
            .and_then(|values| values.get(&key)),
        Some(&PrincipalStorageValue::I64(1))
    );
}

#[test]
fn custom_events_route_by_exact_channel_on_a_later_dispatch_pass() {
    let subscriber = PrincipalId::new("org.example.subscriber").unwrap();
    let sender = PrincipalId::new("org.example.publisher").unwrap();
    let channel = EventId::new("mod.org.example.publisher.event.ready").unwrap();
    let key = StorageKey::new("activation-count").unwrap();
    let mut host = host_with_custom_event_package(subscriber.as_str(), channel.as_str());

    host.pending_custom_events.push(CustomEvent {
        event: channel,
        sender,
        payload: vec![1, 2, 3],
    });
    assert!(host
        .principal_storage
        .values(&subscriber)
        .and_then(|values| values.get(&key))
        .is_none());

    let stats = host.dispatch_custom_events();
    assert_eq!(
        stats,
        ExtensionDispatchStats {
            events: 1,
            deliveries: 1,
            commands_applied: 1,
            faults: 0,
        }
    );
    assert_eq!(
        host.principal_storage
            .values(&subscriber)
            .and_then(|values| values.get(&key)),
        Some(&PrincipalStorageValue::I64(1))
    );

    host.pending_custom_events.push(CustomEvent {
        event: EventId::new("mod.org.example.other.event.ready").unwrap(),
        sender: PrincipalId::new("org.example.other").unwrap(),
        payload: Vec::new(),
    });
    assert_eq!(
        host.dispatch_custom_events(),
        ExtensionDispatchStats {
            events: 1,
            ..ExtensionDispatchStats::default()
        }
    );
}

#[test]
fn shared_skse_mod_events_route_across_principals() {
    let subscriber = PrincipalId::new("org.example.skyui-adapter").unwrap();
    let sender = PrincipalId::new("org.example.config-provider").unwrap();
    let channel =
        byroredux_sdk::event::legacy_skse_mod_event_id("SKICP_configManagerReady").unwrap();
    let payload = byroredux_sdk::event::LegacySkseModEventPayload::new(
        String::new(),
        0.0,
        Some(FormRef::new([0x42; 16], 0x800)),
    )
    .encode()
    .unwrap();
    let key = StorageKey::new("activation-count").unwrap();
    let mut host = host_with_custom_event_package(subscriber.as_str(), channel.as_str());

    host.pending_custom_events.push(CustomEvent {
        event: channel,
        sender,
        payload,
    });
    let stats = host.dispatch_custom_events();

    assert_eq!(stats.events, 1);
    assert_eq!(stats.deliveries, 1);
    assert_eq!(stats.commands_applied, 1);
    assert_eq!(stats.faults, 0);
    assert_eq!(
        host.principal_storage
            .values(&subscriber)
            .and_then(|values| values.get(&key)),
        Some(&PrincipalStorageValue::I64(1))
    );
}

#[test]
fn legacy_mod_event_runtime_registration_controls_live_routing() {
    let subscriber = PrincipalId::new("org.example.dynamic-subscriber").unwrap();
    let sender = PrincipalId::new("org.example.dynamic-publisher").unwrap();
    let channel =
        byroredux_sdk::event::legacy_skse_mod_event_id("SKICP_configManagerReady").unwrap();
    let key = StorageKey::new("activation-count").unwrap();
    let mut host = host_with_storage_package(subscriber.as_str());

    let subscribe =
        HostCommand::LegacyModEventSubscription(LegacyModEventSubscriptionCommand::Subscribe {
            event: channel.clone(),
            callback: "OnConfigManagerReady".to_owned(),
        });
    let mut registration_stats = ExtensionDispatchStats::default();
    let mut staged_registry = host.legacy_containers.get(&subscriber).unwrap().clone();
    let staged_array = staged_registry.create_array();
    let hosted = &mut host.components[0];
    hosted
        .instance
        .set_legacy_container_snapshot(staged_registry);
    apply_delivery_result(
        hosted,
        Ok(vec![subscribe]),
        LifecyclePhase::Activate,
        &subscriber,
        delivery_commit_context!(host, registration_stats),
    );
    assert_eq!(registration_stats.commands_applied, 1);
    assert!(host
        .legacy_containers
        .get(&subscriber)
        .unwrap()
        .contains(staged_array));
    assert!(host.components[0].custom_subscriptions.contains(&channel));
    assert_eq!(
        host.components[0]
            .instance
            .legacy_mod_event_callback(&channel),
        Some("OnConfigManagerReady")
    );

    host.pending_custom_events.push(CustomEvent {
        event: channel.clone(),
        sender: sender.clone(),
        payload: byroredux_sdk::event::LegacySkseModEventPayload::new(String::new(), 0.0, None)
            .encode()
            .unwrap(),
    });
    let delivered = host.dispatch_custom_events();
    assert_eq!(delivered.deliveries, 1);
    assert_eq!(delivered.commands_applied, 1);
    assert_eq!(
        host.principal_storage
            .values(&subscriber)
            .and_then(|values| values.get(&key)),
        Some(&PrincipalStorageValue::I64(1))
    );

    let unsubscribe =
        HostCommand::LegacyModEventSubscription(LegacyModEventSubscriptionCommand::Unsubscribe {
            event: channel.clone(),
        });
    let mut registration_stats = ExtensionDispatchStats::default();
    let hosted = &mut host.components[0];
    apply_delivery_result(
        hosted,
        Ok(vec![unsubscribe]),
        LifecyclePhase::Activate,
        &subscriber,
        delivery_commit_context!(host, registration_stats),
    );
    assert_eq!(registration_stats.commands_applied, 1);
    assert!(!host.components[0].custom_subscriptions.contains(&channel));
    assert_eq!(
        host.components[0]
            .instance
            .legacy_mod_event_callback(&channel),
        None
    );

    host.pending_custom_events.push(CustomEvent {
        event: channel,
        sender,
        payload: Vec::new(),
    });
    let suppressed = host.dispatch_custom_events();
    assert_eq!(suppressed.events, 1);
    assert_eq!(suppressed.deliveries, 0);
}

#[test]
fn invalid_legacy_registration_rejects_its_entire_callback_batch() {
    let principal = PrincipalId::new("org.example.atomic-registration").unwrap();
    let key = StorageKey::new("should-not-commit").unwrap();
    let mut host = host_with_storage_package(principal.as_str());
    let mut staged_registry = host.legacy_containers.get(&principal).unwrap().clone();
    let staged_array = staged_registry.create_array();
    let commands = vec![
        HostCommand::PrincipalStorage(byroredux_sdk::storage::PrincipalStorageCommand::Set {
            key: key.clone(),
            value: ExtensionValue::I64(1),
        }),
        HostCommand::LegacyModEventSubscription(LegacyModEventSubscriptionCommand::Subscribe {
            event: EventId::new("mod.org.example.atomic-registration.event.not-legacy").unwrap(),
            callback: "OnInvalid".to_owned(),
        }),
    ];
    let mut stats = ExtensionDispatchStats::default();
    let hosted = &mut host.components[0];
    hosted
        .instance
        .set_legacy_container_snapshot(staged_registry);
    apply_delivery_result(
        hosted,
        Ok(commands),
        LifecyclePhase::Activate,
        &principal,
        delivery_commit_context!(host, stats),
    );

    assert_eq!(stats.commands_applied, 0);
    assert_eq!(stats.faults, 1);
    assert!(host
        .principal_storage
        .values(&principal)
        .and_then(|values| values.get(&key))
        .is_none());
    assert!(host.components[0].custom_subscriptions.is_empty());
    assert!(!host
        .legacy_containers
        .get(&principal)
        .unwrap()
        .contains(staged_array));
}

#[test]
fn custom_event_queue_overflow_rejects_the_entire_deferred_batch() {
    let mut host = host_with_package("org.example.atomic");
    let principal = PrincipalId::new("org.example.atomic").unwrap();
    let event = EventId::new("mod.org.example.atomic.event.ready").unwrap();
    host.pending_custom_events = (0..MAX_PENDING_CUSTOM_EVENTS)
        .map(|_| CustomEvent {
            event: event.clone(),
            sender: principal.clone(),
            payload: Vec::new(),
        })
        .collect();
    let entity = EntityRef::new(1, 99).unwrap();
    let commands = vec![
        HostCommand::Component(byroredux_sdk::component::ExtensionCommand::IncrementI64 {
            entity,
            schema: ComponentSchemaId::new("example.activation-count").unwrap(),
            field: ComponentFieldId::new("count").unwrap(),
            delta: 1,
        }),
        HostCommand::PublishEvent(
            byroredux_sdk::event::PublishEventCommand::new(event, Vec::new()).unwrap(),
        ),
    ];
    let mut stats = ExtensionDispatchStats::default();
    let hosted = &mut host.components[0];
    apply_delivery_result(
        hosted,
        Ok(commands),
        LifecyclePhase::Activate,
        &principal,
        delivery_commit_context!(host, stats),
    );

    assert_eq!(host.pending_custom_events.len(), MAX_PENDING_CUSTOM_EVENTS);
    assert!(host
        .state
        .row(
            &principal,
            &ComponentSchemaId::new("example.activation-count").unwrap(),
            entity,
        )
        .is_none());
    assert_eq!(stats.commands_applied, 0);
    assert_eq!(stats.faults, 1);
    assert!(matches!(
        host.components[0].instance.status(),
        InstanceStatus::Quarantined(_)
    ));
}

#[test]
fn invalid_hit_damage_is_rejected_before_guest_delivery() {
    let mut host = host_with_hit_package("org.example.invalid-hit");
    let stats = host.dispatch_hits([RawHit {
        subject: 41,
        subject_form: None,
        aggressor: Some(7),
        aggressor_form: None,
        source: None,
        source_form: None,
        projectile: None,
        projectile_form: None,
        damage: f32::NAN,
        power_attack: false,
        sneak_attack: false,
        bash_attack: false,
        blocked: false,
    }]);
    assert_eq!(stats.events, 1);
    assert_eq!(stats.deliveries, 0);
    assert_eq!(stats.commands_applied, 0);
    assert_eq!(stats.faults, 1);
    assert_eq!(
        host.components[0].instance.status(),
        &InstanceStatus::Active
    );
}

#[test]
fn recurring_update_waits_full_interval_and_retains_overshoot() {
    let principal = PrincipalId::new("org.example.update").unwrap();
    let key = StorageKey::new("activation-count").unwrap();
    let mut host = host_with_update_package(principal.as_str());

    assert_eq!(
        host.dispatch_updates(0.04),
        ExtensionDispatchStats::default()
    );
    assert_eq!(
        host.dispatch_updates(0.04),
        ExtensionDispatchStats::default()
    );
    assert_eq!(host.dispatch_updates(0.04).commands_applied, 1);
    assert_eq!(
        host.principal_storage
            .values(&principal)
            .and_then(|values| values.get(&key)),
        Some(&PrincipalStorageValue::I64(1))
    );

    assert_eq!(host.dispatch_updates(0.35).deliveries, 1);
    assert_eq!(host.dispatch_updates(0.0).deliveries, 1);
    assert_eq!(
        host.principal_storage
            .values(&principal)
            .and_then(|values| values.get(&key)),
        Some(&PrincipalStorageValue::I64(3))
    );
}

#[test]
fn invalid_update_delta_is_a_host_fault_without_guest_quarantine() {
    let mut host = host_with_update_package("org.example.invalid-update");
    let stats = host.dispatch_updates(f32::NAN);

    assert_eq!(stats.faults, 1);
    assert_eq!(stats.deliveries, 0);
    assert_eq!(
        host.components[0].instance.status(),
        &InstanceStatus::Active
    );
    assert!(matches!(
        host.take_diagnostics().as_slice(),
        [ExtensionDiagnostic::Fault { extension, .. }]
            if extension.as_str() == "byro.engine"
    ));
}

#[test]
fn scheduler_update_adapter_advances_engine_owned_cadence() {
    let principal = PrincipalId::new("org.example.update-adapter").unwrap();
    let key = StorageKey::new("activation-count").unwrap();
    let slot = ExtensionHostSlot::from_host(host_with_update_package(principal.as_str()));
    let host = slot.host().unwrap();
    let mut world = World::new();
    world.insert_resource(slot);

    extension_update_dispatch_system(&world, 0.05);
    extension_update_dispatch_system(&world, 0.05);

    assert_eq!(
        host.lock()
            .unwrap()
            .principal_storage
            .values(&principal)
            .and_then(|values| values.get(&key)),
        Some(&PrincipalStorageValue::I64(1))
    );
}

#[test]
fn principal_storage_is_live_private_and_save_persistent() {
    let principal = PrincipalId::new("org.example.storage-save").unwrap();
    let key = StorageKey::new("activation-count").unwrap();
    let mut source = host_with_storage_package(principal.as_str());
    let activation = RawActivation {
        subject: 41,
        subject_form: None,
        activator: None,
        activator_form: None,
    };
    assert_eq!(
        source.dispatch_activations([activation]).commands_applied,
        1
    );
    assert_eq!(
        source.dispatch_activations([activation]).commands_applied,
        1
    );
    assert_eq!(
        source
            .principal_storage
            .values(&principal)
            .and_then(|values| values.get(&key)),
        Some(&PrincipalStorageValue::I64(2))
    );
    source
        .principal_storage
        .apply_batch(
            &principal,
            &[
                PrincipalStorageCommand::ArrayPush {
                    key: StorageKey::new("history").unwrap(),
                    value: ExtensionValue::String("Helgen".to_owned()),
                },
                PrincipalStorageCommand::MapSet {
                    key: StorageKey::new("aliases").unwrap(),
                    entry: "player".to_owned(),
                    value: ExtensionValue::String("Dragonborn".to_owned()),
                },
                PrincipalStorageCommand::SetInsert {
                    key: StorageKey::new("visited").unwrap(),
                    value: ExtensionValue::U64(7),
                },
            ],
        )
        .unwrap();

    let saved = source.capture_saved_state(&BTreeMap::new()).unwrap();
    assert_eq!(saved.principal_storage.len(), 1);
    let mut restored = host_with_storage_package(principal.as_str());
    restored
        .restore_saved_state(&saved, &BTreeMap::new())
        .unwrap();
    assert_eq!(
        restored
            .principal_storage
            .values(&principal)
            .and_then(|values| values.get(&StorageKey::new("history").unwrap())),
        Some(&PrincipalStorageValue::Array(vec![ExtensionValue::String(
            "Helgen".to_owned()
        )]))
    );
    assert_eq!(
        restored.dispatch_activations([activation]).commands_applied,
        1
    );
    assert_eq!(
        restored
            .principal_storage
            .values(&principal)
            .and_then(|values| values.get(&key)),
        Some(&PrincipalStorageValue::I64(3))
    );

    let mut unavailable =
        ExtensionHost::new(SandboxConfig::default(), ComponentStoreLimits::default()).unwrap();
    unavailable
        .restore_saved_state(&saved, &BTreeMap::new())
        .unwrap();
    assert_eq!(unavailable.retained_storage, saved.principal_storage);
    assert_eq!(
        unavailable
            .capture_saved_state(&BTreeMap::new())
            .unwrap()
            .principal_storage,
        saved.principal_storage
    );
    install_storage_package(&mut unavailable, principal.as_str());
    assert!(unavailable.retained_storage.is_empty());
    assert_eq!(
        unavailable
            .principal_storage
            .values(&principal)
            .and_then(|values| values.get(&key)),
        Some(&PrincipalStorageValue::I64(2))
    );
}

#[test]
fn legacy_script_principals_are_private_and_restore_as_active_packages() {
    let first = PrincipalId::new("legacy.scripts.first").unwrap();
    let second = PrincipalId::new("legacy.scripts.second").unwrap();
    let key = StorageKey::new("shared-name").unwrap();
    let mut source =
        ExtensionHost::new(SandboxConfig::default(), ComponentStoreLimits::default()).unwrap();
    source
        .register_legacy_script_principal(first.clone())
        .unwrap();
    source
        .register_legacy_script_principal(second.clone())
        .unwrap();
    source
        .principal_storage
        .apply_batch(
            &first,
            &[PrincipalStorageCommand::Set {
                key: key.clone(),
                value: ExtensionValue::I64(1),
            }],
        )
        .unwrap();
    source
        .principal_storage
        .apply_batch(
            &second,
            &[PrincipalStorageCommand::Set {
                key: key.clone(),
                value: ExtensionValue::I64(2),
            }],
        )
        .unwrap();

    let saved = source.capture_saved_state(&BTreeMap::new()).unwrap();
    let mut restored =
        ExtensionHost::new(SandboxConfig::default(), ComponentStoreLimits::default()).unwrap();
    restored
        .register_legacy_script_principal(first.clone())
        .unwrap();
    restored
        .register_legacy_script_principal(second.clone())
        .unwrap();
    restored
        .restore_saved_state(&saved, &BTreeMap::new())
        .unwrap();

    assert_eq!(
        restored
            .principal_storage
            .values(&first)
            .and_then(|values| values.get(&key)),
        Some(&PrincipalStorageValue::I64(1))
    );
    assert_eq!(
        restored
            .principal_storage
            .values(&second)
            .and_then(|values| values.get(&key)),
        Some(&PrincipalStorageValue::I64(2))
    );
    assert!(restored.retained_storage.is_empty());
}

#[test]
fn storage_util_aliases_are_case_folded_principal_private_and_reject_object_keys() {
    let first = PrincipalId::new("legacy.scripts.first-storage-util").unwrap();
    let second = PrincipalId::new("legacy.scripts.second-storage-util").unwrap();
    let mut host =
        ExtensionHost::new(SandboxConfig::default(), ComponentStoreLimits::default()).unwrap();
    host.register_legacy_script_principal(first.clone())
        .unwrap();
    host.register_legacy_script_principal(second.clone())
        .unwrap();

    assert_eq!(
        host.invoke_owned_papyrus_provider(
            Some(&first),
            PAPYRUS_STORAGE_UTIL_SET_INT_VALUE_ROUTE,
            &[
                ScriptValue::None,
                ScriptValue::String("Score".to_owned()),
                ScriptValue::Integer(7),
            ],
        )
        .unwrap(),
        ScriptValue::Integer(7)
    );
    assert_eq!(
        host.invoke_owned_papyrus_provider(
            Some(&first),
            PAPYRUS_STORAGE_UTIL_GET_INT_VALUE_ROUTE,
            &[
                ScriptValue::None,
                ScriptValue::String("sCoRe".to_owned()),
                ScriptValue::Integer(-1),
            ],
        )
        .unwrap(),
        ScriptValue::Integer(7)
    );
    assert_eq!(
        host.invoke_owned_papyrus_provider(
            Some(&second),
            PAPYRUS_STORAGE_UTIL_GET_INT_VALUE_ROUTE,
            &[
                ScriptValue::None,
                ScriptValue::String("score".to_owned()),
                ScriptValue::Integer(-1),
            ],
        )
        .unwrap(),
        ScriptValue::Integer(-1)
    );
    assert_eq!(
        host.invoke_owned_papyrus_provider(
            Some(&first),
            PAPYRUS_STORAGE_UTIL_UNSET_INT_VALUE_ROUTE,
            &[ScriptValue::None, ScriptValue::String("score".to_owned()),],
        )
        .unwrap(),
        ScriptValue::Boolean(true)
    );

    assert_eq!(
        host.invoke_owned_papyrus_provider(
            Some(&first),
            PAPYRUS_STORAGE_UTIL_ADJUST_INT_VALUE_ROUTE,
            &[
                ScriptValue::None,
                ScriptValue::String("visits".to_owned()),
                ScriptValue::Integer(2),
            ],
        )
        .unwrap(),
        ScriptValue::Integer(2)
    );
    assert_eq!(
        host.invoke_owned_papyrus_provider(
            Some(&first),
            PAPYRUS_STORAGE_UTIL_SET_FLOAT_VALUE_ROUTE,
            &[
                ScriptValue::None,
                ScriptValue::String("Ratio".to_owned()),
                ScriptValue::Float(1.25),
            ],
        )
        .unwrap(),
        ScriptValue::Float(1.25)
    );
    assert_eq!(
        host.invoke_owned_papyrus_provider(
            Some(&first),
            PAPYRUS_STORAGE_UTIL_ADJUST_FLOAT_VALUE_ROUTE,
            &[
                ScriptValue::None,
                ScriptValue::String("ratio".to_owned()),
                ScriptValue::Float(0.5),
            ],
        )
        .unwrap(),
        ScriptValue::Float(1.75)
    );
    let stored_form = FormRef::new([0x4d; 16], 0x1020_3040);
    assert_eq!(
        host.invoke_owned_papyrus_provider(
            Some(&first),
            PAPYRUS_STORAGE_UTIL_SET_FORM_VALUE_ROUTE,
            &[
                ScriptValue::None,
                ScriptValue::String("Owner".to_owned()),
                ScriptValue::Form(stored_form),
            ],
        )
        .unwrap(),
        ScriptValue::Form(stored_form)
    );
    assert_eq!(
        host.invoke_owned_papyrus_provider(
            Some(&first),
            PAPYRUS_STORAGE_UTIL_GET_FORM_VALUE_ROUTE,
            &[ScriptValue::None, ScriptValue::String("owner".to_owned())],
        )
        .unwrap(),
        ScriptValue::Form(stored_form)
    );
    assert_eq!(
        host.invoke_owned_papyrus_provider(
            Some(&first),
            PAPYRUS_STORAGE_UTIL_PLUCK_FORM_VALUE_ROUTE,
            &[ScriptValue::None, ScriptValue::String("owner".to_owned())],
        )
        .unwrap(),
        ScriptValue::Form(stored_form)
    );
    assert_eq!(
        host.invoke_owned_papyrus_provider(
            Some(&first),
            PAPYRUS_STORAGE_UTIL_GET_FORM_VALUE_ROUTE,
            &[ScriptValue::None, ScriptValue::String("owner".to_owned())],
        )
        .unwrap(),
        ScriptValue::None
    );

    let object = ScriptValue::Form(FormRef::new([9; 16], 1));
    assert!(matches!(
        host.invoke_owned_papyrus_provider(
            Some(&first),
            PAPYRUS_STORAGE_UTIL_HAS_INT_VALUE_ROUTE,
            &[object, ScriptValue::String("score".to_owned())],
        ),
        Err(ExtensionHostError::ScriptFunctionUnavailable { .. })
    ));
    assert!(matches!(
        host.invoke_owned_papyrus_provider(
            None,
            PAPYRUS_STORAGE_UTIL_HAS_INT_VALUE_ROUTE,
            &[ScriptValue::None, ScriptValue::String("score".to_owned()),],
        ),
        Err(ExtensionHostError::ScriptFunctionUnavailable { .. })
    ));
}

#[test]
fn storage_util_form_filter_uses_live_content_metadata() {
    let principal = PrincipalId::new("legacy.scripts.form-filter").unwrap();
    let source = 1_u128.to_be_bytes();
    let weapon = FormRef::new(source, 0x1234);
    let armor = FormRef::new(source, 0x1235);
    let catalog = ContentCatalog::new_with_metadata(
        vec![byroredux_sdk::content::PluginInfo::new(
            "Skyrim.esm",
            source,
            byroredux_sdk::content::PluginKind::Regular,
        )
        .unwrap()],
        vec![vec![]],
        vec![vec![(0x1234, *b"WEAP"), (0x1235, *b"ARMO")]],
    )
    .unwrap();
    let mut host =
        ExtensionHost::new(SandboxConfig::default(), ComponentStoreLimits::default()).unwrap();
    host.register_legacy_script_principal(principal.clone())
        .unwrap();
    host.set_content_catalog(Arc::new(catalog));
    let add_route = "byro.storage.compat.storage-util.list-form-add";
    for form in [Some(weapon), None, Some(armor)] {
        let value = form.map_or(ScriptValue::None, ScriptValue::Form);
        host.invoke_owned_papyrus_provider(
            Some(&principal),
            add_route,
            &[
                ScriptValue::None,
                ScriptValue::String("Owners".to_owned()),
                value,
            ],
        )
        .unwrap();
    }
    assert_eq!(
        host.invoke_owned_papyrus_provider(
            Some(&principal),
            PAPYRUS_STORAGE_UTIL_FORM_FILTER_BY_TYPES_ROUTE,
            &[
                ScriptValue::None,
                ScriptValue::String("owners".to_owned()),
                ScriptValue::IntegerArray(vec![41]),
            ],
        )
        .unwrap(),
        ScriptValue::FormArray(vec![Some(weapon)])
    );
    assert_eq!(
        host.invoke_owned_papyrus_provider(
            Some(&principal),
            PAPYRUS_STORAGE_UTIL_FORM_FILTER_BY_TYPE_ROUTE,
            &[
                ScriptValue::None,
                ScriptValue::String("owners".to_owned()),
                ScriptValue::Integer(41),
                ScriptValue::Boolean(false),
            ],
        )
        .unwrap(),
        ScriptValue::FormArray(vec![Some(armor)])
    );
}

#[test]
fn input_aliases_read_the_live_engine_binding_snapshot() {
    let mut host =
        ExtensionHost::new(SandboxConfig::default(), ComponentStoreLimits::default()).unwrap();
    host.set_input_bindings(vec![PapyrusInputBinding {
        control: "Forward".to_owned(),
        device_type: 0,
        keycode: 17,
    }]);
    assert_eq!(
        host.invoke_owned_papyrus_provider(
            None,
            PAPYRUS_INPUT_GET_MAPPED_KEY_ROUTE,
            &[ScriptValue::String("forward".to_owned())],
        )
        .unwrap(),
        ScriptValue::Integer(17)
    );
    assert_eq!(
        host.invoke_owned_papyrus_provider(
            None,
            PAPYRUS_INPUT_GET_MAPPED_KEY_ROUTE,
            &[
                ScriptValue::String("Forward".to_owned()),
                ScriptValue::Integer(2),
            ],
        )
        .unwrap(),
        ScriptValue::Integer(0xff)
    );
    assert_eq!(
        host.invoke_owned_papyrus_provider(
            None,
            PAPYRUS_INPUT_GET_MAPPED_CONTROL_ROUTE,
            &[ScriptValue::Integer(17)],
        )
        .unwrap(),
        ScriptValue::String("Forward".to_owned())
    );
    assert_eq!(
        host.invoke_owned_papyrus_provider(
            None,
            PAPYRUS_INPUT_GET_MAPPED_CONTROL_ROUTE,
            &[ScriptValue::Integer(99)],
        )
        .unwrap(),
        ScriptValue::String(String::new())
    );
}

#[test]
fn game_get_player_returns_stable_engine_entity_handle_or_none() {
    let mut host =
        ExtensionHost::new(SandboxConfig::default(), ComponentStoreLimits::default()).unwrap();
    assert_eq!(
        host.invoke_owned_papyrus_provider(None, PAPYRUS_GAME_GET_PLAYER_ROUTE, &[])
            .unwrap(),
        ScriptValue::None
    );

    host.set_player_entity(Some(41));
    let first = host
        .invoke_owned_papyrus_provider(None, PAPYRUS_GAME_GET_PLAYER_ROUTE, &[])
        .unwrap();
    let second = host
        .invoke_owned_papyrus_provider(None, PAPYRUS_GAME_GET_PLAYER_ROUTE, &[])
        .unwrap();
    let ScriptValue::Entity(handle) = first else {
        panic!("Game.GetPlayer should return an entity handle when a player exists");
    };
    assert_eq!(second, ScriptValue::Entity(handle));
    assert_eq!(host.handles.resolve(handle), Some(41));

    host.set_player_entity(None);
    assert_eq!(
        host.invoke_owned_papyrus_provider(None, PAPYRUS_GAME_GET_PLAYER_ROUTE, &[])
            .unwrap(),
        ScriptValue::None
    );
    assert!(matches!(
        host.invoke_owned_papyrus_provider(
            None,
            PAPYRUS_GAME_GET_PLAYER_ROUTE,
            &[ScriptValue::Integer(1)]
        ),
        Err(ExtensionHostError::ScriptFunctionUnavailable { .. })
    ));
}

#[test]
fn player_entity_sync_publishes_the_live_engine_body() {
    let slot = ExtensionHostSlot::initialize_default();
    let host = slot.host().unwrap();
    let mut world = World::new();
    let player = world.spawn();
    world.insert_resource(crate::systems::PlayerEntity(Some(player)));
    world.insert_resource(slot);

    extension_player_entity_sync_system(&world, 0.0);

    let mut host = host.lock().unwrap();
    let value = host
        .invoke_owned_papyrus_provider(None, PAPYRUS_GAME_GET_PLAYER_ROUTE, &[])
        .unwrap();
    let ScriptValue::Entity(handle) = value else {
        panic!("player sync should expose an entity handle");
    };
    assert_eq!(host.handles.resolve(handle), Some(player));
}

#[test]
fn input_binding_sync_publishes_normalized_keyboard_actions() {
    let slot = ExtensionHostSlot::initialize_default();
    let host = slot.host().unwrap();
    let mut world = World::new();
    world.insert_resource(crate::interaction::ActionBindings::default());
    world.insert_resource(slot);

    extension_input_bindings_sync_system(&world, 0.0);

    let host = host.lock().unwrap();
    assert_eq!(
        adapt_papyrus_input_get_mapped_key(&host.input_bindings, "Forward", 0xff),
        17
    );
    assert_eq!(
        adapt_papyrus_input_get_mapped_control(&host.input_bindings, 15),
        "Quick Inventory"
    );
}

#[test]
fn ui_alias_reads_the_active_engine_menu_snapshot() {
    let mut host =
        ExtensionHost::new(SandboxConfig::default(), ComponentStoreLimits::default()).unwrap();
    host.set_ui_menu_snapshot(PapyrusUiMenuSnapshot {
        active_menu: Some("InventoryMenu".to_owned()),
        visible: true,
    });
    assert_eq!(
        host.invoke_owned_papyrus_provider(
            None,
            PAPYRUS_UI_IS_MENU_OPEN_ROUTE,
            &[ScriptValue::String("InventoryMenu".to_owned())],
        )
        .unwrap(),
        ScriptValue::Boolean(true)
    );
    assert_eq!(
        host.invoke_owned_papyrus_provider(
            None,
            PAPYRUS_UI_IS_MENU_OPEN_ROUTE,
            &[ScriptValue::String("PauseMenu".to_owned())],
        )
        .unwrap(),
        ScriptValue::Boolean(false)
    );
    host.set_ui_menu_snapshot(PapyrusUiMenuSnapshot {
        active_menu: Some("InventoryMenu".to_owned()),
        visible: false,
    });
    assert_eq!(
        host.invoke_owned_papyrus_provider(
            None,
            PAPYRUS_UI_IS_MENU_OPEN_ROUTE,
            &[ScriptValue::String("InventoryMenu".to_owned())],
        )
        .unwrap(),
        ScriptValue::Boolean(false)
    );
}

#[test]
fn ui_menu_sync_clears_hidden_or_absent_menus() {
    let slot = ExtensionHostSlot::initialize_default();
    let host = slot.host().unwrap();
    let mut world = World::new();
    world.insert_resource(slot);

    extension_ui_menu_sync(&world, Some("InventoryMenu"), true);
    extension_ui_menu_sync(&world, Some("InventoryMenu"), false);

    let host = host.lock().unwrap();
    assert!(!adapt_papyrus_ui_is_menu_open(
        &host.ui_menu_snapshot,
        "InventoryMenu"
    ));
}

#[test]
fn jcontainers_aliases_cover_typed_nested_values_negative_indices_and_isolation() {
    let first = PrincipalId::new("legacy.scripts.first-jcontainers").unwrap();
    let second = PrincipalId::new("legacy.scripts.second-jcontainers").unwrap();
    let mut host =
        ExtensionHost::new(SandboxConfig::default(), ComponentStoreLimits::default()).unwrap();
    host.register_legacy_script_principal(first.clone())
        .unwrap();
    host.register_legacy_script_principal(second.clone())
        .unwrap();
    let route = |name: &str| format!("{PAPYRUS_LEGACY_CONTAINERS_ROUTE_PREFIX}{name}");

    let array = host
        .invoke_owned_papyrus_provider(Some(&first), &route("jarray-object"), &[])
        .unwrap();
    assert_eq!(array, ScriptValue::Integer(1));
    for value in [10, 20] {
        host.invoke_owned_papyrus_provider(
            Some(&first),
            &route("jarray-add-int"),
            &[array.clone(), ScriptValue::Integer(value)],
        )
        .unwrap();
    }
    host.invoke_owned_papyrus_provider(
        Some(&first),
        &route("jarray-add-int"),
        &[
            array.clone(),
            ScriptValue::Integer(15),
            ScriptValue::Integer(-2),
        ],
    )
    .unwrap();
    assert_eq!(
        host.invoke_owned_papyrus_provider(
            Some(&first),
            &route("jarray-get-int"),
            &[array.clone(), ScriptValue::Integer(-1)],
        )
        .unwrap(),
        ScriptValue::Integer(20)
    );

    let map = host
        .invoke_owned_papyrus_provider(Some(&first), &route("jmap-object"), &[])
        .unwrap();
    let form = FormRef::new([4; 16], 0x55);
    for (suffix, value) in [
        ("int", ScriptValue::Integer(7)),
        ("flt", ScriptValue::Float(2.5)),
        ("str", ScriptValue::String("ready".to_owned())),
        ("form", ScriptValue::Form(form)),
        ("obj", array.clone()),
    ] {
        host.invoke_owned_papyrus_provider(
            Some(&first),
            &route(&format!("jmap-set-{suffix}")),
            &[map.clone(), ScriptValue::String(suffix.to_owned()), value],
        )
        .unwrap();
    }
    assert_eq!(
        host.invoke_owned_papyrus_provider(
            Some(&first),
            &route("jmap-get-form"),
            &[map.clone(), ScriptValue::String("form".to_owned())],
        )
        .unwrap(),
        ScriptValue::Form(form)
    );
    assert_eq!(
        host.invoke_owned_papyrus_provider(
            Some(&first),
            &route("jmap-get-obj"),
            &[map.clone(), ScriptValue::String("obj".to_owned())],
        )
        .unwrap(),
        array
    );
    assert_eq!(
        host.invoke_owned_papyrus_provider(
            Some(&first),
            &route("jvalue-is-array"),
            &[array.clone()],
        )
        .unwrap(),
        ScriptValue::Boolean(true)
    );
    for operation in ["jvalue-shallow-copy", "jvalue-deep-copy"] {
        let copy = host
            .invoke_owned_papyrus_provider(Some(&first), &route(operation), &[map.clone()])
            .unwrap();
        assert_eq!(
            host.invoke_owned_papyrus_provider(
                Some(&first),
                &route("jvalue-is-map"),
                &[copy.clone()],
            )
            .unwrap(),
            ScriptValue::Boolean(true)
        );
        host.invoke_owned_papyrus_provider(Some(&first), &route("jvalue-release"), &[copy])
            .unwrap();
    }
    assert_eq!(
        host.invoke_owned_papyrus_provider(
            Some(&second),
            &route("jvalue-count"),
            &[ScriptValue::Integer(1)],
        )
        .unwrap(),
        ScriptValue::Integer(0)
    );
    assert_eq!(
        host.invoke_owned_papyrus_provider(
            Some(&first),
            &route("jvalue-retain"),
            &[map.clone(), ScriptValue::String("fixture-owner".to_owned())],
        )
        .unwrap(),
        map
    );
    assert_eq!(
        host.invoke_owned_papyrus_provider(
            Some(&first),
            &route("jvalue-release"),
            &[array.clone()],
        )
        .unwrap(),
        ScriptValue::Integer(0)
    );
    assert_eq!(
        host.invoke_owned_papyrus_provider(
            Some(&first),
            &route("jvalue-is-exists"),
            &[array.clone()],
        )
        .unwrap(),
        ScriptValue::Boolean(true)
    );
    assert_eq!(
        host.invoke_owned_papyrus_provider(
            Some(&first),
            &route("jmap-remove-key"),
            &[map.clone(), ScriptValue::String("obj".to_owned())],
        )
        .unwrap(),
        ScriptValue::Boolean(true)
    );
    assert_eq!(
        host.invoke_owned_papyrus_provider(Some(&first), &route("jvalue-is-exists"), &[array],)
            .unwrap(),
        ScriptValue::Boolean(false)
    );
    host.invoke_owned_papyrus_provider(
        Some(&first),
        &route("jvalue-release-objects-with-tag"),
        &[ScriptValue::String("fixture-owner".to_owned())],
    )
    .unwrap();
    assert_eq!(
        host.invoke_owned_papyrus_provider(Some(&first), &route("jvalue-is-exists"), &[map],)
            .unwrap(),
        ScriptValue::Boolean(false)
    );
    assert!(matches!(
        host.invoke_owned_papyrus_provider(None, &route("jarray-object"), &[]),
        Err(ExtensionHostError::ScriptFunctionUnavailable { .. })
    ));
}

#[test]
fn mod_event_aliases_build_typed_principal_owned_events() {
    let first = PrincipalId::new("legacy.scripts.first-mod-event").unwrap();
    let second = PrincipalId::new("legacy.scripts.second-mod-event").unwrap();
    let mut host =
        ExtensionHost::new(SandboxConfig::default(), ComponentStoreLimits::default()).unwrap();
    host.register_legacy_script_principal(first.clone())
        .unwrap();
    host.register_legacy_script_principal(second.clone())
        .unwrap();
    let route = |name: &str| format!("{PAPYRUS_MOD_EVENT_ROUTE_PREFIX}mod-event-{name}");

    let handle = host
        .invoke_owned_papyrus_provider(
            Some(&first),
            &route("create"),
            &[ScriptValue::String("ByroReady".to_owned())],
        )
        .unwrap();
    assert_eq!(handle, ScriptValue::Integer(1));
    for (operation, value) in [
        ("push-bool", ScriptValue::Boolean(true)),
        ("push-int", ScriptValue::Integer(-7)),
        ("push-float", ScriptValue::Float(2.5)),
        ("push-string", ScriptValue::String("ready".to_owned())),
        ("push-form", ScriptValue::None),
    ] {
        host.invoke_owned_papyrus_provider(
            Some(&first),
            &route(operation),
            &[handle.clone(), value],
        )
        .unwrap();
    }
    assert_eq!(
        host.invoke_owned_papyrus_provider(Some(&first), &route("send"), &[handle.clone()])
            .unwrap(),
        ScriptValue::Boolean(true)
    );
    assert_eq!(host.pending_custom_events.len(), 1);
    let event = &host.pending_custom_events[0];
    assert_eq!(event.sender, first);
    assert_eq!(
        byroredux_sdk::event::legacy_skse_mod_event_name(&event.event).as_deref(),
        Some("ByroReady")
    );
    assert_eq!(
        byroredux_sdk::event::LegacySkseVariadicModEventPayload::decode(&event.payload)
            .unwrap()
            .arguments,
        [
            LegacySkseModEventValue::Bool(true),
            LegacySkseModEventValue::Int(-7),
            LegacySkseModEventValue::float(2.5),
            LegacySkseModEventValue::String("ready".to_owned()),
            LegacySkseModEventValue::Form(None),
        ]
    );
    assert_eq!(
        host.invoke_owned_papyrus_provider(Some(&first), &route("send"), &[handle.clone()])
            .unwrap(),
        ScriptValue::Boolean(false)
    );
    assert_eq!(
        host.invoke_owned_papyrus_provider(
            Some(&second),
            &route("create"),
            &[ScriptValue::String("Other".to_owned())],
        )
        .unwrap(),
        ScriptValue::Integer(1)
    );
    assert!(matches!(
        host.invoke_owned_papyrus_provider(
            None,
            &route("create"),
            &[ScriptValue::String("Denied".to_owned())]
        ),
        Err(ExtensionHostError::ScriptFunctionUnavailable { .. })
    ));
}

#[test]
fn legacy_container_objects_are_principal_local_and_save_persistent() {
    let principal = PrincipalId::new("org.example.storage").unwrap();
    let mut source = host_with_storage_package(principal.as_str());
    let registry = source.legacy_containers.get_mut(&principal).unwrap();
    let array = registry.create_array();
    let map = registry.create_map();
    assert!(registry.array_add(
        array,
        byroredux_sdk::legacy_containers::LegacyContainerValue::Int(42),
        None,
    ));
    assert!(registry.map_set(
        map,
        "items".to_owned(),
        byroredux_sdk::legacy_containers::LegacyContainerValue::Object(array),
    ));
    assert_eq!(registry.retain(map, Some("org.example.storage")), map);

    let saved = source.capture_saved_state(&BTreeMap::new()).unwrap();
    assert_eq!(saved.format_version, EXTENSION_STATE_FORMAT_VERSION);
    assert_eq!(saved.legacy_containers.len(), 1);
    let mut restored = host_with_storage_package(principal.as_str());
    restored
        .restore_saved_state(&saved, &BTreeMap::new())
        .unwrap();
    let registry = restored.legacy_containers.get(&principal).unwrap();
    assert_eq!(registry.count(array), 1);
    assert_eq!(
        registry.map_get(map, "items"),
        Some(&byroredux_sdk::legacy_containers::LegacyContainerValue::Object(array))
    );
    assert_eq!(registry.retain_count(map), 1);
    assert_eq!(registry.retention_tag(map), Some("org.example.storage"));

    let mut unavailable =
        ExtensionHost::new(SandboxConfig::default(), ComponentStoreLimits::default()).unwrap();
    unavailable
        .restore_saved_state(&saved, &BTreeMap::new())
        .unwrap();
    assert_eq!(unavailable.retained_legacy_containers.len(), 1);
    assert_eq!(
        unavailable
            .capture_saved_state(&BTreeMap::new())
            .unwrap()
            .legacy_containers,
        saved.legacy_containers
    );
    install_storage_package(&mut unavailable, principal.as_str());
    assert!(unavailable.retained_legacy_containers.is_empty());
    assert_eq!(
        unavailable
            .legacy_containers
            .get(&principal)
            .unwrap()
            .count(array),
        1
    );
}

#[test]
fn unfinished_mod_event_builder_survives_extension_save_round_trip() {
    let principal = PrincipalId::new("legacy.scripts.saved-mod-event").unwrap();
    let mut source =
        ExtensionHost::new(SandboxConfig::default(), ComponentStoreLimits::default()).unwrap();
    source
        .register_legacy_script_principal(principal.clone())
        .unwrap();
    let route = |name: &str| format!("{PAPYRUS_MOD_EVENT_ROUTE_PREFIX}mod-event-{name}");
    let handle = source
        .invoke_owned_papyrus_provider(
            Some(&principal),
            &route("create"),
            &[ScriptValue::String("SavedEvent".to_owned())],
        )
        .unwrap();
    source
        .invoke_owned_papyrus_provider(
            Some(&principal),
            &route("push-string"),
            &[handle.clone(), ScriptValue::String("restored".to_owned())],
        )
        .unwrap();
    let saved = source.capture_saved_state(&BTreeMap::new()).unwrap();

    let mut restored =
        ExtensionHost::new(SandboxConfig::default(), ComponentStoreLimits::default()).unwrap();
    restored
        .register_legacy_script_principal(principal.clone())
        .unwrap();
    restored
        .restore_saved_state(&saved, &BTreeMap::new())
        .unwrap();
    assert_eq!(
        restored
            .invoke_owned_papyrus_provider(Some(&principal), &route("send"), &[handle],)
            .unwrap(),
        ScriptValue::Boolean(true)
    );
    let payload = byroredux_sdk::event::LegacySkseVariadicModEventPayload::decode(
        &restored.pending_custom_events[0].payload,
    )
    .unwrap();
    assert_eq!(
        payload.arguments,
        [LegacySkseModEventValue::String("restored".to_owned())]
    );
}

#[test]
fn pre_container_extension_state_remains_loadable() {
    let mut version_two = ExtensionStateSnapshot::default();
    version_two.format_version = MIN_EXTENSION_STATE_FORMAT_VERSION;
    version_two.legacy_containers.clear();
    let mut host =
        ExtensionHost::new(SandboxConfig::default(), ComponentStoreLimits::default()).unwrap();
    host.restore_saved_state(&version_two, &BTreeMap::new())
        .unwrap();
    assert!(host.legacy_containers.is_empty());
    assert_eq!(
        host.capture_saved_state(&BTreeMap::new())
            .unwrap()
            .format_version,
        EXTENSION_STATE_FORMAT_VERSION
    );
}

#[test]
fn granted_manifest_console_command_registers_and_commits_deferred_state() {
    let id = "org.example.console";
    let manifest = console_manifest(id);
    let mut artifacts = ExtensionArtifacts::new();
    artifacts.insert(
        ComponentId::new("runtime").unwrap(),
        wat::parse_str(STORAGE_COMPONENT).unwrap(),
    );
    let mut extension_host =
        ExtensionHost::new(SandboxConfig::default(), ComponentStoreLimits::default()).unwrap();
    extension_host
        .install_package(&manifest, &artifacts, console_grants())
        .unwrap();
    let slot = ExtensionHostSlot::from_host(extension_host);
    let host = slot.host().unwrap();
    let mut world = World::new();
    world.insert_resource(slot);
    let mut registry = CommandRegistry::new();
    register_console_commands(&world, &mut registry);
    assert_eq!(
        registry.list(),
        vec![("ext.org.example.console.bump", "Increment the test counter")]
    );
    world.insert_resource(registry);

    let output = world
        .resource::<CommandRegistry>()
        .execute(&world, "ext.org.example.console.bump ignored");
    assert_eq!(output.lines, vec!["OK"]);
    let principal = PrincipalId::new(id).unwrap();
    let key = StorageKey::new("activation-count").unwrap();
    assert_eq!(
        host.lock()
            .unwrap()
            .principal_storage
            .values(&principal)
            .and_then(|values| values.get(&key)),
        Some(&PrincipalStorageValue::I64(1))
    );
}

#[test]
fn granted_typed_script_function_commits_side_effects_before_publishing_result() {
    let id = "org.example.functions";
    let manifest = script_function_manifest(id);
    let mut artifacts = ExtensionArtifacts::new();
    artifacts.insert(
        ComponentId::new("runtime").unwrap(),
        wat::parse_str(script_function_component()).unwrap(),
    );
    let mut host =
        ExtensionHost::new(SandboxConfig::default(), ComponentStoreLimits::default()).unwrap();
    host.install_package(&manifest, &artifacts, script_function_grants())
        .unwrap();

    let value = host
        .invoke_script_function(
            "ext.org.example.functions.answer",
            &[ScriptValue::Integer(7)],
        )
        .unwrap();
    assert_eq!(value, ScriptValue::Integer(42));
    let principal = PrincipalId::new(id).unwrap();
    let key = StorageKey::new("activation-count").unwrap();
    assert_eq!(
        host.principal_storage
            .values(&principal)
            .and_then(|values| values.get(&key)),
        Some(&PrincipalStorageValue::I64(1))
    );

    let error = host
        .invoke_script_function(
            "ext.org.example.functions.answer",
            &[ScriptValue::Boolean(true)],
        )
        .unwrap_err();
    assert!(error.to_string().contains("invalid value or type"));
    assert_eq!(
        host.principal_storage
            .values(&principal)
            .and_then(|values| values.get(&key)),
        Some(&PrincipalStorageValue::I64(1))
    );
}

#[test]
fn papyrus_alias_collision_rejects_the_second_principal_atomically() {
    let first = script_function_manifest("org.example.first");
    let second = script_function_manifest("org.example.second");
    let mut artifacts = ExtensionArtifacts::new();
    artifacts.insert(
        ComponentId::new("runtime").unwrap(),
        wat::parse_str(script_function_component()).unwrap(),
    );
    let mut host =
        ExtensionHost::new(SandboxConfig::default(), ComponentStoreLimits::default()).unwrap();
    host.install_package(&first, &artifacts, script_function_grants())
        .unwrap();

    let error = host
        .install_package(&second, &artifacts, script_function_grants())
        .unwrap_err();
    assert!(matches!(error, ExtensionHostError::PapyrusProviderAlias(_)));
    assert!(host
        .invoke_script_function("ext.org.example.first.answer", &[ScriptValue::Integer(7)],)
        .is_ok());
    assert!(matches!(
        host.invoke_script_function("ext.org.example.second.answer", &[ScriptValue::Integer(7)],),
        Err(ExtensionHostError::UnknownScriptFunction(_))
    ));
}

#[test]
fn source_obscript_calls_typed_function_without_a_script_extender() {
    let id = "org.example.functions";
    let manifest = script_function_manifest(id);
    let mut artifacts = ExtensionArtifacts::new();
    artifacts.insert(
        ComponentId::new("runtime").unwrap(),
        wat::parse_str(script_function_component()).unwrap(),
    );
    let mut extension_host =
        ExtensionHost::new(SandboxConfig::default(), ComponentStoreLimits::default()).unwrap();
    extension_host
        .install_package(&manifest, &artifacts, script_function_grants())
        .unwrap();
    let slot = ExtensionHostSlot::from_host(extension_host);
    let live_host = slot.host().unwrap();
    let mut world = World::new();
    byroredux_scripting::register(&mut world);
    world.insert_resource(slot);
    sync_extension_script_function_invoker(&world);
    assert!(world
        .resource::<byroredux_scripting::PapyrusProviderRuntime>()
        .catalog()
        .resolve("byrofixture", "answer")
        .is_some());

    let source = r#"
            begin GameMode
                set answer to ext.org.example.functions.answer integer:7
            end
        "#;
    let script = ScriptRecord {
        source: Some(source.to_owned()),
        locals: vec![ScriptLocalVar {
            index: 0,
            var_type: 2,
            name: "answer".to_owned(),
        }],
        ..Default::default()
    };
    let entity = world.spawn();
    assert!(byroredux_scripting::attach_legacy_obscript_program(
        &mut world, entity, &script, None,
    ));
    byroredux_scripting::legacy_obscript_load_order_system(&world, 0.0);

    assert_eq!(
        world
            .get::<byroredux_scripting::ScriptVariables>(entity)
            .unwrap()
            .get_by_name("answer"),
        Some(42.0)
    );
    let principal = PrincipalId::new(id).unwrap();
    let key = StorageKey::new("activation-count").unwrap();
    assert_eq!(
        live_host
            .lock()
            .unwrap()
            .principal_storage
            .values(&principal)
            .and_then(|values| values.get(&key)),
        Some(&PrincipalStorageValue::I64(1))
    );
}

#[test]
fn source_papyrus_calls_manifest_provider_without_a_script_extender() {
    let id = "org.example.functions";
    let manifest = script_function_manifest(id);
    let mut artifacts = ExtensionArtifacts::new();
    artifacts.insert(
        ComponentId::new("runtime").unwrap(),
        wat::parse_str(script_function_component()).unwrap(),
    );
    let mut extension_host =
        ExtensionHost::new(SandboxConfig::default(), ComponentStoreLimits::default()).unwrap();
    extension_host
        .install_package(&manifest, &artifacts, script_function_grants())
        .unwrap();
    let slot = ExtensionHostSlot::from_host(extension_host);
    let live_host = slot.host().unwrap();
    let mut world = World::new();
    byroredux_scripting::register(&mut world);
    world.insert_resource(slot);
    sync_extension_script_function_invoker(&world);
    let projected_entity = world.spawn();
    let projected_handle = world
        .resource::<byroredux_scripting::PapyrusProviderRuntime>()
        .entity_resolver()
        .unwrap()(projected_entity)
    .unwrap();
    assert_eq!(
        live_host.lock().unwrap().handles.by_entity[&projected_entity],
        projected_handle
    );

    let source = r#"
            ScriptName ProviderFixture
            Event OnLoad()
                Int answer
                answer = ByroFixture.Answer(7)
            EndEvent
        "#;
    let (script, errors) = byroredux_papyrus::parse_script(source).unwrap();
    assert!(errors.is_empty(), "{errors:?}");
    let catalog = world
        .resource::<byroredux_scripting::PapyrusProviderRuntime>()
        .catalog();
    let program = byroredux_scripting::lower_provider_program(&script, &catalog)
        .unwrap()
        .unwrap();
    let entity = world.spawn();
    byroredux_scripting::attach_papyrus_provider_program(&mut world, entity, program);
    world.insert(entity, byroredux_scripting::OnCellLoadEvent);

    byroredux_scripting::papyrus_provider_system(&world, 0.0);

    let principal = PrincipalId::new(id).unwrap();
    let key = StorageKey::new("activation-count").unwrap();
    assert_eq!(
        live_host
            .lock()
            .unwrap()
            .principal_storage
            .values(&principal)
            .and_then(|values| values.get(&key)),
        Some(&PrincipalStorageValue::I64(1))
    );
}

#[test]
fn source_papyrus_runs_principal_private_storage_util_across_wait() {
    let principal = PrincipalId::new("legacy.scripts.storage-util-source").unwrap();
    let mut extension_host =
        ExtensionHost::new(SandboxConfig::default(), ComponentStoreLimits::default()).unwrap();
    extension_host
        .register_legacy_script_principal(principal.clone())
        .unwrap();
    let slot = ExtensionHostSlot::from_host(extension_host);
    let live_host = slot.host().unwrap();
    let mut world = World::new();
    byroredux_scripting::register(&mut world);
    world.insert_resource(slot);
    sync_extension_script_function_invoker(&world);

    let source = r#"
            ScriptName StorageFixture
            Event OnLoad()
                StorageUtil.SetIntValue(None, "Count", 3)
                StorageUtil.SetFloatValue(None, "Ratio", 1.25)
                StorageUtil.SetIntValue(None, "OneShot", 11)
                StorageUtil.SetFloatValue(None, "OneFloat", 2.5)
                StorageUtil.SetStringValue(None, "OneString", "go")
                Utility.Wait(0.0)
                Int value
                value = StorageUtil.GetIntValue(None, "count", -1)
                Int visits
                visits = StorageUtil.AdjustIntValue(None, "visits", 2)
                Float ratio
                ratio = StorageUtil.AdjustFloatValue(None, "ratio", 0.5)
                StorageUtil.PluckIntValue(None, "oneshot", -1)
                StorageUtil.PluckFloatValue(None, "onefloat", -1.0)
                StorageUtil.PluckStringValue(None, "onestring", "missing")
                StorageUtil.PluckFormValue(None, "absent-form", None)
                StorageUtil.SetFormValue(None, "Owner", None)
                StorageUtil.IntListAdd(None, "Numbers", 7)
                StorageUtil.IntListAdd(None, "numbers", 7, false)
                StorageUtil.IntListSet(None, "numbers", 0, 8)
                StorageUtil.IntListAdd(None, "numbers", 9)
                StorageUtil.IntListRemoveAt(None, "numbers", 0)
                StorageUtil.IntListAdd(None, "numbers", 10)
                StorageUtil.IntListShift(None, "numbers")
                StorageUtil.IntListAdd(None, "numbers", 11)
                StorageUtil.IntListPop(None, "numbers")
                StorageUtil.IntListPluck(None, "numbers", -1, -99)
                StorageUtil.IntListInsert(None, "numbers", 0, 5)
                StorageUtil.IntListAdd(None, "numbers", 10)
                Int duplicates
                duplicates = StorageUtil.IntListCountValue(None, "numbers", 10)
                Int adjusted
                adjusted = StorageUtil.IntListAdjust(None, "numbers", 0, 1)
                StorageUtil.IntListRemove(None, "numbers", 10)
                StorageUtil.IntListRemove(None, "numbers", 10, true)
                StorageUtil.IntListAdd(None, "numbers", 2)
                StorageUtil.IntListAdd(None, "numbers", 1)
                StorageUtil.IntListSort(None, "numbers")
                Int grown
                grown = StorageUtil.IntListResize(None, "numbers", 5, 9)
                Int shrunk
                shrunk = StorageUtil.IntListResize(None, "numbers", 4)
                Int randomNumber
                randomNumber = StorageUtil.IntListRandom(None, "numbers")
                Int[] numberCopy
                numberCopy = StorageUtil.IntListToArray(None, "numbers")
                Bool copiedNumbers
                copiedNumbers = StorageUtil.IntListCopy(None, "numbers-copy", numberCopy)
                Int copiedNumberCount
                copiedNumberCount = StorageUtil.IntListCount(None, "numbers-copy")
                Int[] numberSlice = new Int[2]
                StorageUtil.IntListSlice(None, "numbers", numberSlice, 1)
                Bool slicedNumbers
                slicedNumbers = StorageUtil.IntListCopy(None, "numbers-slice", numberSlice)
                Int slicedNumberCount
                slicedNumberCount = StorageUtil.IntListCount(None, "numbers-slice")
                StorageUtil.IntListGet(None, "numbers", 0)
                StorageUtil.IntListCount(None, "numbers")
                StorageUtil.IntListFind(None, "numbers", 7)
                StorageUtil.IntListHas(None, "numbers", 7)
                StorageUtil.FloatListAdd(None, "Ratios", 2.5)
                StorageUtil.StringListAdd(None, "Labels", "ready")
                StorageUtil.StringListClear(None, "labels")
                StorageUtil.FormListAdd(None, "Owners", None)
                StorageUtil.SetIntValue(None, "Pack.Score", 1)
                StorageUtil.SetStringValue(None, "Pack.Note", "ready")
                StorageUtil.IntListAdd(None, "Pack.Items", 3)
                Int prefixCount
                prefixCount = StorageUtil.CountAllPrefix("PACK.")
                Int clearedInts
                clearedInts = StorageUtil.ClearIntValuePrefix("pack.")
                Int prefixRemaining
                prefixRemaining = StorageUtil.CountAllPrefix("pack.")
                Int clearedPrefix
                clearedPrefix = StorageUtil.ClearAllPrefix("pack.")
                Int prefixAfter
                prefixAfter = StorageUtil.CountAllPrefix("pack.")
                If value == 3 && visits == 2 && ratio == 1.75 && duplicates == 2 && adjusted == 6 && grown == 2 && shrunk < 0 && randomNumber >= 1 && randomNumber <= 9 && copiedNumbers && copiedNumberCount == 4 && slicedNumbers && slicedNumberCount == 2 && prefixCount == 3 && clearedInts == 1 && prefixRemaining == 2 && clearedPrefix == 2 && prefixAfter == 0
                    StorageUtil.SetStringValue(None, "Status", "ready")
                EndIf
            EndEvent
        "#;
    let (script, errors) = byroredux_papyrus::parse_script(source).unwrap();
    assert!(errors.is_empty(), "{errors:?}");
    let catalog = world
        .resource::<byroredux_scripting::PapyrusProviderRuntime>()
        .catalog();
    let program = byroredux_scripting::lower_provider_program(&script, &catalog)
        .unwrap()
        .unwrap();
    let entity = world.spawn();
    byroredux_scripting::attach_owned_papyrus_provider_program(
        &mut world,
        entity,
        program,
        principal.clone(),
    );
    world.insert(entity, byroredux_scripting::OnCellLoadEvent);

    byroredux_scripting::papyrus_provider_system(&world, 0.0);
    byroredux_scripting::event_cleanup_system(&world, 0.0);
    byroredux_scripting::papyrus_provider_system(&world, 0.0);

    let host = live_host.lock().unwrap();
    let values = host.principal_storage.values(&principal).unwrap();
    assert_eq!(
        values.get(&StorageKey::new("storageutil.int:count").unwrap()),
        Some(&PrincipalStorageValue::I64(3))
    );
    assert_eq!(
        values.get(&StorageKey::new("storageutil.string:status").unwrap()),
        Some(&PrincipalStorageValue::String("ready".to_owned()))
    );
    assert_eq!(
        values.get(&StorageKey::new("storageutil.int:visits").unwrap()),
        Some(&PrincipalStorageValue::I64(2))
    );
    assert_eq!(
        values.get(&StorageKey::new("storageutil.float:ratio").unwrap()),
        Some(&PrincipalStorageValue::Bytes(
            1.75_f32.to_bits().to_le_bytes().to_vec()
        ))
    );
    assert!(!values.contains_key(&StorageKey::new("storageutil.form:owner").unwrap()));
    assert!(!values.contains_key(&StorageKey::new("storageutil.int:oneshot").unwrap()));
    assert!(!values.contains_key(&StorageKey::new("storageutil.float:onefloat").unwrap()));
    assert!(!values.contains_key(&StorageKey::new("storageutil.string:onestring").unwrap()));
    assert_eq!(
        values.get(&StorageKey::new("storageutil.list.int:numbers").unwrap()),
        Some(&PrincipalStorageValue::Array(vec![
            ExtensionValue::I64(1),
            ExtensionValue::I64(2),
            ExtensionValue::I64(6),
            ExtensionValue::I64(9),
        ]))
    );
    assert_eq!(
        values.get(&StorageKey::new("storageutil.list.int:numbers-copy").unwrap()),
        Some(&PrincipalStorageValue::Array(vec![
            ExtensionValue::I64(1),
            ExtensionValue::I64(2),
            ExtensionValue::I64(6),
            ExtensionValue::I64(9),
        ]))
    );
    assert_eq!(
        values.get(&StorageKey::new("storageutil.list.int:numbers-slice").unwrap()),
        Some(&PrincipalStorageValue::Array(vec![
            ExtensionValue::I64(2),
            ExtensionValue::I64(6),
        ]))
    );
    assert_eq!(
        values.get(&StorageKey::new("storageutil.list.float:ratios").unwrap()),
        Some(&PrincipalStorageValue::Array(vec![ExtensionValue::Bytes(
            2.5_f32.to_bits().to_le_bytes().to_vec()
        )]))
    );
    assert_eq!(
        values.get(&StorageKey::new("storageutil.list.form:owners").unwrap()),
        Some(&PrincipalStorageValue::Array(vec![ExtensionValue::Bytes(
            Vec::new()
        )]))
    );
    assert!(!values.contains_key(&StorageKey::new("storageutil.list.string:labels").unwrap()));
}

#[test]
fn source_papyrus_runs_principal_private_jcontainers_across_wait() {
    let principal = PrincipalId::new("legacy.scripts.jcontainers-source").unwrap();
    let mut extension_host =
        ExtensionHost::new(SandboxConfig::default(), ComponentStoreLimits::default()).unwrap();
    extension_host
        .register_legacy_script_principal(principal.clone())
        .unwrap();
    let slot = ExtensionHostSlot::from_host(extension_host);
    let live_host = slot.host().unwrap();
    let mut world = World::new();
    byroredux_scripting::register(&mut world);
    world.insert_resource(slot);
    sync_extension_script_function_invoker(&world);

    let source = r#"
            ScriptName ContainerFixture
            Event OnLoad()
                Int values
                values = JArray.object()
                values = JValue.retain(values, "fixture-owner")
                JArray.addInt(values, 10)
                JArray.addInt(values, 20)
                JArray.addInt(values, 15, -2)
                Int tail
                tail = JArray.getInt(values, -1, -1)
                Int container
                container = JMap.object()
                JMap.setInt(container, "tail", tail)
                Utility.Wait(0.0)
                Int restored
                restored = JMap.getInt(container, "tail", -1)
                If restored == 20
                    JMap.setStr(container, "status", "ready")
                EndIf
                Int copied
                copied = JValue.deepCopy(container)
                Bool copiedIsMap
                copiedIsMap = JValue.isMap(copied)
                If copiedIsMap
                    JMap.setStr(copied, "copy", "ready")
                EndIf
            EndEvent
        "#;
    let (script, errors) = byroredux_papyrus::parse_script(source).unwrap();
    assert!(errors.is_empty(), "{errors:?}");
    let catalog = world
        .resource::<byroredux_scripting::PapyrusProviderRuntime>()
        .catalog();
    let program = byroredux_scripting::lower_provider_program(&script, &catalog)
        .unwrap()
        .unwrap();
    let entity = world.spawn();
    byroredux_scripting::attach_owned_papyrus_provider_program(
        &mut world,
        entity,
        program,
        principal.clone(),
    );
    world.insert(entity, byroredux_scripting::OnCellLoadEvent);

    byroredux_scripting::papyrus_provider_system(&world, 0.0);
    byroredux_scripting::event_cleanup_system(&world, 0.0);
    byroredux_scripting::papyrus_provider_system(&world, 0.0);

    let host = live_host.lock().unwrap();
    let registry = host.legacy_containers.get(&principal).unwrap();
    assert_eq!(registry.count(1), 3);
    assert_eq!(registry.retain_count(1), 1);
    assert_eq!(registry.retention_tag(1), Some("fixture-owner"));
    assert_eq!(
        registry.map_get(2, "status"),
        Some(&LegacyContainerValue::String("ready".to_owned()))
    );
    assert_eq!(
        registry.map_get(3, "copy"),
        Some(&LegacyContainerValue::String("ready".to_owned()))
    );
}

#[test]
fn source_papyrus_sends_typed_mod_event_across_wait() {
    let principal = PrincipalId::new("legacy.scripts.mod-event-source").unwrap();
    let mut extension_host =
        ExtensionHost::new(SandboxConfig::default(), ComponentStoreLimits::default()).unwrap();
    extension_host
        .register_legacy_script_principal(principal.clone())
        .unwrap();
    let slot = ExtensionHostSlot::from_host(extension_host);
    let live_host = slot.host().unwrap();
    let mut world = World::new();
    byroredux_scripting::register(&mut world);
    world.insert_resource(slot);
    sync_extension_script_function_invoker(&world);

    let source = r#"
            ScriptName ModEventFixture
            Event OnLoad()
                Int handle
                handle = ModEvent.Create("ByroReady")
                ModEvent.PushString(handle, "ready")
                ModEvent.PushInt(handle, 7)
                Utility.Wait(0.0)
                ModEvent.Send(handle)
            EndEvent
        "#;
    let (script, errors) = byroredux_papyrus::parse_script(source).unwrap();
    assert!(errors.is_empty(), "{errors:?}");
    let catalog = world
        .resource::<byroredux_scripting::PapyrusProviderRuntime>()
        .catalog();
    let program = byroredux_scripting::lower_provider_program(&script, &catalog)
        .unwrap()
        .unwrap();
    let entity = world.spawn();
    byroredux_scripting::attach_owned_papyrus_provider_program(
        &mut world,
        entity,
        program,
        principal.clone(),
    );
    world.insert(entity, byroredux_scripting::OnCellLoadEvent);

    byroredux_scripting::papyrus_provider_system(&world, 0.0);
    byroredux_scripting::event_cleanup_system(&world, 0.0);
    byroredux_scripting::papyrus_provider_system(&world, 0.0);

    let host = live_host.lock().unwrap();
    assert_eq!(host.pending_custom_events.len(), 1);
    let event = &host.pending_custom_events[0];
    assert_eq!(event.sender, principal);
    assert_eq!(
        byroredux_sdk::event::legacy_skse_mod_event_name(&event.event).as_deref(),
        Some("ByroReady")
    );
    assert_eq!(
        byroredux_sdk::event::LegacySkseVariadicModEventPayload::decode(&event.payload)
            .unwrap()
            .arguments,
        [
            LegacySkseModEventValue::String("ready".to_owned()),
            LegacySkseModEventValue::Int(7),
        ]
    );
}

#[test]
fn alias_send_mod_event_uses_owning_quest_form_on_shared_bus() {
    let principal = PrincipalId::new("legacy.scripts.alias-source").unwrap();
    let mut extension_host =
        ExtensionHost::new(SandboxConfig::default(), ComponentStoreLimits::default()).unwrap();
    extension_host
        .register_legacy_script_principal(principal.clone())
        .unwrap();
    let slot = ExtensionHostSlot::from_host(extension_host);
    let live_host = slot.host().unwrap();
    let mut world = World::new();
    byroredux_scripting::register(&mut world);
    let mut form_ids = FormIdPool::new();
    let quest_form = form_ids.intern(FormIdPair {
        plugin: byroredux_core::form_id::PluginId::from_filename("Fixture.esm"),
        local: byroredux_core::form_id::LocalFormId(0x1234),
    });
    world.insert_resource(form_ids);
    world.insert_resource(slot);
    sync_extension_script_function_invoker(&world);
    let expected_sender = FormRef::new([4; 16], 0x1234);
    byroredux_scripting::set_papyrus_provider_form_resolver(
        &world,
        Some(Arc::new(move |_form_id| Ok(expected_sender))),
    );

    let source = r#"
            ScriptName AliasFixture extends Alias
            Event OnLoad()
                Utility.Wait(0.0)
                SendModEvent("AliasReady", "ready", 3.5)
            EndEvent
        "#;
    let (script, errors) = byroredux_papyrus::parse_script(source).unwrap();
    assert!(errors.is_empty(), "{errors:?}");
    let catalog = world
        .resource::<byroredux_scripting::PapyrusProviderRuntime>()
        .catalog();
    let program = byroredux_scripting::lower_provider_program(&script, &catalog)
        .unwrap()
        .unwrap();
    // Alias programs are attached to the owning quest entity; this is the
    // same owner Form SKSE places in the callback's sender slot.
    let quest = world.spawn();
    world.insert(quest, FormIdComponent(quest_form));
    byroredux_scripting::attach_owned_papyrus_provider_program(
        &mut world,
        quest,
        program,
        principal.clone(),
    );
    world.insert(quest, byroredux_scripting::OnCellLoadEvent);

    byroredux_scripting::papyrus_provider_system(&world, 0.0);
    byroredux_scripting::event_cleanup_system(&world, 0.0);
    byroredux_scripting::papyrus_provider_system(&world, 0.0);

    let host = live_host.lock().unwrap();
    assert_eq!(host.pending_custom_events.len(), 1);
    let event = &host.pending_custom_events[0];
    assert_eq!(event.sender, principal);
    let payload = byroredux_sdk::event::LegacySkseModEventPayload::decode(&event.payload).unwrap();
    assert_eq!(payload.string_arg, "ready");
    assert_eq!(payload.number_arg(), 3.5);
    assert_eq!(payload.sender, Some(expected_sender));
}

#[test]
fn mod_event_bus_delivers_across_legacy_script_principals() {
    let sender = PrincipalId::new("legacy.scripts.mod-event-sender").unwrap();
    let receiver = PrincipalId::new("legacy.scripts.mod-event-receiver").unwrap();
    let mut extension_host =
        ExtensionHost::new(SandboxConfig::default(), ComponentStoreLimits::default()).unwrap();
    extension_host
        .register_legacy_script_principal(sender.clone())
        .unwrap();
    extension_host
        .register_legacy_script_principal(receiver.clone())
        .unwrap();
    let slot = ExtensionHostSlot::from_host(extension_host);
    let live_host = slot.host().unwrap();
    let mut world = World::new();
    byroredux_scripting::register(&mut world);
    world.insert_resource(slot);
    sync_extension_script_function_invoker(&world);

    let source = r#"
            ScriptName ModEventReceiver
            Event OnLoad()
                RegisterForModEvent("ByroReady", "OnByroReady")
            EndEvent
            Event OnByroReady(String status, Int count)
                StorageUtil.SetStringValue(None, "event-status", status)
                StorageUtil.SetIntValue(None, "event-count", count)
            EndEvent
        "#;
    let (script, errors) = byroredux_papyrus::parse_script(source).unwrap();
    assert!(errors.is_empty(), "{errors:?}");
    let catalog = world
        .resource::<byroredux_scripting::PapyrusProviderRuntime>()
        .catalog();
    let program = byroredux_scripting::lower_provider_program(&script, &catalog)
        .unwrap()
        .unwrap();
    let entity = world.spawn();
    byroredux_scripting::attach_owned_papyrus_provider_program(
        &mut world,
        entity,
        program,
        receiver.clone(),
    );
    world.insert(entity, byroredux_scripting::OnCellLoadEvent);
    byroredux_scripting::papyrus_provider_system(&world, 0.0);
    byroredux_scripting::event_cleanup_system(&world, 0.0);

    let route = |name: &str| format!("{PAPYRUS_MOD_EVENT_ROUTE_PREFIX}mod-event-{name}");
    {
        let mut host = live_host.lock().unwrap();
        let handle = host
            .invoke_owned_papyrus_provider(
                Some(&sender),
                &route("create"),
                &[ScriptValue::String("ByroReady".to_owned())],
            )
            .unwrap();
        for (operation, value) in [
            ("push-string", ScriptValue::String("ready".to_owned())),
            ("push-int", ScriptValue::Integer(7)),
        ] {
            host.invoke_owned_papyrus_provider(
                Some(&sender),
                &route(operation),
                &[handle.clone(), value],
            )
            .unwrap();
        }
        assert_eq!(
            host.invoke_owned_papyrus_provider(Some(&sender), &route("send"), &[handle],)
                .unwrap(),
            ScriptValue::Boolean(true)
        );
    }

    extension_custom_event_dispatch_system(&world, 0.0);
    byroredux_scripting::papyrus_provider_system(&world, 0.0);

    let host = live_host.lock().unwrap();
    let receiver_values = host.principal_storage.values(&receiver).unwrap();
    assert_eq!(
        receiver_values.get(&StorageKey::new("storageutil.string:event-status").unwrap()),
        Some(&PrincipalStorageValue::String("ready".to_owned()))
    );
    assert_eq!(
        receiver_values.get(&StorageKey::new("storageutil.int:event-count").unwrap()),
        Some(&PrincipalStorageValue::I64(7))
    );
    assert!(host
        .principal_storage
        .values(&sender)
        .is_none_or(BTreeMap::is_empty));
}

#[test]
fn game_content_aliases_run_without_an_extension_package() {
    let catalog = ContentCatalog::new_with_dependencies(
        vec![
            byroredux_sdk::content::PluginInfo::new(
                "Skyrim.esm",
                1_u128.to_be_bytes(),
                byroredux_sdk::content::PluginKind::Regular,
            )
            .unwrap(),
            byroredux_sdk::content::PluginInfo::new(
                "Update.esm",
                3_u128.to_be_bytes(),
                byroredux_sdk::content::PluginKind::Regular,
            )
            .unwrap(),
            byroredux_sdk::content::PluginInfo::new(
                "Patch.esl",
                2_u128.to_be_bytes(),
                byroredux_sdk::content::PluginKind::Light,
            )
            .unwrap(),
        ],
        vec![vec![], vec![0], vec![1]],
    )
    .unwrap();
    let mut host =
        ExtensionHost::new(SandboxConfig::default(), ComponentStoreLimits::default()).unwrap();
    host.set_content_catalog(Arc::new(catalog));

    assert!(host
        .papyrus_provider_catalog()
        .resolve("game", "getmodbyname")
        .is_some());
    assert_eq!(
        host.invoke_papyrus_provider(
            PAPYRUS_GAME_GET_MOD_BY_NAME_ROUTE,
            &[ScriptValue::String("UPDATE.ESM".to_owned())],
        )
        .unwrap(),
        ScriptValue::Integer(1)
    );
    assert_eq!(
        host.invoke_papyrus_provider(
            PAPYRUS_GAME_GET_MOD_BY_NAME_ROUTE,
            &[ScriptValue::String("Patch.esl".to_owned())],
        )
        .unwrap(),
        ScriptValue::Integer(0x100)
    );
    assert_eq!(
        host.invoke_papyrus_provider(
            PAPYRUS_GAME_GET_FORM_FROM_FILE_ROUTE,
            &[
                ScriptValue::Integer(0x1234),
                ScriptValue::String("update.esm".to_owned()),
            ],
        )
        .unwrap(),
        ScriptValue::Form(FormRef::new(3_u128.to_be_bytes(), 0x1234))
    );
    assert_eq!(
        host.invoke_papyrus_provider(
            PAPYRUS_GAME_GET_FORM_FROM_FILE_ROUTE,
            &[
                ScriptValue::Integer(-1),
                ScriptValue::String("Update.esm".to_owned()),
            ],
        )
        .unwrap(),
        ScriptValue::None
    );
    assert_eq!(
        host.invoke_papyrus_provider(PAPYRUS_GAME_GET_MOD_COUNT_ROUTE, &[])
            .unwrap(),
        ScriptValue::Integer(2)
    );
    assert_eq!(
        host.invoke_papyrus_provider(
            PAPYRUS_GAME_GET_MOD_NAME_ROUTE,
            &[ScriptValue::Integer(0x100)],
        )
        .unwrap(),
        ScriptValue::String("Patch.esl".to_owned())
    );
    assert_eq!(
        host.invoke_papyrus_provider(
            PAPYRUS_GAME_IS_PLUGIN_INSTALLED_ROUTE,
            &[ScriptValue::String("patch.ESL".to_owned())],
        )
        .unwrap(),
        ScriptValue::Boolean(true)
    );
    assert_eq!(
        host.invoke_papyrus_provider(PAPYRUS_GAME_GET_LIGHT_MOD_COUNT_ROUTE, &[])
            .unwrap(),
        ScriptValue::Integer(1)
    );
    assert_eq!(
        host.invoke_papyrus_provider(
            PAPYRUS_GAME_GET_LIGHT_MOD_BY_NAME_ROUTE,
            &[ScriptValue::String("Patch.esl".to_owned())],
        )
        .unwrap(),
        ScriptValue::Integer(0)
    );
    assert_eq!(
        host.invoke_papyrus_provider(
            PAPYRUS_GAME_GET_LIGHT_MOD_NAME_ROUTE,
            &[ScriptValue::Integer(0)],
        )
        .unwrap(),
        ScriptValue::String("Patch.esl".to_owned())
    );
    assert_eq!(
        host.invoke_papyrus_provider(
            PAPYRUS_GAME_GET_MOD_DEPENDENCY_COUNT_ROUTE,
            &[ScriptValue::Integer(0x100)],
        )
        .unwrap(),
        ScriptValue::Integer(1)
    );
    assert_eq!(
        host.invoke_papyrus_provider(
            PAPYRUS_GAME_GET_LIGHT_MOD_DEPENDENCY_COUNT_ROUTE,
            &[ScriptValue::Integer(0)],
        )
        .unwrap(),
        ScriptValue::Integer(1)
    );
    assert_eq!(
        host.invoke_papyrus_provider(
            PAPYRUS_GAME_GET_NTH_LIGHT_MOD_DEPENDENCY_ROUTE,
            &[ScriptValue::Integer(0), ScriptValue::Integer(0)],
        )
        .unwrap(),
        ScriptValue::Integer(1)
    );
}

#[test]
fn optional_denied_console_capability_publishes_no_engine_command() {
    let mut manifest = console_manifest("org.example.denied-console");
    manifest
        .capabilities
        .iter_mut()
        .find(|request| request.id.as_str() == CONSOLE_REGISTER_CAPABILITY)
        .unwrap()
        .required = false;
    let mut artifacts = ExtensionArtifacts::new();
    artifacts.insert(
        ComponentId::new("runtime").unwrap(),
        wat::parse_str(STORAGE_COMPONENT).unwrap(),
    );
    let mut extension_host =
        ExtensionHost::new(SandboxConfig::default(), ComponentStoreLimits::default()).unwrap();
    extension_host
        .install_package(&manifest, &artifacts, storage_grants())
        .unwrap();
    let slot = ExtensionHostSlot::from_host(extension_host);
    let mut world = World::new();
    world.insert_resource(slot);
    let mut registry = CommandRegistry::new();
    register_console_commands(&world, &mut registry);
    assert!(registry.list().is_empty());
}

#[test]
fn scheduler_adapter_snapshots_events_before_entering_the_host() {
    let mut world = World::new();
    byroredux_scripting::register(&mut world);
    let subject_entity = world.spawn();
    let activator = world.spawn();
    world.insert(
        subject_entity,
        byroredux_scripting::ActivateEvent { activator },
    );
    let slot = ExtensionHostSlot::from_host(host_with_package("org.example.adapter"));
    let host = slot.host().unwrap();
    world.insert_resource(slot);

    extension_activation_dispatch_system(&world, 0.0);

    let host = host.lock().unwrap();
    let subject = host.handles.by_entity[&subject_entity];
    let owner = PrincipalId::new("org.example.adapter").unwrap();
    let schema = ComponentSchemaId::new("example.activation-count").unwrap();
    assert_eq!(
        host.state()
            .row(&owner, &schema, subject)
            .and_then(|row| row.get("count")),
        Some(&ExtensionValue::I64(1))
    );
    assert!(world.has::<byroredux_scripting::ActivateEvent>(subject_entity));
}

#[test]
fn cell_load_adapter_delivers_before_shared_event_cleanup() {
    let mut world = World::new();
    byroredux_scripting::register(&mut world);
    let subject_entity = world.spawn();
    world.insert(subject_entity, byroredux_scripting::OnCellLoadEvent);
    let slot = ExtensionHostSlot::from_host(host_with_cell_load_package("org.example.cell-load"));
    let host = slot.host().unwrap();
    world.insert_resource(slot);

    extension_cell_load_dispatch_system(&world, 0.0);

    let host = host.lock().unwrap();
    let subject = host.handles.by_entity[&subject_entity];
    let owner = PrincipalId::new("org.example.cell-load").unwrap();
    let schema = ComponentSchemaId::new("example.activation-count").unwrap();
    assert_eq!(
        host.state()
            .row(&owner, &schema, subject)
            .and_then(|row| row.get("count")),
        Some(&ExtensionValue::I64(1))
    );
    assert!(world.has::<byroredux_scripting::OnCellLoadEvent>(subject_entity));
}

#[test]
fn hit_adapter_delivers_live_combat_marker_before_cleanup() {
    let mut world = World::new();
    byroredux_scripting::register(&mut world);
    let subject = world.spawn();
    let aggressor = world.spawn();
    world.insert(
        subject,
        byroredux_scripting::HitEvent {
            aggressor,
            source: aggressor,
            projectile: 0,
            damage: 12.5,
            power_attack: true,
            sneak_attack: false,
            bash_attack: true,
            blocked: false,
        },
    );
    let slot = ExtensionHostSlot::from_host(host_with_hit_package("org.example.hit-adapter"));
    let host = slot.host().unwrap();
    world.insert_resource(slot);

    extension_hit_dispatch_system(&world, 0.0);

    let host = host.lock().unwrap();
    let subject_handle = host.handles.by_entity[&subject];
    let owner = PrincipalId::new("org.example.hit-adapter").unwrap();
    let schema = ComponentSchemaId::new("example.activation-count").unwrap();
    assert_eq!(
        host.state()
            .row(&owner, &schema, subject_handle)
            .and_then(|row| row.get("count")),
        Some(&ExtensionValue::I64(1))
    );
    assert!(world.has::<byroredux_scripting::HitEvent>(subject));
}

#[test]
fn equipment_adapter_resolves_items_and_preserves_batch_order_before_cleanup() {
    use crate::cell_loader::load_order::{GlobalFormIdResolver, LoadOrder};

    let mut world = World::new();
    byroredux_scripting::register(&mut world);
    let wearer = world.spawn();
    world.insert(
        wearer,
        byroredux_scripting::EquipmentEventBatch(vec![
            byroredux_scripting::EquipmentChange {
                item_form_id: 0x0112_3456,
                equipped: false,
            },
            byroredux_scripting::EquipmentChange {
                item_form_id: 0xFE00_5ABC,
                equipped: true,
            },
        ]),
    );
    let order = LoadOrder::new(
        vec!["Base.esm".into(), "Gear.esp".into(), "Creation.esl".into()],
        vec![
            byroredux_plugin::esm::reader::GlobalSlot::Regular(0),
            byroredux_plugin::esm::reader::GlobalSlot::Regular(1),
            byroredux_plugin::esm::reader::GlobalSlot::Light(5),
        ],
    );
    world.insert_resource(GlobalFormIdResolver::from_load_order(&order));
    let slot =
        ExtensionHostSlot::from_host(host_with_equipment_package("org.example.equipment-adapter"));
    let host = slot.host().unwrap();
    world.insert_resource(slot);

    extension_equipment_dispatch_system(&world, 0.0);

    let host = host.lock().unwrap();
    let wearer_handle = host.handles.by_entity[&wearer];
    let owner = PrincipalId::new("org.example.equipment-adapter").unwrap();
    let schema = ComponentSchemaId::new("example.activation-count").unwrap();
    assert_eq!(
        host.state()
            .row(&owner, &schema, wearer_handle)
            .and_then(|row| row.get("count")),
        Some(&ExtensionValue::I64(2))
    );
    assert!(world.has::<byroredux_scripting::EquipmentEventBatch>(wearer));
}

#[test]
fn input_adapter_observes_rebound_action_press_and_release_edges() {
    use winit::keyboard::KeyCode;

    let principal = PrincipalId::new("org.example.input-adapter").unwrap();
    let key = StorageKey::new("activation-count").unwrap();
    let mut input = crate::components::InputState::default();
    input.keys_held.insert(KeyCode::KeyQ);
    let mut bindings = crate::interaction::ActionBindings::default();
    bindings.bind_key(KeyCode::KeyQ, crate::interaction::InputAction::Activate);
    let mut world = World::new();
    world.insert_resource(input);
    world.insert_resource(bindings);
    world.insert_resource(crate::interaction::ActionState::default());
    let slot = ExtensionHostSlot::from_host(host_with_input_package(principal.as_str()));
    let host = slot.host().unwrap();
    world.insert_resource(slot);

    crate::interaction::refresh_action_state(&world);
    extension_input_dispatch_system(&world, 0.0);
    world
        .resource_mut::<crate::components::InputState>()
        .keys_held
        .clear();
    crate::interaction::refresh_action_state(&world);
    extension_input_dispatch_system(&world, 0.0);

    assert_eq!(
        host.lock()
            .unwrap()
            .principal_storage
            .values(&principal)
            .and_then(|values| values.get(&key)),
        Some(&PrincipalStorageValue::I64(2))
    );
}

#[test]
fn session_adapter_drains_bounded_committed_events_outside_the_queue_guard() {
    let principal = PrincipalId::new("org.example.session-adapter").unwrap();
    let key = StorageKey::new("activation-count").unwrap();
    let mut world = World::new();
    world.insert_resource(SessionEventQueue::default());
    let slot = ExtensionHostSlot::from_host(host_with_session_package(principal.as_str()));
    let host = slot.host().unwrap();
    world.insert_resource(slot);

    queue_session_event(
        &world,
        SessionEvent {
            phase: SessionPhase::SaveComplete,
            slot: Some(7),
        },
    )
    .unwrap();
    queue_session_event(
        &world,
        SessionEvent {
            phase: SessionPhase::LoadComplete,
            slot: Some(7),
        },
    )
    .unwrap();
    extension_session_dispatch_system(&world, 0.0);

    assert_eq!(world.resource::<SessionEventQueue>().events.len(), 0);
    assert_eq!(
        host.lock()
            .unwrap()
            .principal_storage
            .values(&principal)
            .and_then(|values| values.get(&key)),
        Some(&PrincipalStorageValue::I64(1))
    );
}

#[test]
fn session_event_queue_rejects_invalid_payloads_and_overflow() {
    let mut world = World::new();
    world.insert_resource(SessionEventQueue::default());
    assert_eq!(
        queue_session_event(
            &world,
            SessionEvent {
                phase: SessionPhase::NewGame,
                slot: Some(1),
            }
        ),
        Err("invalid session event payload")
    );
    for _ in 0..MAX_PENDING_SESSION_EVENTS {
        queue_session_event(
            &world,
            SessionEvent {
                phase: SessionPhase::NewGame,
                slot: None,
            },
        )
        .unwrap();
    }
    assert_eq!(
        queue_session_event(
            &world,
            SessionEvent {
                phase: SessionPhase::NewGame,
                slot: None,
            }
        ),
        Err("session event queue is full")
    );
}

#[test]
fn scheduler_adapter_exposes_bounded_name_and_world_transform_projections() {
    let mut world = World::new();
    byroredux_scripting::register(&mut world);
    world.register::<Name>();
    world.register::<GlobalTransform>();
    let subject = world.spawn();
    let activator = world.spawn();
    let mut strings = StringPool::new();
    let subject_name = strings.intern("subject door");
    world.insert_resource(strings);
    world.insert(subject, Name(subject_name));
    world.insert(
        subject,
        GlobalTransform::new(Vec3::new(1.0, 2.0, 3.0), Quat::IDENTITY, 2.0),
    );
    world.insert(subject, byroredux_scripting::ActivateEvent { activator });
    let slot = ExtensionHostSlot::from_host(host_with_projection_package(
        "org.example.projection-adapter",
    ));
    let host = slot.host().unwrap();
    world.insert_resource(slot);

    let raw = capture_entity_projections(&world, &BTreeSet::from([subject, activator]));
    assert_eq!(raw[&subject].name.as_deref(), Some("subject door"));
    assert_eq!(
        raw[&subject].world_transform.unwrap().translation(),
        [1.0, 2.0, 3.0]
    );
    assert_eq!(raw[&subject].world_transform.unwrap().scale(), 2.0);
    assert_eq!(raw[&activator], RawEntityProjection::default());

    extension_activation_dispatch_system(&world, 0.0);

    let host = host.lock().unwrap();
    assert_eq!(
        host.components[0].instance.status(),
        &InstanceStatus::Active
    );
    assert!(host.handles.by_entity.contains_key(&subject));
    assert!(host.handles.by_entity.contains_key(&activator));
}

#[test]
fn actor_value_projection_and_deferred_apply_use_portable_avif_identity_atomically() {
    let mut world = World::new();
    let actor = world.spawn();
    let mut values = ActorValues::new();
    values.set_base(0x333, 100.0);
    world.insert(actor, values);
    let order = crate::cell_loader::load_order::LoadOrder::new(
        vec!["Skyrim.esm".into()],
        vec![byroredux_plugin::esm::reader::GlobalSlot::Regular(0)],
    );
    world.insert_resource(
        crate::cell_loader::load_order::GlobalFormIdResolver::from_load_order(&order),
    );
    let actor_value = FormRef::new(
        byroredux_core::form_id::PluginId::from_filename("Skyrim.esm")
            .0
            .to_be_bytes(),
        0x333,
    );

    let projections = capture_entity_projections(&world, &BTreeSet::from([actor]));
    let captured = projections[&actor]
        .actor_values
        .as_ref()
        .unwrap()
        .iter()
        .find(|(form, _)| *form == actor_value)
        .unwrap()
        .1;
    assert_eq!(captured.current(), 100.0);

    let mut host =
        ExtensionHost::new(SandboxConfig::default(), ComponentStoreLimits::default()).unwrap();
    let handle = host.bind_entity(actor, None).unwrap();
    host.pending_actor_value_writes.push(
        ActorValueCommand::new(
            handle,
            actor_value,
            ActorValueOperation::ModifyPermanent,
            5.0,
        )
        .unwrap(),
    );
    apply_pending_actor_value_writes(&world, &mut host);
    assert_eq!(
        world
            .query::<ActorValues>()
            .unwrap()
            .get(actor)
            .unwrap()
            .current(0x333),
        105.0
    );

    host.pending_actor_value_writes.push(
        ActorValueCommand::new(handle, actor_value, ActorValueOperation::SetBase, 200.0).unwrap(),
    );
    host.pending_actor_value_writes.push(
        ActorValueCommand::new(
            handle,
            FormRef::new([99; 16], 1),
            ActorValueOperation::SetBase,
            1.0,
        )
        .unwrap(),
    );
    apply_pending_actor_value_writes(&world, &mut host);
    assert_eq!(
        world
            .query::<ActorValues>()
            .unwrap()
            .get(actor)
            .unwrap()
            .current(0x333),
        105.0,
        "an unresolved command rejects the whole actor-value batch"
    );
    assert!(host.take_diagnostics().iter().any(|diagnostic| matches!(
        diagnostic,
        ExtensionDiagnostic::Fault { message, .. }
            if message.contains("deferred actor-value batch rejected")
    )));
}

#[test]
fn inventory_projection_aggregates_portable_forms_and_equipment_slots() {
    let mut world = World::new();
    let actor = world.spawn();
    let mut inventory = Inventory::new();
    let first = inventory.push(byroredux_core::ecs::components::ItemStack::new(0x1234, 2));
    inventory.push(byroredux_core::ecs::components::ItemStack::new(0x1234, 5));
    let weapon = inventory.push(byroredux_core::ecs::components::ItemStack::new(0x5678, 1));
    inventory.push(byroredux_core::ecs::components::ItemStack::new(
        0x0100_0001,
        4,
    ));
    world.insert(actor, inventory);
    let mut equipment = EquipmentSlots::new();
    equipment.equip(0b101, first);
    equipment.equip_weapon(weapon);
    world.insert(actor, equipment);
    let order = crate::cell_loader::load_order::LoadOrder::new(
        vec!["Skyrim.esm".into()],
        vec![byroredux_plugin::esm::reader::GlobalSlot::Regular(0)],
    );
    world.insert_resource(
        crate::cell_loader::load_order::GlobalFormIdResolver::from_load_order(&order),
    );

    let projections = capture_entity_projections(&world, &BTreeSet::from([actor]));
    let snapshot = projections[&actor].inventory.as_ref().unwrap();
    assert!(
        snapshot.truncated(),
        "unresolved forms are reported, not hidden"
    );
    assert_eq!(snapshot.entries().len(), 2);
    let armor = snapshot
        .entries()
        .iter()
        .find(|entry| entry.item().local() == 0x1234)
        .unwrap();
    assert_eq!(armor.count(), 7);
    assert_eq!(armor.biped_slots(), 0b101);
    assert!(!armor.weapon_equipped());
    let weapon = snapshot
        .entries()
        .iter()
        .find(|entry| entry.item().local() == 0x5678)
        .unwrap();
    assert_eq!(weapon.count(), 1);
    assert!(weapon.weapon_equipped());
}

#[test]
fn faction_projection_preserves_first_rank_and_reports_unresolved_forms() {
    let mut world = World::new();
    let actor = world.spawn();
    world.insert(
        actor,
        FactionRanks::from_pairs([(0x44, -1), (0x44, 3), (0x0100_0045, 2)]),
    );
    let order = crate::cell_loader::load_order::LoadOrder::new(
        vec!["Skyrim.esm".into()],
        vec![byroredux_plugin::esm::reader::GlobalSlot::Regular(0)],
    );
    world.insert_resource(
        crate::cell_loader::load_order::GlobalFormIdResolver::from_load_order(&order),
    );

    let projections = capture_entity_projections(&world, &BTreeSet::from([actor]));
    let snapshot = projections[&actor].factions.as_ref().unwrap();
    assert!(snapshot.truncated());
    assert_eq!(snapshot.memberships().len(), 1);
    let faction = FormRef::new(PluginId::from_filename("Skyrim.esm").0.to_be_bytes(), 0x44);
    assert_eq!(snapshot.rank(faction), Some(-1));
}

#[test]
fn perk_projection_preserves_first_rank_and_reports_invalid_entries() {
    let mut world = World::new();
    let actor = world.spawn();
    world.insert(
        actor,
        Perks {
            entries: vec![
                byroredux_core::character::PerkRank {
                    perk_form_id: 0x44,
                    rank: 1,
                },
                byroredux_core::character::PerkRank {
                    perk_form_id: 0x44,
                    rank: 3,
                },
                byroredux_core::character::PerkRank {
                    perk_form_id: 0x45,
                    rank: 0,
                },
                byroredux_core::character::PerkRank {
                    perk_form_id: 0x0100_0046,
                    rank: 2,
                },
            ],
        },
    );
    let order = crate::cell_loader::load_order::LoadOrder::new(
        vec!["Skyrim.esm".into()],
        vec![byroredux_plugin::esm::reader::GlobalSlot::Regular(0)],
    );
    world.insert_resource(
        crate::cell_loader::load_order::GlobalFormIdResolver::from_load_order(&order),
    );

    let projections = capture_entity_projections(&world, &BTreeSet::from([actor]));
    let snapshot = projections[&actor].perks.as_ref().unwrap();
    assert!(snapshot.truncated());
    assert_eq!(snapshot.entries().len(), 1);
    let perk = FormRef::new(PluginId::from_filename("Skyrim.esm").0.to_be_bytes(), 0x44);
    assert_eq!(snapshot.rank(perk), Some(1));
}

#[test]
fn package_projection_unifies_ambient_and_scene_state_and_defers_reevaluation() {
    let mut world = World::new();
    byroredux_scripting::register(&mut world);
    let actor = world.spawn();
    let scene_entity = world.spawn();
    world.insert(
        actor,
        AmbientPackageRuntime {
            package_candidates: vec![0x30, 0x31, 0x0100_0032],
            active_package_form_id: Some(0x31),
            actor_form_id: 0x20,
            last_evaluated_game_minute: Some(10),
        },
    );
    world.insert(
        scene_entity,
        byroredux_scripting::ScenePackagePlayback {
            active_actions: vec![byroredux_scripting::ActiveScenePackageAction {
                scene_form_id: 0x40,
                action_index: 2,
                actor,
                package_candidates: vec![0x50],
                package_form_id: 0x50,
                template_form_id: 0x51,
                command: byroredux_scripting::ScenePackageCommand::AwaitExternal {
                    procedure_type: "Test".to_owned(),
                },
            }],
        },
    );
    let order = crate::cell_loader::load_order::LoadOrder::new(
        vec!["Skyrim.esm".into()],
        vec![byroredux_plugin::esm::reader::GlobalSlot::Regular(0)],
    );
    world.insert_resource(
        crate::cell_loader::load_order::GlobalFormIdResolver::from_load_order(&order),
    );

    let projections = capture_entity_projections(&world, &BTreeSet::from([actor]));
    let snapshot = projections[&actor].packages.as_ref().unwrap();
    assert!(snapshot.truncated(), "the unresolved candidate is explicit");
    assert_eq!(snapshot.selections().len(), 2);
    assert_eq!(
        snapshot.selections()[0].source(),
        byroredux_sdk::packages::PackageSelectionSource::Ambient
    );
    assert_eq!(snapshot.selections()[0].candidates().len(), 2);
    assert_eq!(snapshot.selections()[0].active().unwrap().local(), 0x31);
    assert_eq!(
        snapshot.selections()[1].source(),
        byroredux_sdk::packages::PackageSelectionSource::Scene
    );
    assert_eq!(snapshot.selections()[1].action_index(), Some(2));
    assert_eq!(snapshot.selections()[1].scene().unwrap().local(), 0x40);
    assert_eq!(snapshot.selections()[1].template().unwrap().local(), 0x51);

    let mut host =
        ExtensionHost::new(SandboxConfig::default(), ComponentStoreLimits::default()).unwrap();
    let handle = host.bind_entity(actor, None).unwrap();
    host.pending_package_evaluations
        .push(EvaluatePackageCommand::new(handle));
    apply_pending_world_commands(&world, &mut host);
    assert!(world.has::<byroredux_scripting::EvaluatePackageRequest>(actor));
}

#[test]
fn animation_projection_and_command_share_the_authored_idle_runtime() {
    let mut world = World::new();
    byroredux_scripting::register(&mut world);
    let actor = world.spawn();
    world.insert(
        actor,
        byroredux_scripting::ActorCinematicState {
            requested_idle_form_id: Some(0x44),
            idle_request_serial: 3,
            awaited_event: Some(byroredux_scripting::CinematicAnimationEvent::ExitCartEnd),
            last_animation_event: Some(
                byroredux_scripting::CinematicAnimationEvent::IdleFurnitureExit,
            ),
            animation_event_serial: 5,
            ..Default::default()
        },
    );
    let order = crate::cell_loader::load_order::LoadOrder::new(
        vec!["Skyrim.esm".into()],
        vec![byroredux_plugin::esm::reader::GlobalSlot::Regular(0)],
    );
    world.insert_resource(
        crate::cell_loader::load_order::GlobalFormIdResolver::from_load_order(&order),
    );

    let projections = capture_entity_projections(&world, &BTreeSet::from([actor]));
    let snapshot = projections[&actor].animation.unwrap();
    assert_eq!(snapshot.requested_idle().unwrap().local(), 0x44);
    assert_eq!(snapshot.request_generation(), 3);
    assert_eq!(snapshot.awaited_event(), Some(AnimationEvent::ExitCartEnd));
    assert_eq!(
        snapshot.last_event(),
        Some(AnimationEvent::IdleFurnitureExit)
    );
    assert_eq!(snapshot.event_generation(), 5);

    let mut host =
        ExtensionHost::new(SandboxConfig::default(), ComponentStoreLimits::default()).unwrap();
    let handle = host.bind_entity(actor, None).unwrap();
    let replacement = FormRef::new(PluginId::from_filename("Skyrim.esm").0.to_be_bytes(), 0x55);
    host.pending_animation_commands
        .push(PlayIdleCommand::new(handle, replacement));
    apply_pending_world_commands(&world, &mut host);
    let state = world
        .get::<byroredux_scripting::ActorCinematicState>(actor)
        .unwrap();
    assert_eq!(state.requested_idle_form_id, Some(0x55));
    assert_eq!(state.idle_request_serial, 4);
}

#[test]
fn reputation_projection_and_writes_share_canonical_actor_state() {
    let mut world = World::new();
    world.register::<FactionReputation>();
    let actor = world.spawn();
    let mut reputation = FactionReputation::default();
    reputation.add_fame(0x44, 12);
    reputation.add_infamy(0x44, 4);
    world.insert(actor, reputation);
    let order = crate::cell_loader::load_order::LoadOrder::new(
        vec!["FalloutNV.esm".into()],
        vec![byroredux_plugin::esm::reader::GlobalSlot::Regular(0)],
    );
    world.insert_resource(
        crate::cell_loader::load_order::GlobalFormIdResolver::from_load_order(&order),
    );

    let projections = capture_entity_projections(&world, &BTreeSet::from([actor]));
    let snapshot = projections[&actor].reputation.as_ref().unwrap();
    assert!(!snapshot.truncated());
    let repu = FormRef::new(
        PluginId::from_filename("FalloutNV.esm").0.to_be_bytes(),
        0x44,
    );
    assert_eq!(snapshot.get(repu).unwrap().fame(), 12);
    assert_eq!(snapshot.get(repu).unwrap().infamy(), 4);

    let mut host =
        ExtensionHost::new(SandboxConfig::default(), ComponentStoreLimits::default()).unwrap();
    let handle = host.bind_entity(actor, None).unwrap();
    host.pending_reputation_writes
        .push(ReputationCommand::new(handle, repu, ReputationOperation::AddInfamy, 3).unwrap());
    apply_pending_world_commands(&world, &mut host);
    let state = world.get::<FactionReputation>(actor).unwrap();
    assert_eq!(state.fame(0x44), 12);
    assert_eq!(state.infamy(0x44), 7);
    drop(state);

    host.pending_reputation_writes
        .push(ReputationCommand::new(handle, repu, ReputationOperation::AddFame, 9).unwrap());
    host.pending_reputation_writes.push(
        ReputationCommand::new(
            handle,
            FormRef::new([99; 16], 1),
            ReputationOperation::AddInfamy,
            1,
        )
        .unwrap(),
    );
    apply_pending_world_commands(&world, &mut host);
    let state = world.get::<FactionReputation>(actor).unwrap();
    assert_eq!(state.fame(0x44), 12, "an unresolved REPU rejects the batch");
    assert_eq!(state.infamy(0x44), 7);
    assert!(host.take_diagnostics().iter().any(|diagnostic| matches!(
        diagnostic,
        ExtensionDiagnostic::Fault { message, .. }
            if message.contains("deferred reputation batch rejected")
    )));
}

#[test]
fn update_dispatch_flushes_world_commands_left_by_any_callback_phase() {
    let mut world = World::new();
    byroredux_scripting::register(&mut world);
    let actor = world.spawn();
    let mut host =
        ExtensionHost::new(SandboxConfig::default(), ComponentStoreLimits::default()).unwrap();
    let handle = host.bind_entity(actor, None).unwrap();
    host.pending_package_evaluations
        .push(EvaluatePackageCommand::new(handle));
    world.insert_resource(ExtensionHostSlot::from_host(host));

    extension_update_dispatch_system(&world, 0.016);

    assert!(world.has::<byroredux_scripting::EvaluatePackageRequest>(actor));
}

#[test]
fn spatial_snapshot_captures_portable_authored_references_and_queries_by_distance() {
    let mut world = World::new();
    let near_entity = world.spawn();
    let far_entity = world.spawn();
    let plugin = PluginId::from_filename("Skyrim.esm");
    let near_pair = FormIdPair {
        plugin,
        local: LocalFormId(1),
    };
    let far_pair = FormIdPair {
        plugin,
        local: LocalFormId(2),
    };
    let mut pool = FormIdPool::new();
    let near_form = pool.intern(near_pair);
    let far_form = pool.intern(far_pair);
    world.insert(near_entity, FormIdComponent(near_form));
    world.insert(far_entity, FormIdComponent(far_form));
    world.insert(
        near_entity,
        GlobalTransform::new(Vec3::new(2.0, 0.0, 0.0), Quat::IDENTITY, 1.0),
    );
    world.insert(
        far_entity,
        Transform::from_translation(Vec3::new(5.0, 0.0, 0.0)),
    );
    world.insert_resource(pool);

    let snapshot = capture_spatial_snapshot(&world);
    assert_eq!(snapshot.references().len(), 2);
    assert!(!snapshot.truncated());
    let result = snapshot.nearby([0.0; 3], 5.0, 1).unwrap();
    assert_eq!(result.hits().len(), 1);
    assert_eq!(result.hits()[0].reference().form(), form_ref(near_pair));
    assert_eq!(result.hits()[0].distance(), 2.0);
    assert!(result.truncated());
}

#[test]
fn spatial_snapshot_marks_duplicate_portable_forms_truncated() {
    let mut world = World::new();
    let first = world.spawn();
    let duplicate = world.spawn();
    let pair = FormIdPair {
        plugin: PluginId::from_filename("Skyrim.esm"),
        local: LocalFormId(1),
    };
    let mut pool = FormIdPool::new();
    let form = pool.intern(pair);
    world.insert(first, FormIdComponent(form));
    world.insert(duplicate, FormIdComponent(form));
    world.insert(first, GlobalTransform::new(Vec3::ZERO, Quat::IDENTITY, 1.0));
    world.insert(duplicate, Transform::from_translation(Vec3::X));
    world.insert_resource(pool);

    let snapshot = capture_spatial_snapshot(&world);
    assert_eq!(snapshot.references().len(), 1);
    assert!(snapshot.truncated());
}

#[test]
fn world_generation_invalidates_old_entity_handles() {
    let mut host = host_with_package("org.example.generation");
    host.dispatch_activations([RawActivation {
        subject: 7,
        subject_form: None,
        activator: None,
        activator_form: None,
    }]);
    let old = host.handles.by_entity[&7];
    let owner = PrincipalId::new("org.example.generation").unwrap();
    let schema = ComponentSchemaId::new("example.activation-count").unwrap();
    assert!(host.state().row(&owner, &schema, old).is_some());
    assert_eq!(host.handles.resolve(old), Some(7));
    assert_eq!(host.begin_world_generation().unwrap(), 2);
    assert_eq!(host.handles.resolve(old), None);
    assert!(host.state().row(&owner, &schema, old).is_none());
    let replacement = host.handles.handle_for(7).unwrap();
    assert_ne!(old, replacement);
    assert_eq!(replacement.world_generation(), 2);
}

#[test]
fn content_catalog_sync_publishes_the_live_load_order_to_the_host() {
    let order = crate::cell_loader::load_order::LoadOrder::new(
        vec!["Skyrim.esm".into(), "Creation.esl".into()],
        vec![
            byroredux_plugin::esm::reader::GlobalSlot::Regular(0),
            byroredux_plugin::esm::reader::GlobalSlot::Light(3),
        ],
    );
    let faction = |form_id, relations| byroredux_plugin::esm::records::FactionRecord {
        form_id,
        editor_id: format!("Faction{form_id:08X}"),
        full_name: String::new(),
        flags: 0,
        relations,
        ranks: Vec::new(),
        reputation: None,
    };
    let factions = std::collections::HashMap::from([
        (
            0x0000_0100,
            faction(
                0x0000_0100,
                vec![byroredux_plugin::esm::records::FactionRelation {
                    other_faction: 0x0000_0200,
                    modifier: 25,
                    combat_reaction: 3,
                }],
            ),
        ),
        (0x0000_0200, faction(0x0000_0200, Vec::new())),
    ]);
    let resolver = crate::cell_loader::load_order::GlobalFormIdResolver::from_load_order_with_records_and_factions(
            &order,
            &std::collections::HashMap::from([(0xFE00_3ABC, *b"STAT")]),
            &factions,
        );
    let slot = ExtensionHostSlot::initialize_default();
    let host = slot.host().unwrap();
    let mut world = World::new();
    byroredux_scripting::register(&mut world);
    world.insert_resource(resolver);
    world.insert_resource(slot);

    extension_content_catalog_sync_system(&world, 0.0);

    let host = host.lock().unwrap();
    assert_eq!(host.content_catalog.len(), 2);
    assert_eq!(host.content_catalog.plugin(0).unwrap().name(), "Skyrim.esm");
    assert_eq!(host.content_catalog.find("CREATION.ESL").unwrap().0, 1);
    let form = byroredux_sdk::identity::FormRef::new(
        byroredux_core::form_id::PluginId::from_filename("Creation.esl")
            .0
            .to_be_bytes(),
        0xabc,
    );
    assert_eq!(
        host.content_catalog.record(form).unwrap().record_type(),
        *b"STAT"
    );
    let source = FormRef::new(
        byroredux_core::form_id::PluginId::from_filename("Skyrim.esm")
            .0
            .to_be_bytes(),
        0x100,
    );
    let target = FormRef::new(
        byroredux_core::form_id::PluginId::from_filename("Skyrim.esm")
            .0
            .to_be_bytes(),
        0x200,
    );
    let relationship = host
        .faction_relationships
        .relationship(source, target)
        .unwrap();
    assert_eq!(relationship.modifier(), 25);
    assert_eq!(relationship.combat_reaction_raw(), 3);
    drop(host);
    let form_resolver = world
        .resource::<byroredux_scripting::PapyrusProviderRuntime>()
        .form_resolver()
        .expect("content sync publishes a form resolver");
    assert_eq!(form_resolver(0xFE00_3ABC).unwrap(), form);
}

#[test]
fn content_catalog_sync_feeds_builtin_obscript_without_extension_host() {
    let order = crate::cell_loader::load_order::LoadOrder::new(
        vec!["FalloutNV.esm".into(), "Companion.esp".into()],
        vec![
            byroredux_plugin::esm::reader::GlobalSlot::Regular(0),
            byroredux_plugin::esm::reader::GlobalSlot::Regular(1),
        ],
    );
    let mut world = World::new();
    byroredux_scripting::register(&mut world);
    world.insert_resource(
        crate::cell_loader::load_order::GlobalFormIdResolver::from_load_order(&order),
    );

    extension_content_catalog_sync_system(&world, 0.0);

    let catalog = world.resource::<byroredux_scripting::LegacyObscriptContentCatalog>();
    assert_eq!(catalog.0.len(), 2);
    assert_eq!(catalog.0.find("companion.ESP").unwrap().0, 1);
}

#[test]
fn engine_settings_sync_publishes_public_configuration_to_the_host() {
    let slot = ExtensionHostSlot::initialize_default();
    let host = slot.host().unwrap();
    let mut world = World::new();
    let mut settings = byroredux_core::settings::SettingsRegistry::default();
    settings
        .register(byroredux_core::settings::SettingEntry::slider(
            "gameplay.fov",
            "Gameplay",
            "FOV",
            "test",
            90.0,
            45.0,
            120.0,
            1.0,
            "degrees",
        ))
        .unwrap();
    settings
        .set(
            "gameplay.fov",
            byroredux_core::settings::SettingValue::Number(110.0),
        )
        .unwrap();
    world.insert_resource(settings);
    world.insert_resource(slot);

    extension_engine_settings_sync_system(&world, 0.0);

    let host = host.lock().unwrap();
    assert_eq!(
        host.engine_settings.get("gameplay.fov"),
        Some(&byroredux_sdk::settings::SettingValue::Number(110.0))
    );
}

#[test]
fn deferred_extension_setting_writes_commit_through_native_registry() {
    let slot = ExtensionHostSlot::initialize_default();
    let host = slot.host().unwrap();
    let mut settings = byroredux_core::settings::SettingsRegistry::default();
    settings
        .register(byroredux_core::settings::SettingEntry::toggle(
            "ext.org.example.settings.enabled",
            "org.example.settings",
            "Enabled",
            "test",
            false,
        ))
        .unwrap();
    host.lock().unwrap().pending_setting_writes.push(
        byroredux_sdk::settings::SettingWriteCommand {
            key: "ext.org.example.settings.enabled".to_owned(),
            value: SdkSettingValue::Boolean(true),
        },
    );
    let mut world = World::new();
    world.insert_resource(settings);
    world.insert_resource(slot);

    extension_setting_write_apply_system(&world, 0.0);

    assert_eq!(
        world
            .resource::<byroredux_core::settings::SettingsRegistry>()
            .get("ext.org.example.settings.enabled")
            .unwrap()
            .value,
        byroredux_core::settings::SettingValue::Bool(true)
    );
    let host = host.lock().unwrap();
    assert!(host.pending_setting_writes.is_empty());
    assert_eq!(
        host.engine_settings.get("ext.org.example.settings.enabled"),
        Some(&SdkSettingValue::Boolean(true))
    );
}

#[test]
fn orderly_shutdown_stops_active_components() {
    let mut host = host_with_package("org.example.shutdown");
    host.shutdown_all();

    assert_eq!(
        host.components[0].instance.status(),
        &InstanceStatus::Stopped
    );
    host.shutdown_all();
    assert_eq!(
        host.components[0].instance.status(),
        &InstanceStatus::Stopped
    );
}

#[test]
fn extension_rows_round_trip_through_stable_form_identity() {
    let slot = ExtensionHostSlot::from_host(host_with_package("org.example.save"));
    let host = slot.host().unwrap();
    let (source, old_entity) = world_with_form_and_host(slot.clone(), form_pair_for_test());
    host.lock().unwrap().dispatch_activations([RawActivation {
        subject: old_entity,
        subject_form: None,
        activator: None,
        activator_form: None,
    }]);
    let mut snapshot = empty_snapshot();
    assert_eq!(capture_extension_state(&source, &mut snapshot).unwrap(), 1);
    let encoded = byroredux_save::encode(&snapshot, 0xfeed_beef).unwrap();
    let snapshot = byroredux_save::decode(&encoded, 0xfeed_beef).unwrap();

    let (restored, new_entity) = world_with_form_and_host(slot, form_pair_for_test());
    assert_eq!(
        old_entity, new_entity,
        "fixture intentionally reuses the raw ECS id"
    );
    preflight_extension_state(&restored, &snapshot).unwrap();
    assert_eq!(restore_extension_state(&restored, &snapshot).unwrap(), 1);

    let host = host.lock().unwrap();
    let handle = host.handles.by_entity[&new_entity];
    assert_eq!(handle.world_generation(), 2);
    let owner = PrincipalId::new("org.example.save").unwrap();
    let schema = ComponentSchemaId::new("example.activation-count").unwrap();
    assert_eq!(
        host.state()
            .row(&owner, &schema, handle)
            .and_then(|row| row.get("count")),
        Some(&ExtensionValue::I64(1))
    );
}

#[test]
fn unavailable_extension_rows_are_preserved_verbatim() {
    let source_slot = ExtensionHostSlot::from_host(host_with_package("org.example.missing"));
    let source_host = source_slot.host().unwrap();
    let (source, entity) = world_with_form_and_host(source_slot, form_pair_for_test());
    source_host
        .lock()
        .unwrap()
        .dispatch_activations([RawActivation {
            subject: entity,
            subject_form: None,
            activator: None,
            activator_form: None,
        }]);
    let mut original = empty_snapshot();
    capture_extension_state(&source, &mut original).unwrap();

    let empty_slot = ExtensionHostSlot::initialize_default();
    let target_host = empty_slot.host().unwrap();
    let (target, _) = world_with_form_and_host(empty_slot, form_pair_for_test());
    preflight_extension_state(&target, &original).unwrap();
    assert_eq!(restore_extension_state(&target, &original).unwrap(), 0);
    assert_eq!(target_host.lock().unwrap().retained_rows.len(), 1);

    let mut resaved = empty_snapshot();
    capture_extension_state(&target, &mut resaved).unwrap();
    assert_eq!(
        original.resources[EXTENSION_STATE_RESOURCE],
        resaved.resources[EXTENSION_STATE_RESOURCE]
    );
}

#[test]
fn retained_form_row_rebinds_before_the_next_activation() {
    let source_slot = ExtensionHostSlot::from_host(host_with_package("org.example.streamed"));
    let source_host = source_slot.host().unwrap();
    let (source, entity) = world_with_form_and_host(source_slot, form_pair_for_test());
    source_host
        .lock()
        .unwrap()
        .dispatch_activations([RawActivation {
            subject: entity,
            subject_form: None,
            activator: None,
            activator_form: None,
        }]);
    let mut snapshot = empty_snapshot();
    capture_extension_state(&source, &mut snapshot).unwrap();

    let target_slot = ExtensionHostSlot::from_host(host_with_package("org.example.streamed"));
    let target_host = target_slot.host().unwrap();
    let mut target = World::new();
    target.insert_resource(FormIdPool::new());
    target.insert_resource(target_slot);
    preflight_extension_state(&target, &snapshot).unwrap();
    assert_eq!(restore_extension_state(&target, &snapshot).unwrap(), 0);

    let stable = form_ref(form_pair_for_test());
    let stats = target_host
        .lock()
        .unwrap()
        .dispatch_activations([RawActivation {
            subject: 77,
            subject_form: Some(stable),
            activator: None,
            activator_form: None,
        }]);
    assert_eq!(stats.commands_applied, 1);
    let host = target_host.lock().unwrap();
    assert!(host.retained_rows.is_empty());
    let handle = host.handles.by_entity[&77];
    let owner = PrincipalId::new("org.example.streamed").unwrap();
    let schema = ComponentSchemaId::new("example.activation-count").unwrap();
    assert_eq!(
        host.state()
            .row(&owner, &schema, handle)
            .and_then(|row| row.get("count")),
        Some(&ExtensionValue::I64(2))
    );
}

#[test]
fn schema_mismatch_is_rejected_before_world_replacement() {
    let source_slot = ExtensionHostSlot::from_host(host_with_package("org.example.schema"));
    let source_host = source_slot.host().unwrap();
    let (source, entity) = world_with_form_and_host(source_slot, form_pair_for_test());
    source_host
        .lock()
        .unwrap()
        .dispatch_activations([RawActivation {
            subject: entity,
            subject_form: None,
            activator: None,
            activator_form: None,
        }]);
    let mut snapshot = empty_snapshot();
    capture_extension_state(&source, &mut snapshot).unwrap();

    let mut changed = manifest("org.example.schema");
    changed.component_schemas[0].version = 2;
    let target_slot = ExtensionHostSlot::from_host(host_with_manifest(changed));
    let (target, _) = world_with_form_and_host(target_slot, form_pair_for_test());
    let error = preflight_extension_state(&target, &snapshot).unwrap_err();
    assert!(error.to_string().contains("saved state requires 1"));
}

#[test]
fn unsupported_extension_state_format_fails_preflight() {
    let slot = ExtensionHostSlot::initialize_default();
    let (world, _) = world_with_form_and_host(slot, form_pair_for_test());
    let mut snapshot = empty_snapshot();
    snapshot.resources.insert(
        EXTENSION_STATE_RESOURCE.to_owned(),
        serde_json::json!({ "format_version": 99, "rows": [] }),
    );
    let error = preflight_extension_state(&world, &snapshot).unwrap_err();
    assert!(error.to_string().contains("format 99 is unsupported"));
}

#[test]
fn transient_entity_rows_abort_instead_of_producing_a_lossy_save() {
    let slot = ExtensionHostSlot::from_host(host_with_package("org.example.transient"));
    let host = slot.host().unwrap();
    let mut world = World::new();
    let entity = world.spawn();
    world.insert_resource(slot);
    host.lock().unwrap().dispatch_activations([RawActivation {
        subject: entity,
        subject_form: None,
        activator: None,
        activator_form: None,
    }]);
    let error = capture_extension_state(&world, &mut empty_snapshot()).unwrap_err();
    assert!(error.to_string().contains("refusing a lossy save"));
}

#[test]
fn lossy_extension_snapshot_rejection_does_not_consume_quicksave_slot() {
    let directory = tempfile::tempdir().unwrap();
    let slot = ExtensionHostSlot::from_host(host_with_package("org.example.save-abort"));
    let host = slot.host().unwrap();
    let mut world = World::new();
    world.insert_resource(byroredux_core::string::StringPool::new());
    world.insert_resource(FormIdPool::new());
    world.insert_resource(crate::save_io::build_save_registry());
    world.insert_resource(crate::save_io::SaveState::new(
        directory.path().to_owned(),
        4,
    ));
    world.insert_resource(SessionEventQueue::default());
    let entity = world.spawn();
    world.insert_resource(slot);
    host.lock().unwrap().dispatch_activations([RawActivation {
        subject: entity,
        subject_form: None,
        activator: None,
        activator_form: None,
    }]);

    let output = crate::save_io::quicksave(&world);
    assert!(
        output.lines.join(" ").contains("refusing a lossy save"),
        "unexpected save output: {:?}",
        output.lines
    );
    assert_eq!(world.resource::<crate::save_io::SaveState>().ring.peek(), 0);
    assert!(byroredux_save::disk::list_slots(directory.path()).is_empty());
    assert!(pending_session_events(&world).is_empty());
}

#[test]
fn denied_capability_prevents_package_publication() {
    let mut host =
        ExtensionHost::new(SandboxConfig::default(), ComponentStoreLimits::default()).unwrap();
    let mut artifacts = ExtensionArtifacts::new();
    artifacts.insert(
        ComponentId::new("runtime").unwrap(),
        wat::parse_str(COMPONENT).unwrap(),
    );
    assert!(host
        .install_package(
            &manifest("org.example.denied"),
            &artifacts,
            CapabilitySet::new()
        )
        .is_err());
    assert_eq!(host.package_count(), 0);
    assert_eq!(host.component_count(), 0);
}

#[test]
fn one_trapping_package_does_not_block_an_unrelated_package() {
    let mut host = host_with_package("org.example.healthy");
    let trapping = COMPONENT.replacen(
        "      call $increment)",
        "      call $increment\n      unreachable)",
        1,
    );
    let mut artifacts = ExtensionArtifacts::new();
    artifacts.insert(
        ComponentId::new("runtime").unwrap(),
        wat::parse_str(&trapping).unwrap(),
    );
    host.install_package(&manifest("org.example.trapping"), &artifacts, grants())
        .unwrap();

    let stats = host.dispatch_activations([RawActivation {
        subject: 9,
        subject_form: None,
        activator: None,
        activator_form: None,
    }]);
    assert_eq!(stats.deliveries, 2);
    assert_eq!(stats.commands_applied, 1);
    assert_eq!(stats.faults, 1);
    let healthy = host
        .components
        .iter()
        .find(|component| component.extension.as_str() == "org.example.healthy")
        .unwrap();
    let trapping = host
        .components
        .iter()
        .find(|component| component.extension.as_str() == "org.example.trapping")
        .unwrap();
    assert_eq!(healthy.instance.status(), &InstanceStatus::Active);
    assert!(matches!(
        trapping.instance.status(),
        InstanceStatus::Quarantined(_)
    ));
}

#[test]
fn cli_package_set_requires_explicit_grants_and_commits_atomically() {
    let directory = tempfile::tempdir().unwrap();
    let manifest_path = directory.path().join("extension.toml");
    let component_path = directory.path().join("runtime.wasm");
    std::fs::write(&component_path, wat::parse_str(COMPONENT).unwrap()).unwrap();
    std::fs::write(
        &manifest_path,
        r#"
manifest_version = 1
id = "org.example.cli"
name = "CLI fixture"
version = "1.0.0"
sdk = "^0.1"

[[components]]
id = "runtime"
path = "runtime.wasm"
world = "byro.mod-host.extension"
world_version = "^0.1"

[[capabilities]]
id = "byro.events.subscribe"
required = true

[[capabilities]]
id = "byro.components.write-own"
required = true

[[capabilities]]
id = "byro.settings.register"
required = true

[[subscriptions]]
event = "byro.events.activate"

[[component_schemas]]
id = "example.activation-count"
version = 1

[[component_schemas.fields]]
id = "count"
value_type = "i64"

[[settings]]
id = "strength"
label = "Strength"
description = "Effect strength"
default = { Number = 1.0 }
control = { kind = "slider", min = 0.0, max = 2.0, step = 0.1, unit = "x" }
"#,
    )
    .unwrap();

    let mut world = World::new();
    world.insert_resource(ExtensionHostSlot::initialize_default());
    world.insert_resource(byroredux_core::settings::SettingsRegistry::default());
    let base_args = vec![
        "byroredux".to_owned(),
        "--extension".to_owned(),
        manifest_path.to_string_lossy().into_owned(),
    ];
    assert!(load_requested_extensions(&world, &base_args).is_err());
    {
        let slot = world.resource::<ExtensionHostSlot>();
        let host = slot.host().unwrap();
        assert_eq!(host.lock().unwrap().package_count(), 0);
    }
    assert!(!world
        .resource::<byroredux_core::settings::SettingsRegistry>()
        .contains("ext.org.example.cli.strength"));

    let mut granted_args = base_args;
    granted_args.extend([
        "--extension-grant".to_owned(),
        "org.example.cli=*".to_owned(),
    ]);
    assert_eq!(load_requested_extensions(&world, &granted_args).unwrap(), 1);
    let slot = world.resource::<ExtensionHostSlot>();
    let host = slot.host().unwrap();
    assert_eq!(host.lock().unwrap().package_count(), 1);
    let settings = world.resource::<byroredux_core::settings::SettingsRegistry>();
    let entry = settings.get("ext.org.example.cli.strength").unwrap();
    assert_eq!(
        entry.value,
        byroredux_core::settings::SettingValue::Number(1.0)
    );
    assert_eq!(entry.section, "org.example.cli");
}

#[test]
fn empty_extension_set_still_installs_the_engine_provider_host() {
    let mut world = World::new();
    world.insert_resource(ExtensionHostSlot::initialize_default());
    world.insert_resource(byroredux_core::settings::SettingsRegistry::default());
    byroredux_scripting::register(&mut world);

    assert_eq!(load_requested_extensions(&world, &[]).unwrap(), 0);
    assert!(world.resource::<ExtensionHostSlot>().host().is_some());
    assert!(world
        .resource::<byroredux_scripting::PapyrusProviderRuntime>()
        .callback()
        .is_some());
}
