//! SDK <-> WIT value converters, plus their projection tests.

use super::*;

pub(crate) fn sdk_legacy_container_value(
    value: wit_legacy_containers::Value,
) -> LegacyContainerValue {
    match value {
        wit_legacy_containers::Value::Int(value) => LegacyContainerValue::Int(value),
        wit_legacy_containers::Value::Float(value) => LegacyContainerValue::float(value),
        wit_legacy_containers::Value::Text(value) => LegacyContainerValue::String(value),
        wit_legacy_containers::Value::Form(value) => {
            LegacyContainerValue::Form(value.map(sdk_form_ref))
        }
        wit_legacy_containers::Value::Object(value) => LegacyContainerValue::Object(value),
    }
}

pub(crate) fn wit_legacy_container_value(
    value: &LegacyContainerValue,
) -> wit_legacy_containers::Value {
    match value {
        LegacyContainerValue::Int(value) => wit_legacy_containers::Value::Int(*value),
        LegacyContainerValue::FloatBits(bits) => {
            wit_legacy_containers::Value::Float(f32::from_bits(*bits))
        }
        LegacyContainerValue::String(value) => wit_legacy_containers::Value::Text(value.clone()),
        LegacyContainerValue::Form(value) => {
            wit_legacy_containers::Value::Form(value.map(wit_form_ref))
        }
        LegacyContainerValue::Object(value) => wit_legacy_containers::Value::Object(*value),
    }
}

pub(crate) fn sdk_storage_key(key: String) -> wasmtime::Result<StorageKey> {
    StorageKey::new(key)
        .map_err(|error| wasmtime::Error::msg(format!("invalid storage key: {error}")))
}

pub(crate) fn sdk_storage_value(value: wit_storage::Value) -> ExtensionValue {
    match value {
        wit_storage::Value::Boolean(value) => ExtensionValue::Bool(value),
        wit_storage::Value::Signed(value) => ExtensionValue::I64(value),
        wit_storage::Value::Unsigned(value) => ExtensionValue::U64(value),
        wit_storage::Value::Text(value) => ExtensionValue::String(value),
        wit_storage::Value::Bytes(value) => ExtensionValue::Bytes(value),
    }
}

pub(crate) fn wit_storage_value(value: ExtensionValue) -> wit_storage::Value {
    wit_storage_value_ref(&value)
}

pub(crate) fn wit_storage_value_ref(value: &ExtensionValue) -> wit_storage::Value {
    match value {
        ExtensionValue::Bool(value) => wit_storage::Value::Boolean(*value),
        ExtensionValue::I64(value) => wit_storage::Value::Signed(*value),
        ExtensionValue::U64(value) => wit_storage::Value::Unsigned(*value),
        ExtensionValue::String(value) => wit_storage::Value::Text(value.clone()),
        ExtensionValue::Bytes(value) => wit_storage::Value::Bytes(value.clone()),
    }
}

pub(crate) fn validate_plugin_query(name: &str) -> wasmtime::Result<()> {
    if name.is_empty()
        || name.len() > MAX_PLUGIN_NAME_BYTES
        || name.chars().any(char::is_control)
        || name.contains(['/', '\\'])
    {
        wasmtime::bail!("invalid plugin basename query");
    }
    Ok(())
}

pub(crate) fn sdk_entity_ref(
    entity: state::EntityRef,
) -> wasmtime::Result<byroredux_sdk::identity::EntityRef> {
    byroredux_sdk::identity::EntityRef::new(entity.world_generation, entity.object)
        .ok_or_else(|| wasmtime::Error::msg("entity reference contains a reserved zero value"))
}

pub(crate) fn sdk_form_ref(form: state::FormRef) -> FormRef {
    let mut source = [0_u8; 16];
    source[..8].copy_from_slice(&form.source_high.to_be_bytes());
    source[8..].copy_from_slice(&form.source_low.to_be_bytes());
    FormRef::new(source, form.local)
}

pub(crate) fn wit_actor_value_state(value: ActorValueState) -> actor_values::ActorValueState {
    actor_values::ActorValueState {
        base: value.base(),
        permanent: value.permanent(),
        temporary: value.temporary(),
        damage: value.damage(),
        current: value.current(),
    }
}

pub(crate) fn wit_inventory_snapshot(snapshot: &InventorySnapshot) -> inventory::InventorySnapshot {
    inventory::InventorySnapshot {
        entries: snapshot.entries().iter().map(wit_inventory_entry).collect(),
        truncated: snapshot.truncated(),
    }
}

pub(crate) fn wit_inventory_entry(entry: &InventoryEntry) -> inventory::InventoryEntry {
    inventory::InventoryEntry {
        item: wit_form_ref(entry.item()),
        count: entry.count(),
        biped_slots: entry.biped_slots(),
        weapon_equipped: entry.weapon_equipped(),
        metadata: entry.metadata().map(wit_item_metadata),
    }
}

pub(crate) fn wit_item_metadata(metadata: &ItemMetadata) -> inventory::ItemMetadata {
    let category = match metadata.category() {
        ItemCategory::Misc => inventory::ItemCategory::Misc,
        ItemCategory::Junk => inventory::ItemCategory::Junk,
        ItemCategory::Mod => inventory::ItemCategory::Mod,
        ItemCategory::Book => inventory::ItemCategory::Book,
        ItemCategory::Note => inventory::ItemCategory::Note,
        ItemCategory::Ingredient => inventory::ItemCategory::Ingredient,
        ItemCategory::Aid => inventory::ItemCategory::Aid,
        ItemCategory::Key => inventory::ItemCategory::Key,
        ItemCategory::Ammo => inventory::ItemCategory::Ammo,
        ItemCategory::Armor => inventory::ItemCategory::Armor,
        ItemCategory::Weapon => inventory::ItemCategory::Weapon,
    };
    inventory::ItemMetadata {
        name: metadata.name().to_owned(),
        category,
        value: metadata.value(),
        weight: metadata.weight(),
    }
}

pub(crate) fn wit_faction_snapshot(snapshot: &FactionSnapshot) -> factions::FactionSnapshot {
    factions::FactionSnapshot {
        memberships: snapshot
            .memberships()
            .iter()
            .copied()
            .map(wit_faction_membership)
            .collect(),
        truncated: snapshot.truncated(),
    }
}

pub(crate) fn wit_faction_membership(membership: FactionMembership) -> factions::FactionMembership {
    factions::FactionMembership {
        faction: wit_form_ref(membership.faction()),
        rank: membership.rank(),
    }
}

