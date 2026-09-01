//! Principal-isolated dynamic component state and deferred mutations.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::identity::{ComponentFieldId, ComponentSchemaId, EntityRef, FormRef, PrincipalId};
use crate::legacy_containers::PersistedLegacyContainers;
use crate::storage::PersistedPrincipalStorage;

/// Current engine-owned extension-state payload format.
pub const EXTENSION_STATE_FORMAT_VERSION: u32 = 3;

/// Oldest extension-state payload accepted by this SDK. Version 2 predates
/// legacy container records, which deserialize as an empty list.
pub const MIN_EXTENSION_STATE_FORMAT_VERSION: u32 = 2;

/// Portable value kinds supported by the first dynamic-component contract.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ExtensionValueType {
    Bool,
    I64,
    U64,
    String,
    Bytes,
}

/// Bounded, serialization-safe value stored in an extension-owned row.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "kebab-case")]
pub enum ExtensionValue {
    Bool(bool),
    I64(i64),
    U64(u64),
    String(String),
    Bytes(Vec<u8>),
}

impl ExtensionValue {
    /// Portable type tag used for schema and storage validation.
    pub fn value_type(&self) -> ExtensionValueType {
        match self {
            Self::Bool(_) => ExtensionValueType::Bool,
            Self::I64(_) => ExtensionValueType::I64,
            Self::U64(_) => ExtensionValueType::U64,
            Self::String(_) => ExtensionValueType::String,
            Self::Bytes(_) => ExtensionValueType::Bytes,
        }
    }
}

/// One typed field in a principal-owned schema.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ComponentFieldDeclaration {
    pub id: ComponentFieldId,
    pub value_type: ExtensionValueType,
}

/// Materialized schema registered by the engine for one principal.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ComponentSchema {
    pub id: ComponentSchemaId,
    pub version: u32,
    pub fields: Vec<ComponentFieldDeclaration>,
}

/// One sparse extension-component row.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ComponentRow(BTreeMap<ComponentFieldId, ExtensionValue>);

impl ComponentRow {
    /// Read one value by its stable field identity.
    pub fn get(&self, field: &str) -> Option<&ExtensionValue> {
        self.0.get(field)
    }

    /// Iterate fields in stable identity order.
    pub fn iter(&self) -> impl Iterator<Item = (&ComponentFieldId, &ExtensionValue)> {
        self.0.iter()
    }

    /// Return whether this row carries no materialized fields.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// One extension-owned row encoded against stable authored identity.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PersistedComponentRow {
    pub principal: PrincipalId,
    pub schema: ComponentSchemaId,
    pub schema_version: u32,
    pub entity: FormRef,
    pub row: ComponentRow,
}

/// Versioned payload embedded in the ByroRedux save container.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExtensionStateSnapshot {
    pub format_version: u32,
    pub rows: Vec<PersistedComponentRow>,
    #[serde(default)]
    pub principal_storage: Vec<PersistedPrincipalStorage>,
    #[serde(default)]
    pub legacy_containers: Vec<PersistedLegacyContainers>,
}

impl Default for ExtensionStateSnapshot {
    fn default() -> Self {
        Self {
            format_version: EXTENSION_STATE_FORMAT_VERSION,
            rows: Vec::new(),
            principal_storage: Vec::new(),
            legacy_containers: Vec::new(),
        }
    }
}

/// A persisted row after its stable form has been rebound by the host.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RestoredComponentRow {
    pub principal: PrincipalId,
    pub schema: ComponentSchemaId,
    pub schema_version: u32,
    pub entity: EntityRef,
    pub row: ComponentRow,
}

/// A mutation produced by a sandbox callback but not yet applied to the world.
///
/// Principal identity is intentionally absent. The host supplies it from the
/// isolated instance that produced the command, so a guest cannot forge a
/// target principal on the wire.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExtensionCommand {
    Set {
        entity: EntityRef,
        schema: ComponentSchemaId,
        field: ComponentFieldId,
        value: ExtensionValue,
    },
    IncrementI64 {
        entity: EntityRef,
        schema: ComponentSchemaId,
        field: ComponentFieldId,
        delta: i64,
    },
}

/// Hard bounds for one engine-owned extension component store.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ComponentStoreLimits {
    pub max_schemas_per_principal: usize,
    pub max_fields_per_schema: usize,
    pub max_rows_per_principal: usize,
    pub max_commands_per_batch: usize,
    pub max_string_bytes: usize,
    pub max_blob_bytes: usize,
}

