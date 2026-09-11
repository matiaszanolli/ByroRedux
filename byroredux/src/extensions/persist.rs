//! Extension-state persistence: validation, capture, restore, and the
//! decode half of the saved row format.

use super::*;

impl ExtensionHost {
    fn validate_saved_state(
        &self,
        saved: &ExtensionStateSnapshot,
    ) -> Result<(), ExtensionHostError> {
        if !(MIN_EXTENSION_STATE_FORMAT_VERSION..=EXTENSION_STATE_FORMAT_VERSION)
            .contains(&saved.format_version)
        {
            return Err(ExtensionHostError::UnsupportedStateFormat {
                actual: saved.format_version,
                expected: EXTENSION_STATE_FORMAT_VERSION,
            });
        }
        if saved.rows.len() > MAX_PERSISTED_EXTENSION_ROWS {
            return Err(ExtensionHostError::PersistedRowBudgetExceeded {
                actual: saved.rows.len(),
                maximum: MAX_PERSISTED_EXTENSION_ROWS,
            });
        }

        let active = self.active_principals();
        let mut stable_keys = BTreeSet::new();
        let mut rows_per_principal = BTreeMap::<PrincipalId, usize>::new();
        let mut rebound = Vec::new();
        for (index, row) in saved.rows.iter().enumerate() {
            if !stable_keys.insert((row.principal.clone(), row.schema.clone(), row.entity)) {
                return Err(ExtensionHostError::DuplicatePersistedRow {
                    principal: row.principal.clone(),
                    schema: row.schema.clone(),
                    entity: row.entity,
                });
            }
            let principal_rows = rows_per_principal.entry(row.principal.clone()).or_default();
            *principal_rows += 1;
            if *principal_rows > self.state.limits().max_rows_per_principal {
                return Err(ComponentStoreError::RowBudgetExceeded {
                    principal: row.principal.clone(),
                    maximum: self.state.limits().max_rows_per_principal,
                }
                .into());
            }
            if row.row.iter().count() > self.state.limits().max_fields_per_schema {
                return Err(ComponentStoreError::TooManyFields {
                    schema: row.schema.clone(),
                    maximum: self.state.limits().max_fields_per_schema,
                }
                .into());
            }
            for (field, value) in row.row.iter() {
                let too_large = match value {
                    ExtensionValue::String(value) => (value.len()
                        > self.state.limits().max_string_bytes)
                        .then_some(self.state.limits().max_string_bytes),
                    ExtensionValue::Bytes(value) => (value.len()
                        > self.state.limits().max_blob_bytes)
                        .then_some(self.state.limits().max_blob_bytes),
                    _ => None,
                };
                if let Some(maximum) = too_large {
                    return Err(ComponentStoreError::ValueTooLarge {
                        field: field.clone(),
                        maximum,
                    }
                    .into());
                }
            }
            if !active.contains(&row.principal) {
                continue;
            }
            let object = u64::try_from(index)
                .ok()
                .and_then(|value| value.checked_add(1))
                .ok_or(ExtensionHostError::HandleSpaceExhausted)?;
            let entity =
                EntityRef::new(1, object).ok_or(ExtensionHostError::HandleSpaceExhausted)?;
            rebound.push(RestoredComponentRow {
                principal: row.principal.clone(),
                schema: row.schema.clone(),
                schema_version: row.schema_version,
                entity,
                row: row.row.clone(),
            });
        }
        let mut staged = self.state.clone();
        staged.replace_rows(rebound)?;

        let mut storage_principals = BTreeSet::new();
        let mut active_storage = Vec::new();
        for record in &saved.principal_storage {
            if !storage_principals.insert(record.principal.clone()) {
                return Err(PrincipalStorageError::DuplicatePersistedPrincipal(
                    record.principal.clone(),
                )
                .into());
            }
            self.principal_storage.validate_record_bounds(record)?;
            if active.contains(&record.principal) {
                active_storage.push(record.clone());
            }
        }
        let mut staged_storage = self.principal_storage.clone();
        staged_storage.replace_active(active_storage)?;
        let mut legacy_principals = BTreeSet::new();
        for record in &saved.legacy_containers {
            if !legacy_principals.insert(record.principal.clone()) {
                return Err(ExtensionHostError::DuplicateLegacyContainerPrincipal(
                    record.principal.clone(),
                ));
            }
            record.registry.validate()?;
        }
        let mut builder_principals = BTreeSet::new();
        for record in &saved.legacy_mod_event_builders {
            if !builder_principals.insert(record.principal.clone()) {
                return Err(ExtensionHostError::DuplicateLegacyModEventBuilderPrincipal(
                    record.principal.clone(),
                ));
            }
            if !record.builders.is_valid() {
                return Err(ExtensionHostError::InvalidLegacyModEventBuilders(
                    record.principal.clone(),
                ));
            }
        }
        Ok(())
    }