pub(crate) fn wit_perk_snapshot(snapshot: &PerkSnapshot) -> perks::PerkSnapshot {
    perks::PerkSnapshot {
        entries: snapshot
            .entries()
            .iter()
            .copied()
            .map(wit_perk_entry)
            .collect(),
        truncated: snapshot.truncated(),
    }
}

pub(crate) fn wit_perk_entry(entry: PerkEntry) -> perks::PerkEntry {
    perks::PerkEntry {
        perk: wit_form_ref(entry.perk()),
        rank: entry.rank(),
    }
}

pub(crate) fn wit_package_snapshot(snapshot: &PackageSnapshot) -> packages::PackageSnapshot {
    packages::PackageSnapshot {
        selections: snapshot
            .selections()
            .iter()
            .map(wit_package_selection)
            .collect(),
        truncated: snapshot.truncated(),
    }
}

pub(crate) fn wit_package_selection(selection: &PackageSelection) -> packages::PackageSelection {
    packages::PackageSelection {
        source: match selection.source() {
            PackageSelectionSource::Ambient => packages::SelectionSource::Ambient,
            PackageSelectionSource::Scene => packages::SelectionSource::Scene,
        },
        scene: selection.scene().map(wit_form_ref),
        action_index: selection.action_index(),
        candidates: selection
            .candidates()
            .iter()
            .copied()
            .map(wit_form_ref)
            .collect(),
        active: selection.active().map(wit_form_ref),
        template: selection.template().map(wit_form_ref),
    }
}

pub(crate) fn wit_animation_snapshot(snapshot: AnimationSnapshot) -> animation::AnimationSnapshot {
    animation::AnimationSnapshot {
        requested_idle: snapshot.requested_idle().map(wit_form_ref),
        request_generation: snapshot.request_generation(),
        awaited_event: snapshot.awaited_event().map(wit_animation_event),
        last_event: snapshot.last_event().map(wit_animation_event),
        event_generation: snapshot.event_generation(),
    }
}

pub(crate) fn wit_animation_event(event: AnimationEvent) -> animation::AnimationEvent {
    match event {
        AnimationEvent::PlayImod => animation::AnimationEvent::PlayImod,
        AnimationEvent::IdleFurnitureExit => animation::AnimationEvent::IdleFurnitureExit,
        AnimationEvent::ExitCartEnd => animation::AnimationEvent::ExitCartEnd,
    }
}

pub(crate) fn wit_reputation_snapshot(
    snapshot: &ReputationSnapshot,
) -> reputation::ReputationSnapshot {
    reputation::ReputationSnapshot {
        entries: snapshot
            .entries()
            .iter()
            .copied()
            .map(wit_reputation_entry)
            .collect(),
        truncated: snapshot.truncated(),
    }
}

pub(crate) fn wit_reputation_entry(entry: ReputationEntry) -> reputation::ReputationEntry {
    reputation::ReputationEntry {
        reputation: wit_form_ref(entry.reputation()),
        fame: entry.fame(),
        infamy: entry.infamy(),
    }
}

pub(crate) fn wit_spatial_query_result(
    result: &SpatialQueryResult,
) -> world_spatial::SpatialQueryResult {
    world_spatial::SpatialQueryResult {
        hits: result.hits().iter().copied().map(wit_spatial_hit).collect(),
        truncated: result.truncated(),
    }
}

pub(crate) fn wit_spatial_hit(hit: SpatialHit) -> world_spatial::SpatialHit {
    let reference = hit.reference();
    let [x, y, z] = reference.position();
    world_spatial::SpatialHit {
        reference: wit_form_ref(reference.form()),
        x,
        y,
        z,
        distance: hit.distance(),
    }
}

pub(crate) fn wit_entity_projection(
    projection: &EntityProjection,
    include_transform: bool,
) -> world_state::EntityProjection {
    let entity = projection.entity();
    let form = projection.form().map(wit_form_ref);
    let world_transform = include_transform
        .then(|| projection.world_transform())
        .flatten()
        .map(|transform| {
            let [x, y, z] = transform.translation();
            let [qx, qy, qz, qw] = transform.rotation();
            world_state::WorldTransform {
                translation: world_state::Vec3 { x, y, z },
                rotation: world_state::Quat {
                    x: qx,
                    y: qy,
                    z: qz,
                    w: qw,
                },
                scale: transform.scale(),
            }
        });
    world_state::EntityProjection {
        entity: state::EntityRef {
            world_generation: entity.world_generation(),
            object: entity.object(),
        },
        form,
        name: projection.name().map(str::to_owned),
        world_transform,
    }
}

pub(crate) fn wit_form_ref(form: byroredux_sdk::identity::FormRef) -> state::FormRef {
    let source = form.source();
    state::FormRef {
        source_high: u64::from_be_bytes(source[..8].try_into().expect("eight-byte source half")),
        source_low: u64::from_be_bytes(source[8..].try_into().expect("eight-byte source half")),
        local: form.local(),
    }
}

#[cfg(test)]
mod projection_tests {
    use super::*;
    use byroredux_sdk::content::PluginInfo;
    use byroredux_sdk::identity::FormRef;
    use byroredux_sdk::projection::WorldTransform;
    use std::collections::{BTreeMap, BTreeSet};

    #[test]
    fn wit_projection_preserves_portable_fields_and_redacts_transform_without_grant() {
        let entity = byroredux_sdk::identity::EntityRef::new(2, 9).unwrap();
        let form = FormRef::new([0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15], 42);
        let transform = WorldTransform::new([1.0, 2.0, 3.0], [0.0, 0.0, 0.0, 1.0], 2.0).unwrap();
        let projection =
            EntityProjection::new(entity, Some(form), Some("Door".to_owned()), Some(transform))
                .unwrap();

        let visible = wit_entity_projection(&projection, true);
        assert_eq!(visible.entity.world_generation, 2);
        assert_eq!(visible.entity.object, 9);
        assert_eq!(
            visible.form.as_ref().unwrap().source_high,
            0x0001_0203_0405_0607
        );
        assert_eq!(
            visible.form.as_ref().unwrap().source_low,
            0x0809_0a0b_0c0d_0e0f
        );
        assert_eq!(visible.form.as_ref().unwrap().local, 42);
        assert_eq!(visible.name.as_deref(), Some("Door"));
        assert_eq!(visible.world_transform.as_ref().unwrap().translation.x, 1.0);
        assert_eq!(visible.world_transform.as_ref().unwrap().scale, 2.0);

        assert!(wit_entity_projection(&projection, false)
            .world_transform
            .is_none());
    }