impl Default for ComponentStoreLimits {
    fn default() -> Self {
        Self {
            max_schemas_per_principal: 256,
            max_fields_per_schema: 128,
            max_rows_per_principal: 65_536,
            max_commands_per_batch: 1_024,
            max_string_bytes: 16 * 1024,
            max_blob_bytes: 64 * 1024,
        }
    }
}

/// Rejection from schema registration or atomic command application.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum ComponentStoreError {
    #[error("{field} must be non-zero")]
    InvalidLimit { field: &'static str },
    #[error("schema version must be positive")]
    ZeroSchemaVersion,
    #[error("schema {schema} declares no fields")]
    EmptySchema { schema: ComponentSchemaId },
    #[error("schema {schema} exceeds the field limit of {maximum}")]
    TooManyFields {
        schema: ComponentSchemaId,
        maximum: usize,
    },
    #[error("schema {schema} repeats field {field}")]
    DuplicateField {
        schema: ComponentSchemaId,
        field: ComponentFieldId,
    },
    #[error("principal {principal} already registered schema {schema}")]
    DuplicateSchema {
        principal: PrincipalId,
        schema: ComponentSchemaId,
    },
    #[error("principal {principal} exceeds the schema limit of {maximum}")]
    TooManySchemas {
        principal: PrincipalId,
        maximum: usize,
    },
    #[error("principal {principal} has no schema {schema}")]
    UnknownSchema {
        principal: PrincipalId,
        schema: ComponentSchemaId,
    },
    #[error("schema {schema} has no field {field}")]
    UnknownField {
        schema: ComponentSchemaId,
        field: ComponentFieldId,
    },
    #[error("field {field} in schema {schema} is {actual:?}, expected {expected:?}")]
    TypeMismatch {
        schema: ComponentSchemaId,
        field: ComponentFieldId,
        expected: ExtensionValueType,
        actual: ExtensionValueType,
    },
    #[error("batch has {actual} commands, exceeding the limit of {maximum}")]
    CommandBudgetExceeded { actual: usize, maximum: usize },
    #[error("principal {principal} exceeds the row limit of {maximum}")]
    RowBudgetExceeded {
        principal: PrincipalId,
        maximum: usize,
    },
    #[error("integer overflow incrementing {schema}.{field}")]
    IntegerOverflow {
        schema: ComponentSchemaId,
        field: ComponentFieldId,
    },
    #[error("field {field} exceeds its value-size limit of {maximum} bytes")]
    ValueTooLarge {
        field: ComponentFieldId,
        maximum: usize,
    },
    #[error(
        "schema {schema} for principal {principal} is version {actual}, but saved state requires {saved}"
    )]
    SchemaVersionMismatch {
        principal: PrincipalId,
        schema: ComponentSchemaId,
        saved: u32,
        actual: u32,
    },
    #[error("saved state repeats row {principal}/{schema} on entity {entity:?}")]
    DuplicateRestoredRow {
        principal: PrincipalId,
        schema: ComponentSchemaId,
        entity: EntityRef,
    },
}

type SchemaKey = (PrincipalId, ComponentSchemaId);
type RowKey = (PrincipalId, ComponentSchemaId, EntityRef);

/// Engine-owned storage for extension schemas and rows.
#[derive(Clone, Debug)]
pub struct ExtensionComponentStore {
    limits: ComponentStoreLimits,
    schemas: BTreeMap<SchemaKey, ComponentSchema>,
    rows: BTreeMap<RowKey, ComponentRow>,
}

impl ExtensionComponentStore {
    pub fn new(limits: ComponentStoreLimits) -> Result<Self, ComponentStoreError> {
        for (field, value) in [
            (
                "max_schemas_per_principal",
                limits.max_schemas_per_principal,
            ),
            ("max_fields_per_schema", limits.max_fields_per_schema),
            ("max_rows_per_principal", limits.max_rows_per_principal),
            ("max_commands_per_batch", limits.max_commands_per_batch),
            ("max_string_bytes", limits.max_string_bytes),
            ("max_blob_bytes", limits.max_blob_bytes),
        ] {
            if value == 0 {
                return Err(ComponentStoreError::InvalidLimit { field });
            }
        }
        Ok(Self {
            limits,
            schemas: BTreeMap::new(),
            rows: BTreeMap::new(),
        })
    }