    pub(super) fn capture_saved_state(
        &self,
        forms_by_entity: &BTreeMap<EntityId, FormRef>,
    ) -> Result<ExtensionStateSnapshot, ExtensionHostError> {
        let mut rows = BTreeMap::new();
        for row in &self.retained_rows {
            rows.insert(
                (row.principal.clone(), row.schema.clone(), row.entity),
                row.clone(),
            );
        }
        let mut unpersistable = 0;
        for (principal, schema, handle, row) in self.state.rows() {
            let Some(entity) = self.handles.by_handle.get(&handle) else {
                unpersistable += 1;
                continue;
            };
            let Some(form) = forms_by_entity.get(entity) else {
                unpersistable += 1;
                continue;
            };
            let declaration = self
                .state
                .schema(principal, schema)
                .expect("materialized extension row always has a registered schema");
            let saved = PersistedComponentRow {
                principal: principal.clone(),
                schema: schema.clone(),
                schema_version: declaration.version,
                entity: *form,
                row: row.clone(),
            };
            rows.insert((principal.clone(), schema.clone(), *form), saved);
        }
        if unpersistable != 0 {
            return Err(ExtensionHostError::UnpersistableRows {
                count: unpersistable,
            });
        }
        let mut principal_storage = BTreeMap::new();
        for record in &self.retained_storage {
            principal_storage.insert(record.principal.clone(), record.clone());
        }
        for record in self.principal_storage.persisted() {
            principal_storage.insert(record.principal.clone(), record);
        }
        let mut legacy_containers = BTreeMap::new();
        for record in &self.retained_legacy_containers {
            legacy_containers.insert(record.principal.clone(), record.clone());
        }
        for (principal, registry) in &self.legacy_containers {
            legacy_containers.insert(
                principal.clone(),
                PersistedLegacyContainers {
                    principal: principal.clone(),
                    registry: registry.clone(),
                },
            );
        }
        let legacy_mod_event_builders = self
            .legacy_mod_event_builders
            .iter()
            .map(|(principal, builders)| PersistedLegacyModEventBuilders {
                principal: principal.clone(),
                builders: builders.clone(),
            })
            .collect();
        let saved = ExtensionStateSnapshot {
            format_version: EXTENSION_STATE_FORMAT_VERSION,
            rows: rows.into_values().collect(),
            principal_storage: principal_storage.into_values().collect(),
            legacy_containers: legacy_containers.into_values().collect(),
            legacy_mod_event_builders,
        };
        self.validate_saved_state(&saved)?;
        Ok(saved)
    }

    pub(super) fn restore_saved_state(
        &mut self,
        saved: &ExtensionStateSnapshot,
        entities_by_form: &BTreeMap<FormRef, EntityId>,
    ) -> Result<u64, ExtensionHostError> {
        self.validate_saved_state(saved)?;
        let active = self.active_principals();
        let mut staged_handles = self.handles.clone();
        let generation = staged_handles.begin_world_generation()?;
        let mut rebound = Vec::new();
        let mut retained = Vec::new();
        for row in &saved.rows {
            if !active.contains(&row.principal)
                || self.state.schema(&row.principal, &row.schema).is_none()
            {
                retained.push(row.clone());
                continue;
            }
            let Some(entity) = entities_by_form.get(&row.entity) else {
                retained.push(row.clone());
                continue;
            };
            rebound.push(RestoredComponentRow {
                principal: row.principal.clone(),
                schema: row.schema.clone(),
                schema_version: row.schema_version,
                entity: staged_handles.handle_for(*entity)?,
                row: row.row.clone(),
            });
        }
        let mut staged_state = self.state.clone();
        staged_state.replace_rows(rebound)?;
        let mut active_storage = Vec::new();
        let mut retained_storage = Vec::new();
        for record in &saved.principal_storage {
            if active.contains(&record.principal)
                && self
                    .principal_storage
                    .schema_version(&record.principal)
                    .is_some()
            {
                active_storage.push(record.clone());
            } else {
                retained_storage.push(record.clone());
            }
        }
        let mut staged_storage = self.principal_storage.clone();
        staged_storage.replace_active(active_storage)?;
        let mut active_legacy_containers = BTreeMap::new();
        let mut retained_legacy_containers = Vec::new();
        for record in &saved.legacy_containers {
            if active.contains(&record.principal) {
                active_legacy_containers.insert(record.principal.clone(), record.registry.clone());
            } else {
                retained_legacy_containers.push(record.clone());
            }
        }
        for principal in &active {
            active_legacy_containers
                .entry(principal.clone())
                .or_default();
        }
        let mut active_mod_event_builders = BTreeMap::new();
        for record in &saved.legacy_mod_event_builders {
            if active.contains(&record.principal) {
                active_mod_event_builders.insert(record.principal.clone(), record.builders.clone());
            }
        }
        for principal in &self.legacy_script_principals {
            active_mod_event_builders
                .entry(principal.clone())
                .or_default();
        }
        self.handles = staged_handles;
        self.state = staged_state;
        self.principal_storage = staged_storage;
        self.retained_rows = retained;
        self.retained_storage = retained_storage;
        self.legacy_containers = active_legacy_containers;
        self.retained_legacy_containers = retained_legacy_containers;
        self.legacy_mod_event_builders = active_mod_event_builders;
        Ok(generation)
    }
}