    #[test]
    fn collection_snapshot_reads_preserve_kind_length_and_primitive_values() {
        let mut grants = CapabilitySet::new();
        grants.grant(STORAGE_READ_OWN_CAPABILITY).unwrap();
        let mut state = HostState {
            principal: Principal::new(
                PrincipalId::new("org.example.collections").unwrap(),
                "Collections".to_owned(),
            )
            .unwrap(),
            grants,
            catalog: Arc::new(ServiceCatalog::new(current_sdk_version())),
            limits: StoreLimitsBuilder::new().build(),
            logs: Vec::new(),
            log_bytes: 0,
            max_log_entries: 1,
            max_log_message_bytes: 1,
            max_log_bytes: 1,
            log_budget_exhausted: false,
            schemas: Vec::new(),
            subscribed_to_activate: false,
            subscribed_to_cell_load: false,
            subscribed_to_hit: false,
            subscribed_to_equipment: false,
            subscribed_to_input: false,
            subscribed_to_session: false,
            custom_subscriptions: Vec::new(),
            legacy_mod_event_callbacks: BTreeMap::new(),
            legacy_mod_event_builders: LegacySkseModEventBuilders::new(),
            legacy_containers: LegacyContainerRegistry::new(),
            current_custom_event: None,
            current_legacy_callback: None,
            current_console_args: None,
            console_command_indices: BTreeSet::new(),
            script_functions: BTreeMap::new(),
            current_script_arguments: None,
            current_script_result: None,
            console_output: Vec::new(),
            console_output_bytes: 0,
            console_failed: false,
            console_output_budget_exhausted: false,
            subscribed_to_update: false,
            principal_storage_schema: Some(1),
            principal_storage: BTreeMap::from([
                (
                    StorageKey::new("array").unwrap(),
                    PrincipalStorageValue::Array(vec![ExtensionValue::I64(7)]),
                ),
                (
                    StorageKey::new("map").unwrap(),
                    PrincipalStorageValue::Map(BTreeMap::from([(
                        "entry".to_owned(),
                        ExtensionValue::String("value".to_owned()),
                    )])),
                ),
                (
                    StorageKey::new("set").unwrap(),
                    PrincipalStorageValue::Set(BTreeSet::from([ExtensionValue::U64(9)])),
                ),
            ]),
            entity_projections: BTreeMap::new(),
            spatial_snapshot: Arc::new(SpatialSnapshot::default()),
            content_catalog: Arc::new(ContentCatalog::default()),
            faction_relationships: Arc::new(FactionRelationshipCatalog::default()),
            engine_settings: Arc::new(SettingsSnapshot::default()),
            setting_declarations: Vec::new(),
            pending_commands: Vec::new(),
            max_commands_per_entry: 1,
            accepting_commands: false,
            command_budget_exhausted: false,
        };

        assert!(matches!(
            <HostState as wit_storage::Host>::get_collection_kind(&mut state, "array".to_owned())
                .unwrap(),
            Some(wit_storage::CollectionKind::Array)
        ));
        assert_eq!(
            <HostState as wit_storage::Host>::collection_len(&mut state, "map".to_owned()).unwrap(),
            Some(1)
        );
        assert!(matches!(
            <HostState as wit_storage::Host>::array_get(&mut state, "array".to_owned(), 0).unwrap(),
            Some(wit_storage::Value::Signed(7))
        ));
        assert!(matches!(
            <HostState as wit_storage::Host>::map_get(
                &mut state,
                "map".to_owned(),
                "entry".to_owned()
            )
            .unwrap(),
            Some(wit_storage::Value::Text(value)) if value == "value"
        ));
        assert!(<HostState as wit_storage::Host>::set_contains(
            &mut state,
            "set".to_owned(),
            wit_storage::Value::Unsigned(9),
        )
        .unwrap());
    }

    #[test]
    fn legacy_container_host_preserves_mixed_values_and_nested_handles() {
        let mut state = content_host_state(false);
        state.grants.grant(STORAGE_READ_OWN_CAPABILITY).unwrap();
        state.grants.grant(STORAGE_WRITE_OWN_CAPABILITY).unwrap();

        let array = <HostState as wit_legacy_containers::Host>::array_create(&mut state).unwrap();
        let map = <HostState as wit_legacy_containers::Host>::map_create(&mut state).unwrap();
        assert_ne!(array, 0);
        assert_ne!(map, 0);
        assert!(<HostState as wit_legacy_containers::Host>::array_add(
            &mut state,
            array,
            wit_legacy_containers::Value::Float(2.5),
            None,
        )
        .unwrap());
        assert!(<HostState as wit_legacy_containers::Host>::array_add(
            &mut state,
            array,
            wit_legacy_containers::Value::Form(Some(state::FormRef {
                source_high: 0x0001_0203_0405_0607,
                source_low: 0x0809_0a0b_0c0d_0e0f,
                local: 0x1234,
            })),
            None,
        )
        .unwrap());
        assert!(<HostState as wit_legacy_containers::Host>::map_set(
            &mut state,
            map,
            "items".to_owned(),
            wit_legacy_containers::Value::Object(array),
        )
        .unwrap());
        assert_eq!(
            <HostState as wit_legacy_containers::Host>::count(&mut state, array).unwrap(),
            2
        );
        assert!(matches!(
            <HostState as wit_legacy_containers::Host>::map_get(
                &mut state,
                map,
                "items".to_owned(),
            )
            .unwrap(),
            Some(wit_legacy_containers::Value::Object(handle)) if handle == array
        ));
        assert!(matches!(
            <HostState as wit_legacy_containers::Host>::array_get(&mut state, array, 1).unwrap(),
            Some(wit_legacy_containers::Value::Form(Some(form))) if form.local == 0x1234
        ));

        let mut denied = content_host_state(false);
        assert!(<HostState as wit_legacy_containers::Host>::array_create(&mut denied).is_err());
        assert!(<HostState as wit_legacy_containers::Host>::count(&mut denied, array).is_err());
    }

    #[test]
    fn shared_skse_mod_event_publication_is_capability_gated_and_deferred() {
        let mut state = content_host_state(false);
        state.grants.grant(EVENTS_PUBLISH_CAPABILITY).unwrap();
        state.accepting_commands = true;
        state.max_commands_per_entry = 1;
        let channel =
            byroredux_sdk::event::legacy_skse_mod_event_id("SKICP_configManagerReady").unwrap();

        <HostState as events::Host>::publish(
            &mut state,
            channel.as_str().to_owned(),
            vec![1, 2, 3],
        )
        .unwrap();
        assert_eq!(
            state.pending_commands,
            vec![HostCommand::PublishEvent(PublishEventCommand {
                event: channel,
                payload: vec![1, 2, 3],
            })]
        );

        let mut denied = content_host_state(false);
        denied.accepting_commands = true;
        denied.max_commands_per_entry = 1;
        assert!(<HostState as events::Host>::publish(
            &mut denied,
            byroredux_sdk::event::legacy_skse_mod_event_id("SKICP_configManagerReady")
                .unwrap()
                .as_str()
                .to_owned(),
            Vec::new(),
        )
        .is_err());
        assert!(denied.pending_commands.is_empty());
    }