    /// Hard limits governing schemas, rows, values, and callback batches.
    pub fn limits(&self) -> &ComponentStoreLimits {
        &self.limits
    }

    /// Register a schema under the authenticated package principal.
    pub fn register_schema(
        &mut self,
        principal: &PrincipalId,
        schema: ComponentSchema,
    ) -> Result<(), ComponentStoreError> {
        if schema.version == 0 {
            return Err(ComponentStoreError::ZeroSchemaVersion);
        }
        if schema.fields.is_empty() {
            return Err(ComponentStoreError::EmptySchema { schema: schema.id });
        }
        if schema.fields.len() > self.limits.max_fields_per_schema {
            return Err(ComponentStoreError::TooManyFields {
                schema: schema.id,
                maximum: self.limits.max_fields_per_schema,
            });
        }
        let mut fields = BTreeSet::new();
        for field in &schema.fields {
            if !fields.insert(field.id.clone()) {
                return Err(ComponentStoreError::DuplicateField {
                    schema: schema.id,
                    field: field.id.clone(),
                });
            }
        }
        let key = (principal.clone(), schema.id.clone());
        if self.schemas.contains_key(&key) {
            return Err(ComponentStoreError::DuplicateSchema {
                principal: principal.clone(),
                schema: schema.id,
            });
        }
        let schema_count = self
            .schemas
            .keys()
            .filter(|(owner, _)| owner == principal)
            .count();
        if schema_count >= self.limits.max_schemas_per_principal {
            return Err(ComponentStoreError::TooManySchemas {
                principal: principal.clone(),
                maximum: self.limits.max_schemas_per_principal,
            });
        }
        self.schemas.insert(key, schema);
        Ok(())
    }

    /// Read a row only within the authenticated principal's namespace.
    pub fn row(
        &self,
        principal: &PrincipalId,
        schema: &ComponentSchemaId,
        entity: EntityRef,
    ) -> Option<&ComponentRow> {
        self.rows.get(&(principal.clone(), schema.clone(), entity))
    }

    /// Return a registered schema in its authenticated namespace.
    pub fn schema(
        &self,
        principal: &PrincipalId,
        schema: &ComponentSchemaId,
    ) -> Option<&ComponentSchema> {
        self.schemas.get(&(principal.clone(), schema.clone()))
    }

    /// Iterate materialized rows in deterministic key order.
    pub fn rows(
        &self,
    ) -> impl Iterator<Item = (&PrincipalId, &ComponentSchemaId, EntityRef, &ComponentRow)> {
        self.rows
            .iter()
            .map(|((principal, schema, entity), row)| (principal, schema, *entity, row))
    }

    /// Remove all entity-bound rows while retaining registered schemas.
    ///
    /// Entity references are scoped to one loaded-world generation. Save
    /// restoration can repopulate migrated rows after assigning fresh handles.
    pub fn clear_rows(&mut self) -> usize {
        let removed = self.rows.len();
        self.rows.clear();
        removed
    }

    /// Atomically replace all live rows with pre-rebound saved rows.
    ///
    /// The host authenticates principals and resolves stable form identities;
    /// this layer revalidates schema versions, types, value bounds, duplicate
    /// keys, and per-principal row budgets before publishing any state.
    pub fn replace_rows(
        &mut self,
        rows: impl IntoIterator<Item = RestoredComponentRow>,
    ) -> Result<(), ComponentStoreError> {
        self.apply_restored_rows(rows, true)
    }

    /// Atomically merge pre-rebound rows into the live store.
    ///
    /// Hosts use this when a previously unavailable stable form streams back
    /// into the world. Existing unrelated rows remain intact.
    pub fn merge_rows(
        &mut self,
        rows: impl IntoIterator<Item = RestoredComponentRow>,
    ) -> Result<(), ComponentStoreError> {
        self.apply_restored_rows(rows, false)
    }