fn decode_saved_state(
    snapshot: &byroredux_save::Snapshot,
) -> anyhow::Result<ExtensionStateSnapshot> {
    snapshot
        .resources
        .get(EXTENSION_STATE_RESOURCE)
        .cloned()
        .map(serde_json::from_value)
        .transpose()
        .map(|saved| saved.unwrap_or_default())
        .map_err(|error| anyhow::anyhow!("invalid {EXTENSION_STATE_RESOURCE} payload: {error}"))
}

/// Add engine-owned extension state to the checksummed ByroRedux snapshot.
///
/// This refuses a lossy save when a guest attached state to an entity that
/// lacks stable authored identity; transient SDK handles are never serialized.
pub(crate) fn capture_extension_state(
    world: &World,
    snapshot: &mut byroredux_save::Snapshot,
) -> anyhow::Result<usize> {
    let forms = forms_by_entity(world);
    let host = {
        let Some(slot) = world.try_resource::<ExtensionHostSlot>() else {
            return Ok(0);
        };
        slot.host()
    };
    let Some(host) = host else {
        return Ok(0);
    };
    let saved = host
        .lock()
        .expect("ExtensionHost mutex poisoned by a host panic")
        .capture_saved_state(&forms)?;
    let row_count = saved.rows.len();
    snapshot.resources.insert(
        EXTENSION_STATE_RESOURCE.to_owned(),
        serde_json::to_value(saved)
            .map_err(|error| anyhow::anyhow!("could not encode extension state: {error}"))?,
    );
    Ok(row_count)
}

/// Validate extension payloads before the live loader tears down the world.
pub(crate) fn preflight_extension_state(
    world: &World,
    snapshot: &byroredux_save::Snapshot,
) -> anyhow::Result<()> {
    let saved = decode_saved_state(snapshot)?;
    let host = {
        let Some(slot) = world.try_resource::<ExtensionHostSlot>() else {
            if saved.rows.is_empty() && saved.principal_storage.is_empty() {
                return Ok(());
            }
            anyhow::bail!("save contains extension state but the extension host slot is absent");
        };
        slot.host()
    };
    let Some(host) = host else {
        if saved.rows.is_empty() && saved.principal_storage.is_empty() {
            return Ok(());
        }
        anyhow::bail!("save contains extension state but the extension host is unavailable");
    };
    host.lock()
        .expect("ExtensionHost mutex poisoned by a host panic")
        .validate_saved_state(&saved)?;
    Ok(())
}

/// Rebind saved form-backed rows after a successful world replacement.
pub(crate) fn restore_extension_state(
    world: &World,
    snapshot: &byroredux_save::Snapshot,
) -> anyhow::Result<usize> {
    let saved = decode_saved_state(snapshot)?;
    let entities = entities_by_form(world);
    let host = {
        let Some(slot) = world.try_resource::<ExtensionHostSlot>() else {
            if saved.rows.is_empty() && saved.principal_storage.is_empty() {
                return Ok(0);
            }
            anyhow::bail!("save contains extension state but the extension host slot is absent");
        };
        slot.host()
    };
    let Some(host) = host else {
        if saved.rows.is_empty() && saved.principal_storage.is_empty() {
            return Ok(0);
        }
        anyhow::bail!("save contains extension state but the extension host is unavailable");
    };
    let mut host = host
        .lock()
        .expect("ExtensionHost mutex poisoned by a host panic");
    let generation = host.restore_saved_state(&saved, &entities)?;
    let retained = host.retained_rows.len();
    let retained_storage = host.retained_storage.len();
    log::debug!("extension entity-handle generation is now {generation}");
    if retained != 0 {
        log::info!("retained {retained} extension state row(s) for unavailable packages or forms");
    }
    if retained_storage != 0 {
        log::info!(
            "retained {retained_storage} principal storage record(s) for unavailable packages"
        );
    }
    Ok(saved.rows.len().saturating_sub(retained))
}

/// Stop all executable components without holding an ECS resource guard.
pub(crate) fn shutdown_extension_host(world: &World) {
    let host = {
        let Some(slot) = world.try_resource::<ExtensionHostSlot>() else {
            return;
        };
        slot.host()
    };
    let Some(host) = host else {
        return;
    };
    let diagnostics = {
        let mut host = host
            .lock()
            .expect("ExtensionHost mutex poisoned by a host panic");
        host.shutdown_all();
        host.take_diagnostics()
    };
    emit_diagnostics(diagnostics);
}
