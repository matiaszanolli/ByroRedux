//! Translated OBSE/xNVSE load-order handlers and manifest-published Papyrus provider calls.
//!
//! Split out of `late.rs` (#6703); the rows, their order and their comments are unchanged.

use byroredux_core::ecs::{Access, Scheduler, Stage};

/// Translated OBSE/xNVSE load-order handlers and manifest-published Papyrus provider calls.
pub(super) fn register_script_host_systems(scheduler: &mut Scheduler) {
    // Run translated OBSE/xNVSE load-order handlers after their shared live
    // catalog snapshot is published and before transient OnLoad/OnActivate
    // markers are drained. The program component is static translated data;
    // numeric results land in the existing save-backed ScriptVariables.
    scheduler.add_exclusive_with_access(
        Stage::Late,
        byroredux_scripting::legacy_obscript_load_order_system,
        Access::new()
            .reads_resource::<byroredux_scripting::LegacyObscriptContentCatalog>()
            // #3951 — the host-function invoker was missing. These
            // declarations do not affect scheduling today (the analyzer only
            // walks parallel-stage pairs, and exclusives run serially), so
            // this is not a deadlock fix; their stated purpose (#3473) is to
            // be the thing compared against if either system is ever
            // promoted to a parallel lane, and an under-declaration defeats
            // exactly that.
            .reads_resource::<byroredux_scripting::ExtensionScriptFunctionInvoker>()
            .reads::<byroredux_scripting::LegacyObscriptProgram>()
            .reads::<byroredux_scripting::OnCellLoadEvent>()
            .reads::<byroredux_scripting::ActivateEvent>()
            // #6218 — through `papyrus_activations`: a native-only activation is no OnActivate.
            .reads::<byroredux_scripting::NativeActivation>()
            .writes::<byroredux_scripting::ScriptVariables>(),
    );
    // Manifest-published `Provider.Function(...)` calls use the same live host
    // as ObScript, but their static Papyrus/PEX program is snapshotted before
    // guest entry so no ECS guard crosses the sandbox boundary.
    scheduler.add_exclusive_with_access(
        Stage::Late,
        byroredux_scripting::papyrus_provider_system,
        Access::new()
            .reads_resource::<byroredux_scripting::PapyrusProviderRuntime>()
            // #3951 — the continuation queue and the mod-event runtime are
            // `resource_mut`s, the form pool and six further event/identity
            // component storages are reads, and none of them were declared:
            // the body acquires thirteen types where this list named four.
            // See the sibling above for why that matters even though these
            // declarations do not currently affect scheduling.
            .writes_resource::<byroredux_scripting::PapyrusProviderContinuationQueue>()
            .writes_resource::<byroredux_scripting::PapyrusModEventRuntime>()
            // #6017 — the once-only `OnInit` flag, keyed by reference FormID.
            .writes_resource::<byroredux_scripting::QuestScriptVariables>()
            .reads_resource::<byroredux_core::form_id::FormIdPool>()
            .reads::<byroredux_scripting::PapyrusProviderProgram>()
            .reads::<byroredux_scripting::OnCellLoadEvent>()
            .reads::<byroredux_scripting::ActivateEvent>()
            .reads::<byroredux_scripting::OnInitEvent>()
            .reads::<byroredux_scripting::HitEvent>()
            .reads::<byroredux_scripting::EquipmentEventBatch>()
            .reads::<byroredux_scripting::OnTriggerEnterEvent>()
            .reads::<byroredux_scripting::OnUpdateEvent>()
            .reads::<byroredux_core::ecs::components::FormIdComponent>()
            .writes_resource::<crate::extensions::ExtensionHostSlot>(),
    );
    // The load-order handlers read the catalog snapshot the extension sync publishes.
    scheduler.run_after(
        byroredux_core::ecs::system_name(&byroredux_scripting::legacy_obscript_load_order_system),
        byroredux_core::ecs::system_name(&crate::extensions::extension_content_catalog_sync_system),
    );
}