    fn apply_restored_rows(
        &mut self,
        rows: impl IntoIterator<Item = RestoredComponentRow>,
        replace: bool,
    ) -> Result<(), ComponentStoreError> {
        let mut staged = self.clone();
        if replace {
            staged.rows.clear();
        }
        let mut keys = BTreeSet::new();
        for restored in rows {
            let key = (
                restored.principal.clone(),
                restored.schema.clone(),
                restored.entity,
            );
            if !keys.insert(key) {
                return Err(ComponentStoreError::DuplicateRestoredRow {
                    principal: restored.principal,
                    schema: restored.schema,
                    entity: restored.entity,
                });
            }
            let declaration = staged
                .schema(&restored.principal, &restored.schema)
                .ok_or_else(|| ComponentStoreError::UnknownSchema {
                    principal: restored.principal.clone(),
                    schema: restored.schema.clone(),
                })?;
            if declaration.version != restored.schema_version {
                return Err(ComponentStoreError::SchemaVersionMismatch {
                    principal: restored.principal,
                    schema: restored.schema,
                    saved: restored.schema_version,
                    actual: declaration.version,
                });
            }
            for (field, value) in restored.row.iter() {
                staged.apply_batch(
                    &restored.principal,
                    &[ExtensionCommand::Set {
                        entity: restored.entity,
                        schema: restored.schema.clone(),
                        field: field.clone(),
                        value: value.clone(),
                    }],
                )?;
            }
        }
        self.rows = staged.rows;
        Ok(())
    }