    #[test]
    fn legacy_mod_event_builder_preserves_typed_arguments_and_send_is_deferred() {
        let mut state = content_host_state(false);
        state.grants.grant(EVENTS_PUBLISH_CAPABILITY).unwrap();
        state.accepting_commands = true;
        state.max_commands_per_entry = 1;

        let handle = <HostState as events::Host>::legacy_builder_create(
            &mut state,
            "typed-ready".to_owned(),
        )
        .unwrap();
        assert_ne!(handle, 0);
        <HostState as events::Host>::legacy_builder_push_bool(&mut state, handle, true).unwrap();
        <HostState as events::Host>::legacy_builder_push_int(&mut state, handle, -7).unwrap();
        <HostState as events::Host>::legacy_builder_push_float(&mut state, handle, 1.25).unwrap();
        <HostState as events::Host>::legacy_builder_push_string(
            &mut state,
            handle,
            "payload".to_owned(),
        )
        .unwrap();
        <HostState as events::Host>::legacy_builder_push_form(
            &mut state,
            handle,
            Some(state::FormRef {
                source_high: 0x0001_0203_0405_0607,
                source_low: 0x0809_0a0b_0c0d_0e0f,
                local: 0x1234,
            }),
        )
        .unwrap();
        assert!(<HostState as events::Host>::legacy_builder_send(&mut state, handle).unwrap());

        let HostCommand::PublishEvent(command) = &state.pending_commands[0] else {
            panic!("builder send must queue a custom event");
        };
        assert_eq!(
            command.event,
            byroredux_sdk::event::legacy_skse_mod_event_id("typed-ready").unwrap()
        );
        let decoded =
            byroredux_sdk::event::LegacySkseVariadicModEventPayload::decode(&command.payload)
                .unwrap();
        assert_eq!(
            decoded.arguments,
            vec![
                LegacySkseModEventValue::Bool(true),
                LegacySkseModEventValue::Int(-7),
                LegacySkseModEventValue::float(1.25),
                LegacySkseModEventValue::String("payload".to_owned()),
                LegacySkseModEventValue::Form(Some(FormRef::new(
                    [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15],
                    0x1234,
                ))),
            ]
        );
        assert!(!<HostState as events::Host>::legacy_builder_send(&mut state, handle).unwrap());

        let mut denied = content_host_state(false);
        denied.accepting_commands = true;
        assert!(<HostState as events::Host>::legacy_builder_create(
            &mut denied,
            "typed-ready".to_owned(),
        )
        .is_err());
    }

    #[test]
    fn legacy_mod_event_builder_survives_a_rejected_send_budget() {
        let mut state = content_host_state(false);
        state.grants.grant(EVENTS_PUBLISH_CAPABILITY).unwrap();
        state.accepting_commands = true;
        state.max_commands_per_entry = 0;
        let handle = <HostState as events::Host>::legacy_builder_create(
            &mut state,
            "retry-ready".to_owned(),
        )
        .unwrap();

        assert!(<HostState as events::Host>::legacy_builder_send(&mut state, handle).is_err());
        assert_eq!(state.legacy_mod_event_builders.len(), 1);
        state.max_commands_per_entry = 1;
        assert!(<HostState as events::Host>::legacy_builder_send(&mut state, handle).unwrap());
        assert!(state.legacy_mod_event_builders.is_empty());
    }

    #[test]
    fn legacy_mod_event_registration_commands_are_bounded_and_deferred() {
        let mut state = content_host_state(false);
        state.grants.grant(EVENTS_SUBSCRIBE_CAPABILITY).unwrap();
        state.accepting_commands = true;
        state.max_commands_per_entry = 3;

        <HostState as events::Host>::queue_legacy_subscribe(
            &mut state,
            "SKICP_configManagerReady".to_owned(),
            "OnConfigManagerReady".to_owned(),
        )
        .unwrap();
        <HostState as events::Host>::queue_legacy_unsubscribe(
            &mut state,
            "SKICP_configManagerReady".to_owned(),
        )
        .unwrap();
        <HostState as events::Host>::queue_legacy_unsubscribe_all(&mut state).unwrap();
        assert_eq!(state.pending_commands.len(), 3);
        assert!(matches!(
            &state.pending_commands[0],
            HostCommand::LegacyModEventSubscription(
                LegacyModEventSubscriptionCommand::Subscribe { callback, .. }
            ) if callback == "OnConfigManagerReady"
        ));

        let mut denied = content_host_state(false);
        denied.accepting_commands = true;
        denied.max_commands_per_entry = 1;
        assert!(<HostState as events::Host>::queue_legacy_subscribe(
            &mut denied,
            "ready".to_owned(),
            "OnReady".to_owned(),
        )
        .is_err());
        assert!(denied.pending_commands.is_empty());
    }

    #[test]
    fn scalar_storage_commands_have_callback_local_read_your_writes() {
        let mut state = content_host_state(false);
        state.grants.grant(STORAGE_READ_OWN_CAPABILITY).unwrap();
        state.grants.grant(STORAGE_WRITE_OWN_CAPABILITY).unwrap();
        state.principal_storage_schema = Some(1);
        state.accepting_commands = true;
        state.max_commands_per_entry = 4;
        state.principal_storage.insert(
            StorageKey::new("counter").unwrap(),
            PrincipalStorageValue::I64(4),
        );

        <HostState as wit_storage::Host>::queue_increment_i64(&mut state, "counter".to_owned(), 3)
            .unwrap();
        assert!(matches!(
            <HostState as wit_storage::Host>::get(&mut state, "counter".to_owned()).unwrap(),
            Some(wit_storage::Value::Signed(7))
        ));

        <HostState as wit_storage::Host>::queue_set(
            &mut state,
            "name".to_owned(),
            wit_storage::Value::Text("Dragonborn".to_owned()),
        )
        .unwrap();
        assert!(matches!(
            <HostState as wit_storage::Host>::get(&mut state, "name".to_owned()).unwrap(),
            Some(wit_storage::Value::Text(value)) if value == "Dragonborn"
        ));

        <HostState as wit_storage::Host>::queue_delete(&mut state, "counter".to_owned()).unwrap();
        assert!(
            <HostState as wit_storage::Host>::get(&mut state, "counter".to_owned())
                .unwrap()
                .is_none()
        );
        assert_eq!(state.pending_commands.len(), 3);
    }

