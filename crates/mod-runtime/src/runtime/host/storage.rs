//! `storage` WIT host interface.

use crate::runtime::*;

impl wit_storage::Host for HostState {
    fn schema_version(&mut self) -> wasmtime::Result<Option<u32>> {
        Ok(self.principal_storage_schema)
    }

    fn get(&mut self, key: String) -> wasmtime::Result<Option<wit_storage::Value>> {
        self.require_storage(STORAGE_READ_OWN_CAPABILITY)?;
        let key = sdk_storage_key(key)?;
        Ok(self
            .principal_storage
            .get(&key)
            .and_then(PrincipalStorageValue::as_scalar)
            .map(wit_storage_value))
    }

    fn queue_set(&mut self, key: String, value: wit_storage::Value) -> wasmtime::Result<()> {
        self.require_storage_write()?;
        let key = sdk_storage_key(key)?;
        let value = sdk_storage_value(value);
        // The engine still commits the command after guest return, but the
        // callback-local snapshot is a transaction overlay: later reads in
        // this same callback observe accepted writes. Every entry receives a
        // fresh committed snapshot from ExtensionHost, so a trapped callback
        // cannot leak this speculative value into a later entry.
        self.principal_storage
            .insert(key.clone(), value.clone().into());
        self.pending_commands.push(HostCommand::PrincipalStorage(
            PrincipalStorageCommand::Set { key, value },
        ));
        Ok(())
    }

    fn queue_delete(&mut self, key: String) -> wasmtime::Result<()> {
        self.require_storage_write()?;
        let key = sdk_storage_key(key)?;
        self.principal_storage.remove(&key);
        self.pending_commands.push(HostCommand::PrincipalStorage(
            PrincipalStorageCommand::Delete { key },
        ));
        Ok(())
    }

    fn queue_increment_i64(&mut self, key: String, delta: i64) -> wasmtime::Result<()> {
        self.require_storage_write()?;
        let key = sdk_storage_key(key)?;
        let current = match self.principal_storage.get(&key) {
            Some(PrincipalStorageValue::I64(value)) => *value,
            Some(_) => wasmtime::bail!("storage key {key} is not a signed integer"),
            None => 0,
        };
        let next = current
            .checked_add(delta)
            .ok_or_else(|| wasmtime::Error::msg(format!("storage key {key} overflowed i64")))?;
        self.principal_storage
            .insert(key.clone(), PrincipalStorageValue::I64(next));
        self.pending_commands.push(HostCommand::PrincipalStorage(
            PrincipalStorageCommand::IncrementI64 { key, delta },
        ));
        Ok(())
    }

    fn get_collection_kind(
        &mut self,
        key: String,
    ) -> wasmtime::Result<Option<wit_storage::CollectionKind>> {
        self.require_storage(STORAGE_READ_OWN_CAPABILITY)?;
        let key = sdk_storage_key(key)?;
        Ok(match self.principal_storage.get(&key) {
            Some(PrincipalStorageValue::Array(_)) => Some(wit_storage::CollectionKind::Array),
            Some(PrincipalStorageValue::Map(_)) => {
                Some(wit_storage::CollectionKind::AssociativeMap)
            }
            Some(PrincipalStorageValue::Set(_)) => Some(wit_storage::CollectionKind::Set),
            _ => None,
        })
    }

    fn collection_len(&mut self, key: String) -> wasmtime::Result<Option<u32>> {
        self.require_storage(STORAGE_READ_OWN_CAPABILITY)?;
        let key = sdk_storage_key(key)?;
        let length = match self.principal_storage.get(&key) {
            Some(PrincipalStorageValue::Array(values)) => values.len(),
            Some(PrincipalStorageValue::Map(values)) => values.len(),
            Some(PrincipalStorageValue::Set(values)) => values.len(),
            _ => return Ok(None),
        };
        Ok(Some(u32::try_from(length).expect(
            "storage collection length is bounded below u32::MAX",
        )))
    }