    /// Apply one callback's commands atomically under its authenticated owner.
    pub fn apply_batch(
        &mut self,
        principal: &PrincipalId,
        commands: &[ExtensionCommand],
    ) -> Result<(), ComponentStoreError> {
        if commands.len() > self.limits.max_commands_per_batch {
            return Err(ComponentStoreError::CommandBudgetExceeded {
                actual: commands.len(),
                maximum: self.limits.max_commands_per_batch,
            });
        }

        let mut staged = BTreeMap::<RowKey, ComponentRow>::new();
        let existing_rows = self
            .rows
            .keys()
            .filter(|(owner, _, _)| owner == principal)
            .count();
        let mut new_rows = BTreeSet::new();

        for command in commands {
            match command {
                ExtensionCommand::Set {
                    entity,
                    schema,
                    field,
                    value,
                } => {
                    let schema_key = (principal.clone(), schema.clone());
                    let declaration = self.schemas.get(&schema_key).ok_or_else(|| {
                        ComponentStoreError::UnknownSchema {
                            principal: principal.clone(),
                            schema: schema.clone(),
                        }
                    })?;
                    let field_declaration = declaration
                        .fields
                        .iter()
                        .find(|candidate| candidate.id == *field)
                        .ok_or_else(|| ComponentStoreError::UnknownField {
                            schema: schema.clone(),
                            field: field.clone(),
                        })?;
                    let actual = value.value_type();
                    if field_declaration.value_type != actual {
                        return Err(ComponentStoreError::TypeMismatch {
                            schema: schema.clone(),
                            field: field.clone(),
                            expected: field_declaration.value_type,
                            actual,
                        });
                    }
                    let value_limit = match value {
                        ExtensionValue::String(value)
                            if value.len() > self.limits.max_string_bytes =>
                        {
                            Some(self.limits.max_string_bytes)
                        }
                        ExtensionValue::Bytes(value)
                            if value.len() > self.limits.max_blob_bytes =>
                        {
                            Some(self.limits.max_blob_bytes)
                        }
                        _ => None,
                    };
                    if let Some(maximum) = value_limit {
                        return Err(ComponentStoreError::ValueTooLarge {
                            field: field.clone(),
                            maximum,
                        });
                    }

                    let row_key = (principal.clone(), schema.clone(), *entity);
                    let row_exists = self.rows.contains_key(&row_key);
                    if !row_exists
                        && new_rows.insert(row_key.clone())
                        && existing_rows + new_rows.len() > self.limits.max_rows_per_principal
                    {
                        return Err(ComponentStoreError::RowBudgetExceeded {
                            principal: principal.clone(),
                            maximum: self.limits.max_rows_per_principal,
                        });
                    }
                    let row = staged
                        .entry(row_key.clone())
                        .or_insert_with(|| self.rows.get(&row_key).cloned().unwrap_or_default());
                    row.0.insert(field.clone(), value.clone());
                }
                ExtensionCommand::IncrementI64 {
                    entity,
                    schema,
                    field,
                    delta,
                } => {
                    let schema_key = (principal.clone(), schema.clone());
                    let declaration = self.schemas.get(&schema_key).ok_or_else(|| {
                        ComponentStoreError::UnknownSchema {
                            principal: principal.clone(),
                            schema: schema.clone(),
                        }
                    })?;
                    let field_declaration = declaration
                        .fields
                        .iter()
                        .find(|candidate| candidate.id == *field)
                        .ok_or_else(|| ComponentStoreError::UnknownField {
                            schema: schema.clone(),
                            field: field.clone(),
                        })?;
                    if field_declaration.value_type != ExtensionValueType::I64 {
                        return Err(ComponentStoreError::TypeMismatch {
                            schema: schema.clone(),
                            field: field.clone(),
                            expected: field_declaration.value_type,
                            actual: ExtensionValueType::I64,
                        });
                    }

                    let row_key = (principal.clone(), schema.clone(), *entity);
                    let row_exists = self.rows.contains_key(&row_key);
                    if !row_exists
                        && new_rows.insert(row_key.clone())
                        && existing_rows + new_rows.len() > self.limits.max_rows_per_principal
                    {
                        return Err(ComponentStoreError::RowBudgetExceeded {
                            principal: principal.clone(),
                            maximum: self.limits.max_rows_per_principal,
                        });
                    }
                    let row = staged
                        .entry(row_key.clone())
                        .or_insert_with(|| self.rows.get(&row_key).cloned().unwrap_or_default());
                    let current = match row.0.get(field) {
                        Some(ExtensionValue::I64(value)) => *value,
                        Some(value) => {
                            return Err(ComponentStoreError::TypeMismatch {
                                schema: schema.clone(),
                                field: field.clone(),
                                expected: ExtensionValueType::I64,
                                actual: value.value_type(),
                            });
                        }
                        None => 0,
                    };
                    let value = current.checked_add(*delta).ok_or_else(|| {
                        ComponentStoreError::IntegerOverflow {
                            schema: schema.clone(),
                            field: field.clone(),
                        }
                    })?;
                    row.0.insert(field.clone(), ExtensionValue::I64(value));
                }
            }
        }

        self.rows.extend(staged);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn principal(id: &str) -> PrincipalId {
        PrincipalId::new(id).unwrap()
    }

    fn schema() -> ComponentSchema {
        ComponentSchema {
            id: ComponentSchemaId::new("example.activation-count").unwrap(),
            version: 1,
            fields: vec![
                ComponentFieldDeclaration {
                    id: ComponentFieldId::new("count").unwrap(),
                    value_type: ExtensionValueType::I64,
                },
                ComponentFieldDeclaration {
                    id: ComponentFieldId::new("label").unwrap(),
                    value_type: ExtensionValueType::String,
                },
            ],
        }
    }

    fn increment(entity: EntityRef, delta: i64) -> ExtensionCommand {
        ExtensionCommand::IncrementI64 {
            entity,
            schema: ComponentSchemaId::new("example.activation-count").unwrap(),
            field: ComponentFieldId::new("count").unwrap(),
            delta,
        }
    }

    #[test]
    fn identical_schema_ids_are_isolated_by_principal() {
        let mut store = ExtensionComponentStore::new(ComponentStoreLimits::default()).unwrap();
        let a = principal("org.example.a");
        let b = principal("org.example.b");
        store.register_schema(&a, schema()).unwrap();
        store.register_schema(&b, schema()).unwrap();
        let entity = EntityRef::new(1, 9).unwrap();

        store.apply_batch(&a, &[increment(entity, 2)]).unwrap();
        assert_eq!(
            store
                .row(&a, &schema().id, entity)
                .and_then(|row| row.get("count")),
            Some(&ExtensionValue::I64(2))
        );
        assert_eq!(store.row(&b, &schema().id, entity), None);
    }

    #[test]
    fn a_failing_batch_has_no_partial_effect() {
        let mut store = ExtensionComponentStore::new(ComponentStoreLimits::default()).unwrap();
        let owner = principal("org.example.atomic");
        store.register_schema(&owner, schema()).unwrap();
        let entity = EntityRef::new(1, 3).unwrap();
        store
            .apply_batch(&owner, &[increment(entity, i64::MAX)])
            .unwrap();

        let error = store.apply_batch(&owner, &[increment(entity, 1), increment(entity, 1)]);
        assert!(matches!(
            error,
            Err(ComponentStoreError::IntegerOverflow { .. })
        ));
        assert_eq!(
            store
                .row(&owner, &schema().id, entity)
                .and_then(|row| row.get("count")),
            Some(&ExtensionValue::I64(i64::MAX))
        );
    }

    #[test]
    fn command_and_row_budgets_reject_without_mutation() {
        let mut store = ExtensionComponentStore::new(ComponentStoreLimits {
            max_commands_per_batch: 1,
            max_rows_per_principal: 1,
            ..ComponentStoreLimits::default()
        })
        .unwrap();
        let owner = principal("org.example.bounded");
        store.register_schema(&owner, schema()).unwrap();
        let first = EntityRef::new(1, 1).unwrap();
        let second = EntityRef::new(1, 2).unwrap();

        assert!(matches!(
            store.apply_batch(&owner, &[increment(first, 1), increment(second, 1)]),
            Err(ComponentStoreError::CommandBudgetExceeded { .. })
        ));
        assert!(store.row(&owner, &schema().id, first).is_none());
        store.apply_batch(&owner, &[increment(first, 1)]).unwrap();
        assert!(matches!(
            store.apply_batch(&owner, &[increment(second, 1)]),
            Err(ComponentStoreError::RowBudgetExceeded { .. })
        ));
        assert!(store.row(&owner, &schema().id, second).is_none());
    }

    #[test]
    fn clearing_rows_preserves_registered_schemas() {
        let mut store = ExtensionComponentStore::new(ComponentStoreLimits::default()).unwrap();
        let owner = principal("org.example.world-boundary");
        store.register_schema(&owner, schema()).unwrap();
        let entity = EntityRef::new(1, 1).unwrap();
        store.apply_batch(&owner, &[increment(entity, 1)]).unwrap();

        assert_eq!(store.clear_rows(), 1);
        assert!(store.row(&owner, &schema().id, entity).is_none());
        let replacement = EntityRef::new(2, 1).unwrap();
        store
            .apply_batch(&owner, &[increment(replacement, 1)])
            .unwrap();
        assert!(store.row(&owner, &schema().id, replacement).is_some());
    }

    #[test]
    fn saved_rows_replace_atomically_and_require_exact_schema_versions() {
        let mut store = ExtensionComponentStore::new(ComponentStoreLimits::default()).unwrap();
        let owner = principal("org.example.restore");
        store.register_schema(&owner, schema()).unwrap();
        let old = EntityRef::new(1, 1).unwrap();
        store.apply_batch(&owner, &[increment(old, 3)]).unwrap();

        let replacement = EntityRef::new(2, 9).unwrap();
        let mut row = ComponentRow::default();
        row.0.insert(
            ComponentFieldId::new("count").unwrap(),
            ExtensionValue::I64(7),
        );
        let restored = RestoredComponentRow {
            principal: owner.clone(),
            schema: schema().id,
            schema_version: 1,
            entity: replacement,
            row: row.clone(),
        };
        store.replace_rows([restored.clone()]).unwrap();
        assert!(store.row(&owner, &schema().id, old).is_none());
        assert_eq!(store.row(&owner, &schema().id, replacement), Some(&row));

        let mismatched = RestoredComponentRow {
            schema_version: 2,
            ..restored
        };
        assert!(matches!(
            store.replace_rows([mismatched]),
            Err(ComponentStoreError::SchemaVersionMismatch { .. })
        ));
        assert_eq!(store.row(&owner, &schema().id, replacement), Some(&row));
    }

    #[test]
    fn set_commands_enforce_schema_types_and_value_bounds() {
        let mut store = ExtensionComponentStore::new(ComponentStoreLimits {
            max_string_bytes: 3,
            ..ComponentStoreLimits::default()
        })
        .unwrap();
        let owner = principal("org.example.values");
        store.register_schema(&owner, schema()).unwrap();
        let entity = EntityRef::new(1, 1).unwrap();
        let label = ComponentFieldId::new("label").unwrap();
        let command = |value| ExtensionCommand::Set {
            entity,
            schema: schema().id,
            field: label.clone(),
            value,
        };

        assert!(matches!(
            store.apply_batch(&owner, &[command(ExtensionValue::I64(1))]),
            Err(ComponentStoreError::TypeMismatch { .. })
        ));
        assert!(matches!(
            store.apply_batch(
                &owner,
                &[command(ExtensionValue::String("four".to_owned()))]
            ),
            Err(ComponentStoreError::ValueTooLarge { .. })
        ));
        assert!(store.row(&owner, &schema().id, entity).is_none());
        store
            .apply_batch(&owner, &[command(ExtensionValue::String("ok".to_owned()))])
            .unwrap();
        assert_eq!(
            store
                .row(&owner, &schema().id, entity)
                .and_then(|row| row.get("label")),
            Some(&ExtensionValue::String("ok".to_owned()))
        );
    }
}