    #[test]
    fn rejected_scalar_storage_command_does_not_mutate_callback_overlay() {
        let mut state = content_host_state(false);
        state.grants.grant(STORAGE_READ_OWN_CAPABILITY).unwrap();
        state.grants.grant(STORAGE_WRITE_OWN_CAPABILITY).unwrap();
        state.principal_storage_schema = Some(1);
        state.accepting_commands = true;
        state.max_commands_per_entry = 0;
        let key = StorageKey::new("counter").unwrap();
        state
            .principal_storage
            .insert(key.clone(), PrincipalStorageValue::I64(4));

        assert!(<HostState as wit_storage::Host>::queue_set(
            &mut state,
            "counter".to_owned(),
            wit_storage::Value::Signed(9),
        )
        .is_err());
        assert_eq!(
            state.principal_storage.get(&key),
            Some(&PrincipalStorageValue::I64(4))
        );
        assert!(state.pending_commands.is_empty());
    }

    fn content_host_state(granted: bool) -> HostState {
        let mut grants = CapabilitySet::new();
        if granted {
            grants.grant(CONTENT_CATALOG_READ_CAPABILITY).unwrap();
        }
        HostState {
            principal: Principal::new(
                PrincipalId::new("org.example.content").unwrap(),
                "Content".to_owned(),
            )
            .unwrap(),
            grants,
            catalog: Arc::new(ServiceCatalog::new(current_sdk_version())),
            limits: StoreLimitsBuilder::new().build(),
            logs: Vec::new(),
            log_bytes: 0,
            max_log_entries: 1,
            max_log_message_bytes: 1,
            max_log_bytes: 1,
            log_budget_exhausted: false,
            schemas: Vec::new(),
            subscribed_to_activate: false,
            subscribed_to_cell_load: false,
            subscribed_to_hit: false,
            subscribed_to_equipment: false,
            subscribed_to_input: false,
            subscribed_to_session: false,
            custom_subscriptions: Vec::new(),
            legacy_mod_event_callbacks: BTreeMap::new(),
            legacy_mod_event_builders: LegacySkseModEventBuilders::new(),
            legacy_containers: LegacyContainerRegistry::new(),
            current_custom_event: None,
            current_legacy_callback: None,
            current_console_args: None,
            console_command_indices: BTreeSet::new(),
            script_functions: BTreeMap::new(),
            current_script_arguments: None,
            current_script_result: None,
            console_output: Vec::new(),
            console_output_bytes: 0,
            console_failed: false,
            console_output_budget_exhausted: false,
            subscribed_to_update: false,
            principal_storage_schema: None,
            principal_storage: BTreeMap::new(),
            entity_projections: BTreeMap::new(),
            spatial_snapshot: Arc::new(SpatialSnapshot::default()),
            content_catalog: Arc::new(
                ContentCatalog::new_with_metadata(
                    vec![
                        PluginInfo::new("Skyrim.esm", 1_u128.to_be_bytes(), PluginKind::Regular)
                            .unwrap(),
                        PluginInfo::new("Creation.esl", 2_u128.to_be_bytes(), PluginKind::Light)
                            .unwrap(),
                    ],
                    vec![vec![], vec![0]],
                    vec![
                        vec![(0x333, *b"AVIF"), (0x1234, *b"WEAP")],
                        vec![(0xabc, *b"STAT")],
                    ],
                )
                .unwrap(),
            ),
            faction_relationships: Arc::new(FactionRelationshipCatalog::default()),
            engine_settings: Arc::new(
                SettingsSnapshot::new([
                    ("render.vsync".to_owned(), SettingValue::Boolean(false)),
                    ("gameplay.fov".to_owned(), SettingValue::Number(120.0)),
                    (
                        "render.upscaler".to_owned(),
                        SettingValue::Choice("taa".to_owned()),
                    ),
                ])
                .unwrap(),
            ),
            setting_declarations: Vec::new(),
            pending_commands: Vec::new(),
            max_commands_per_entry: 1,
            accepting_commands: false,
            command_budget_exhausted: false,
        }
    }

    #[test]
    fn content_catalog_host_reads_are_portable_case_insensitive_and_capability_gated() {
        let mut state = content_host_state(true);
        assert_eq!(
            <HostState as content_catalog::Host>::plugin_count(&mut state).unwrap(),
            2
        );
        let plugin = <HostState as content_catalog::Host>::plugin_at(&mut state, 1)
            .unwrap()
            .unwrap();
        assert_eq!(plugin.name, "Creation.esl");
        assert!(matches!(plugin.kind, content_catalog::PluginKind::Light));
        assert_eq!(plugin.source_high, 0);
        assert_eq!(plugin.source_low, 2);
        assert_eq!(
            <HostState as content_catalog::Host>::find_plugin(
                &mut state,
                "CREATION.ESL".to_owned(),
            )
            .unwrap(),
            Some(1)
        );
        assert_eq!(
            <HostState as content_catalog::Host>::dependency_count(&mut state, 1).unwrap(),
            Some(1)
        );
        assert_eq!(
            <HostState as content_catalog::Host>::dependency_at(&mut state, 1, 0).unwrap(),
            Some(0)
        );
        assert_eq!(
            <HostState as content_catalog::Host>::dependency_at(&mut state, 1, 1).unwrap(),
            None
        );
        let record = <HostState as content_catalog::Host>::get_record(
            &mut state,
            state::FormRef {
                source_high: 0,
                source_low: 2,
                local: 0xabc,
            },
        )
        .unwrap()
        .unwrap();
        assert_eq!(record.record_type, u32::from_be_bytes(*b"STAT"));
        let form = <HostState as content_catalog::Host>::qualify_form(
            &mut state,
            "creation.esl".to_owned(),
            0xabc,
        )
        .unwrap()
        .unwrap();
        assert_eq!(
            (form.source_high, form.source_low, form.local),
            (0, 2, 0xabc)
        );
        assert!(<HostState as content_catalog::Host>::qualify_form(
            &mut state,
            "Creation.esl".to_owned(),
            0x1000,
        )
        .unwrap()
        .is_none());

        let mut denied = content_host_state(false);
        let error = <HostState as content_catalog::Host>::plugin_count(&mut denied).unwrap_err();
        assert!(error.to_string().contains(CONTENT_CATALOG_READ_CAPABILITY));
        let error =
            <HostState as content_catalog::Host>::dependency_count(&mut denied, 1).unwrap_err();
        assert!(error.to_string().contains(CONTENT_CATALOG_READ_CAPABILITY));
        let error = <HostState as content_catalog::Host>::get_record(
            &mut denied,
            state::FormRef {
                source_high: 0,
                source_low: 2,
                local: 0xabc,
            },
        )
        .unwrap_err();
        assert!(error.to_string().contains(CONTENT_CATALOG_READ_CAPABILITY));
        assert!(<HostState as content_catalog::Host>::find_plugin(
            &mut state,
            "../escape.esm".to_owned(),
        )
        .is_err());
    }