    fn array_get(
        &mut self,
        key: String,
        index: u32,
    ) -> wasmtime::Result<Option<wit_storage::Value>> {
        self.require_storage(STORAGE_READ_OWN_CAPABILITY)?;
        let key = sdk_storage_key(key)?;
        match self.principal_storage.get(&key) {
            Some(PrincipalStorageValue::Array(values)) => {
                Ok(values.get(index as usize).map(wit_storage_value_ref))
            }
            Some(_) => wasmtime::bail!("storage key {key} is not an array"),
            None => Ok(None),
        }
    }

    fn queue_array_push(&mut self, key: String, value: wit_storage::Value) -> wasmtime::Result<()> {
        self.require_storage_write()?;
        self.pending_commands.push(HostCommand::PrincipalStorage(
            PrincipalStorageCommand::ArrayPush {
                key: sdk_storage_key(key)?,
                value: sdk_storage_value(value),
            },
        ));
        Ok(())
    }

    fn queue_array_set(
        &mut self,
        key: String,
        index: u32,
        value: wit_storage::Value,
    ) -> wasmtime::Result<()> {
        self.require_storage_write()?;
        self.pending_commands.push(HostCommand::PrincipalStorage(
            PrincipalStorageCommand::ArraySet {
                key: sdk_storage_key(key)?,
                index,
                value: sdk_storage_value(value),
            },
        ));
        Ok(())
    }

    fn queue_array_remove(&mut self, key: String, index: u32) -> wasmtime::Result<()> {
        self.require_storage_write()?;
        self.pending_commands.push(HostCommand::PrincipalStorage(
            PrincipalStorageCommand::ArrayRemove {
                key: sdk_storage_key(key)?,
                index,
            },
        ));
        Ok(())
    }

    fn map_get(
        &mut self,
        key: String,
        entry: String,
    ) -> wasmtime::Result<Option<wit_storage::Value>> {
        self.require_storage(STORAGE_READ_OWN_CAPABILITY)?;
        let key = sdk_storage_key(key)?;
        match self.principal_storage.get(&key) {
            Some(PrincipalStorageValue::Map(values)) => {
                Ok(values.get(&entry).map(wit_storage_value_ref))
            }
            Some(_) => wasmtime::bail!("storage key {key} is not a map"),
            None => Ok(None),
        }
    }

    fn queue_map_set(
        &mut self,
        key: String,
        entry: String,
        value: wit_storage::Value,
    ) -> wasmtime::Result<()> {
        self.require_storage_write()?;
        self.pending_commands.push(HostCommand::PrincipalStorage(
            PrincipalStorageCommand::MapSet {
                key: sdk_storage_key(key)?,
                entry,
                value: sdk_storage_value(value),
            },
        ));
        Ok(())
    }

    fn queue_map_delete(&mut self, key: String, entry: String) -> wasmtime::Result<()> {
        self.require_storage_write()?;
        self.pending_commands.push(HostCommand::PrincipalStorage(
            PrincipalStorageCommand::MapDelete {
                key: sdk_storage_key(key)?,
                entry,
            },
        ));
        Ok(())
    }

    fn set_contains(&mut self, key: String, value: wit_storage::Value) -> wasmtime::Result<bool> {
        self.require_storage(STORAGE_READ_OWN_CAPABILITY)?;
        let key = sdk_storage_key(key)?;
        let value = sdk_storage_value(value);
        match self.principal_storage.get(&key) {
            Some(PrincipalStorageValue::Set(values)) => Ok(values.contains(&value)),
            Some(_) => wasmtime::bail!("storage key {key} is not a set"),
            None => Ok(false),
        }
    }

    fn queue_set_insert(&mut self, key: String, value: wit_storage::Value) -> wasmtime::Result<()> {
        self.require_storage_write()?;
        self.pending_commands.push(HostCommand::PrincipalStorage(
            PrincipalStorageCommand::SetInsert {
                key: sdk_storage_key(key)?,
                value: sdk_storage_value(value),
            },
        ));
        Ok(())
    }

    fn queue_set_remove(&mut self, key: String, value: wit_storage::Value) -> wasmtime::Result<()> {
        self.require_storage_write()?;
        self.pending_commands.push(HostCommand::PrincipalStorage(
            PrincipalStorageCommand::SetRemove {
                key: sdk_storage_key(key)?,
                value: sdk_storage_value(value),
            },
        ));
        Ok(())
    }
}