    #[test]
    fn actor_values_are_callback_local_portable_deferred_and_capability_gated() {
        let mut state = content_host_state(false);
        state.grants.grant(ACTOR_VALUES_READ_CAPABILITY).unwrap();
        state.grants.grant(ACTOR_VALUES_WRITE_CAPABILITY).unwrap();
        state.accepting_commands = true;
        state.max_commands_per_entry = 2;
        let entity = EntityRef::new(1, 9).unwrap();
        let actor_value = FormRef::new(1_u128.to_be_bytes(), 0x333);
        let projection = EntityProjection::new(entity, None, None, None)
            .unwrap()
            .with_actor_values([(
                actor_value,
                ActorValueState::new(100.0, 20.0, 10.0, 35.0).unwrap(),
            )])
            .unwrap();
        state.entity_projections.insert(entity, projection);
        let wit_entity = state::EntityRef {
            world_generation: 1,
            object: 9,
        };
        let wit_actor_value = wit_form_ref(actor_value);

        let value = <HostState as actor_values::Host>::get(&mut state, wit_entity, wit_actor_value)
            .unwrap()
            .unwrap();
        assert_eq!((value.base, value.current), (100.0, 95.0));
        <HostState as actor_values::Host>::queue(
            &mut state,
            wit_entity,
            wit_actor_value,
            actor_values::Operation::ModifyPermanent,
            5.0,
        )
        .unwrap();
        let [HostCommand::ActorValue(command)] = state.pending_commands.as_slice() else {
            panic!("expected one actor-value command")
        };
        assert_eq!(command.entity(), entity);
        assert_eq!(command.actor_value(), actor_value);
        assert_eq!(command.operation(), ActorValueOperation::ModifyPermanent);
        assert_eq!(command.value(), 5.0);

        let mut denied = content_host_state(false);
        assert!(
            <HostState as actor_values::Host>::get(&mut denied, wit_entity, wit_actor_value,)
                .is_err()
        );
        denied.accepting_commands = true;
        assert!(<HostState as actor_values::Host>::queue(
            &mut denied,
            wit_entity,
            wit_actor_value,
            actor_values::Operation::SetBase,
            1.0,
        )
        .is_err());
    }

    #[test]
    fn inventory_is_callback_local_portable_bounded_and_capability_gated() {
        let entity = EntityRef::new(1, 9).unwrap();
        let item = FormRef::new(1_u128.to_be_bytes(), 0x1234);
        let snapshot = InventorySnapshot::new(
            vec![InventoryEntry::new(
                item,
                7,
                0b101,
                true,
                Some(
                    ItemMetadata::new("Iron Sword".to_owned(), ItemCategory::Weapon, 25, 9.0)
                        .unwrap(),
                ),
            )
            .unwrap()],
            true,
        )
        .unwrap();
        let projection = EntityProjection::new(entity, None, None, None)
            .unwrap()
            .with_inventory(snapshot);
        let wit_entity = state::EntityRef {
            world_generation: 1,
            object: 9,
        };

        let mut state = content_host_state(false);
        state.grants.grant(INVENTORY_READ_CAPABILITY).unwrap();
        state.entity_projections.insert(entity, projection);
        let snapshot = <HostState as inventory::Host>::get(&mut state, wit_entity)
            .unwrap()
            .unwrap();
        assert!(snapshot.truncated);
        assert_eq!(snapshot.entries.len(), 1);
        let entry = &snapshot.entries[0];
        assert_eq!(entry.count, 7);
        assert_eq!(entry.biped_slots, 0b101);
        assert!(entry.weapon_equipped);
        assert_eq!(sdk_form_ref(entry.item), item);
        let metadata = entry.metadata.as_ref().unwrap();
        assert_eq!(metadata.name, "Iron Sword");
        assert!(matches!(metadata.category, inventory::ItemCategory::Weapon));
        assert_eq!((metadata.value, metadata.weight), (25, 9.0));

        let mut denied = content_host_state(false);
        let error = <HostState as inventory::Host>::get(&mut denied, wit_entity).unwrap_err();
        assert!(error.to_string().contains(INVENTORY_READ_CAPABILITY));
    }

    #[test]
    fn factions_are_callback_local_portable_ranked_and_capability_gated() {
        let entity = EntityRef::new(1, 9).unwrap();
        let faction = FormRef::new(1_u128.to_be_bytes(), 0x44);
        let snapshot =
            FactionSnapshot::new(vec![FactionMembership::new(faction, -1).unwrap()], true).unwrap();
        let projection = EntityProjection::new(entity, None, None, None)
            .unwrap()
            .with_factions(snapshot);
        let wit_entity = state::EntityRef {
            world_generation: 1,
            object: 9,
        };

        let mut state = content_host_state(false);
        state.grants.grant(FACTIONS_READ_CAPABILITY).unwrap();
        state.entity_projections.insert(entity, projection);
        let snapshot = <HostState as factions::Host>::get(&mut state, wit_entity)
            .unwrap()
            .unwrap();
        assert!(snapshot.truncated);
        assert_eq!(snapshot.memberships.len(), 1);
        assert_eq!(sdk_form_ref(snapshot.memberships[0].faction), faction);
        assert_eq!(snapshot.memberships[0].rank, -1);

        let mut denied = content_host_state(false);
        let error = <HostState as factions::Host>::get(&mut denied, wit_entity).unwrap_err();
        assert!(error.to_string().contains(FACTIONS_READ_CAPABILITY));
    }

    #[test]
    fn faction_relationships_are_directional_portable_and_capability_gated() {
        use byroredux_sdk::relationships::{FactionRelationship, FactionRelationshipCatalog};

        let source = FormRef::new(1_u128.to_be_bytes(), 0x44);
        let target = FormRef::new(2_u128.to_be_bytes(), 0x55);
        let relationship = FactionRelationship::new(source, target, -35, 1).unwrap();
        let mut state = content_host_state(false);
        state
            .grants
            .grant(FACTION_RELATIONSHIPS_READ_CAPABILITY)
            .unwrap();
        state.faction_relationships =
            Arc::new(FactionRelationshipCatalog::new([relationship], false).unwrap());

        let got = <HostState as faction_relationships::Host>::get(
            &mut state,
            wit_form_ref(source),
            wit_form_ref(target),
        )
        .unwrap()
        .unwrap();
        assert_eq!(got.modifier, -35);
        assert_eq!(got.combat_reaction, 1);
        assert!(!<HostState as faction_relationships::Host>::truncated(&mut state).unwrap());
        assert!(<HostState as faction_relationships::Host>::get(
            &mut state,
            wit_form_ref(target),
            wit_form_ref(source),
        )
        .unwrap()
        .is_none());

        let mut denied = content_host_state(false);
        let error = <HostState as faction_relationships::Host>::get(
            &mut denied,
            wit_form_ref(source),
            wit_form_ref(target),
        )
        .unwrap_err();
        assert!(error
            .to_string()
            .contains(FACTION_RELATIONSHIPS_READ_CAPABILITY));
        let error = <HostState as faction_relationships::Host>::truncated(&mut denied).unwrap_err();
        assert!(error
            .to_string()
            .contains(FACTION_RELATIONSHIPS_READ_CAPABILITY));
    }

    #[test]
    fn perks_are_callback_local_portable_ranked_and_capability_gated() {
        let entity = EntityRef::new(1, 9).unwrap();
        let perk = FormRef::new(1_u128.to_be_bytes(), 0x44);
        let snapshot = PerkSnapshot::new(vec![PerkEntry::new(perk, 2).unwrap()], true).unwrap();
        let projection = EntityProjection::new(entity, None, None, None)
            .unwrap()
            .with_perks(snapshot);
        let wit_entity = state::EntityRef {
            world_generation: 1,
            object: 9,
        };

        let mut state = content_host_state(false);
        state.grants.grant(PERKS_READ_CAPABILITY).unwrap();
        state.entity_projections.insert(entity, projection);
        let snapshot = <HostState as perks::Host>::get(&mut state, wit_entity)
            .unwrap()
            .unwrap();
        assert!(snapshot.truncated);
        assert_eq!(snapshot.entries.len(), 1);
        assert_eq!(sdk_form_ref(snapshot.entries[0].perk), perk);
        assert_eq!(snapshot.entries[0].rank, 2);

        let mut denied = content_host_state(false);
        let error = <HostState as perks::Host>::get(&mut denied, wit_entity).unwrap_err();
        assert!(error.to_string().contains(PERKS_READ_CAPABILITY));
    }

    #[test]
    fn packages_are_portable_deferred_bounded_and_capability_gated() {
        let entity = EntityRef::new(1, 9).unwrap();
        let first = FormRef::new(1_u128.to_be_bytes(), 0x44);
        let second = FormRef::new(1_u128.to_be_bytes(), 0x45);
        let snapshot = PackageSnapshot::new(
            vec![PackageSelection::ambient(vec![first, second], Some(second)).unwrap()],
            true,
        )
        .unwrap();
        let projection = EntityProjection::new(entity, None, None, None)
            .unwrap()
            .with_packages(snapshot);
        let wit_entity = state::EntityRef {
            world_generation: 1,
            object: 9,
        };

        let mut state = content_host_state(false);
        state.grants.grant(PACKAGES_READ_CAPABILITY).unwrap();
        state.grants.grant(PACKAGES_EVALUATE_CAPABILITY).unwrap();
        state.accepting_commands = true;
        state.entity_projections.insert(entity, projection);
        let snapshot = <HostState as packages::Host>::get(&mut state, wit_entity)
            .unwrap()
            .unwrap();
        assert!(snapshot.truncated);
        assert_eq!(snapshot.selections.len(), 1);
        assert_eq!(
            snapshot.selections[0].source,
            packages::SelectionSource::Ambient
        );
        assert_eq!(snapshot.selections[0].candidates.len(), 2);
        assert_eq!(sdk_form_ref(snapshot.selections[0].active.unwrap()), second);
        <HostState as packages::Host>::queue_evaluate(&mut state, wit_entity).unwrap();
        let [HostCommand::EvaluatePackage(command)] = state.pending_commands.as_slice() else {
            panic!("expected one package reevaluation command")
        };
        assert_eq!(command.entity(), entity);

        let mut denied = content_host_state(false);
        denied.accepting_commands = true;
        let error = <HostState as packages::Host>::get(&mut denied, wit_entity).unwrap_err();
        assert!(error.to_string().contains(PACKAGES_READ_CAPABILITY));
        let error =
            <HostState as packages::Host>::queue_evaluate(&mut denied, wit_entity).unwrap_err();
        assert!(error.to_string().contains(PACKAGES_EVALUATE_CAPABILITY));
    }

    #[test]
    fn animation_is_portable_deferred_and_capability_gated() {
        let entity = EntityRef::new(1, 9).unwrap();
        let idle = FormRef::new(1_u128.to_be_bytes(), 0x44);
        let snapshot = AnimationSnapshot::new(
            Some(idle),
            7,
            Some(AnimationEvent::ExitCartEnd),
            Some(AnimationEvent::PlayImod),
            8,
        );
        let projection = EntityProjection::new(entity, None, None, None)
            .unwrap()
            .with_animation(snapshot);
        let wit_entity = state::EntityRef {
            world_generation: 1,
            object: 9,
        };
        let wit_idle = wit_form_ref(idle);

        let mut state = content_host_state(false);
        state.grants.grant(ANIMATION_READ_CAPABILITY).unwrap();
        state.grants.grant(ANIMATION_PLAY_CAPABILITY).unwrap();
        state.accepting_commands = true;
        state.entity_projections.insert(entity, projection);
        let snapshot = <HostState as animation::Host>::get(&mut state, wit_entity)
            .unwrap()
            .unwrap();
        assert_eq!(sdk_form_ref(snapshot.requested_idle.unwrap()), idle);
        assert_eq!(snapshot.request_generation, 7);
        assert_eq!(
            snapshot.awaited_event,
            Some(animation::AnimationEvent::ExitCartEnd)
        );
        assert_eq!(
            snapshot.last_event,
            Some(animation::AnimationEvent::PlayImod)
        );
        assert_eq!(snapshot.event_generation, 8);
        <HostState as animation::Host>::queue_play_idle(&mut state, wit_entity, wit_idle).unwrap();
        let [HostCommand::PlayIdle(command)] = state.pending_commands.as_slice() else {
            panic!("expected one authored animation command")
        };
        assert_eq!(command.entity(), entity);
        assert_eq!(command.idle(), idle);

        let mut denied = content_host_state(false);
        denied.accepting_commands = true;
        let error = <HostState as animation::Host>::get(&mut denied, wit_entity).unwrap_err();
        assert!(error.to_string().contains(ANIMATION_READ_CAPABILITY));
        let error =
            <HostState as animation::Host>::queue_play_idle(&mut denied, wit_entity, wit_idle)
                .unwrap_err();
        assert!(error.to_string().contains(ANIMATION_PLAY_CAPABILITY));
    }

    #[test]
    fn reputation_is_portable_deferred_and_capability_gated() {
        let entity = EntityRef::new(1, 9).unwrap();
        let repu = FormRef::new(1_u128.to_be_bytes(), 0x44);
        let snapshot =
            ReputationSnapshot::new(vec![ReputationEntry::new(repu, 12, 4).unwrap()], true)
                .unwrap();
        let projection = EntityProjection::new(entity, None, None, None)
            .unwrap()
            .with_reputation(snapshot);
        let wit_entity = state::EntityRef {
            world_generation: 1,
            object: 9,
        };
        let wit_repu = wit_form_ref(repu);

        let mut state = content_host_state(false);
        state.grants.grant(REPUTATION_READ_CAPABILITY).unwrap();
        state.grants.grant(REPUTATION_WRITE_CAPABILITY).unwrap();
        state.accepting_commands = true;
        state.entity_projections.insert(entity, projection);
        let snapshot = <HostState as reputation::Host>::get(&mut state, wit_entity)
            .unwrap()
            .unwrap();
        assert!(snapshot.truncated);
        assert_eq!(snapshot.entries.len(), 1);
        assert_eq!(sdk_form_ref(snapshot.entries[0].reputation), repu);
        assert_eq!(
            (snapshot.entries[0].fame, snapshot.entries[0].infamy),
            (12, 4)
        );
        <HostState as reputation::Host>::queue(
            &mut state,
            wit_entity,
            wit_repu,
            reputation::Operation::AddInfamy,
            3,
        )
        .unwrap();
        let [HostCommand::Reputation(command)] = state.pending_commands.as_slice() else {
            panic!("expected one reputation command")
        };
        assert_eq!(command.entity(), entity);
        assert_eq!(command.reputation(), repu);
        assert_eq!(command.operation(), ReputationOperation::AddInfamy);
        assert_eq!(command.points(), 3);

        let mut denied = content_host_state(false);
        denied.accepting_commands = true;
        let error = <HostState as reputation::Host>::get(&mut denied, wit_entity).unwrap_err();
        assert!(error.to_string().contains(REPUTATION_READ_CAPABILITY));
        let error = <HostState as reputation::Host>::queue(
            &mut denied,
            wit_entity,
            wit_repu,
            reputation::Operation::Reset,
            0,
        )
        .unwrap_err();
        assert!(error.to_string().contains(REPUTATION_WRITE_CAPABILITY));
    }

    #[test]
    fn spatial_queries_are_callback_local_portable_bounded_and_capability_gated() {
        let near = FormRef::new(1_u128.to_be_bytes(), 1);
        let far = FormRef::new(2_u128.to_be_bytes(), 1);
        let mut state = content_host_state(false);
        state.grants.grant(WORLD_SPATIAL_READ_CAPABILITY).unwrap();
        state.accepting_commands = true;
        state.spatial_snapshot = Arc::new(
            SpatialSnapshot::new(
                vec![
                    byroredux_sdk::spatial::SpatialReference::new(near, [2.0, 0.0, 0.0]).unwrap(),
                    byroredux_sdk::spatial::SpatialReference::new(far, [5.0, 0.0, 0.0]).unwrap(),
                ],
                false,
            )
            .unwrap(),
        );

        let result =
            <HostState as world_spatial::Host>::nearby(&mut state, 0.0, 0.0, 0.0, 5.0, 1).unwrap();
        assert_eq!(result.hits.len(), 1);
        assert_eq!(sdk_form_ref(result.hits[0].reference), near);
        assert_eq!(result.hits[0].distance, 2.0);
        assert!(result.truncated);

        state.accepting_commands = false;
        assert!(
            <HostState as world_spatial::Host>::nearby(&mut state, 0.0, 0.0, 0.0, 1.0, 1,).is_err()
        );
        let mut denied = content_host_state(false);
        denied.accepting_commands = true;
        let error = <HostState as world_spatial::Host>::nearby(&mut denied, 0.0, 0.0, 0.0, 1.0, 1)
            .unwrap_err();
        assert!(error.to_string().contains(WORLD_SPATIAL_READ_CAPABILITY));
    }

    #[test]
    fn engine_settings_are_typed_bounded_and_capability_gated() {
        let mut state = content_host_state(true);
        state.grants.grant(SETTINGS_READ_CAPABILITY).unwrap();
        assert!(matches!(
            <HostState as context::Host>::engine_setting(&mut state, "gameplay.fov".to_owned(),)
                .unwrap(),
            Some(context::SettingValue::Number(120.0))
        ));
        assert!(
            <HostState as context::Host>::engine_setting(&mut state, "unknown".to_owned())
                .unwrap()
                .is_none()
        );

        let mut denied = content_host_state(false);
        assert!(<HostState as context::Host>::engine_setting(
            &mut denied,
            "render.vsync".to_owned(),
        )
        .is_err());
        assert!(
            <HostState as context::Host>::engine_setting(&mut state, "bad\nkey".to_owned(),)
                .is_err()
        );
    }

    #[test]
    fn own_setting_writes_are_deferred_typed_and_capability_gated() {
        let mut state = content_host_state(false);
        state.grants.grant(SETTINGS_WRITE_OWN_CAPABILITY).unwrap();
        state.accepting_commands = true;
        state.setting_declarations = vec![SettingDeclaration {
            id: byroredux_sdk::identity::SettingId::new("strength").unwrap(),
            label: "Strength".to_owned(),
            description: "Effect strength".to_owned(),
            default: SettingValue::Number(1.0),
            control: byroredux_sdk::settings::SettingControlDeclaration::Slider {
                min: 0.0,
                max: 2.0,
                step: 0.1,
                unit: "x".to_owned(),
            },
            restart_required: false,
        }];

        <HostState as context::Host>::queue_own_setting(
            &mut state,
            0,
            context::SettingValue::Number(1.5),
        )
        .unwrap();
        assert!(matches!(
            state.pending_commands.as_slice(),
            [HostCommand::Setting(SettingWriteCommand { key, value: SettingValue::Number(1.5) })]
                if key == "ext.org.example.content.strength"
        ));
        assert!(<HostState as context::Host>::queue_own_setting(
            &mut state,
            0,
            context::SettingValue::Number(3.0),
        )
        .is_err());

        let mut denied = content_host_state(false);
        denied.accepting_commands = true;
        denied.setting_declarations = state.setting_declarations;
        assert!(<HostState as context::Host>::queue_own_setting(
            &mut denied,
            0,
            context::SettingValue::Number(1.0),
        )
        .is_err());
    }
}
