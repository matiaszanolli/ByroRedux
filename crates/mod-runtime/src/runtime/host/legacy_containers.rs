//! `legacy_containers` WIT host interface.

use crate::runtime::*;

impl wit_legacy_containers::Host for HostState {
    fn array_create(&mut self) -> wasmtime::Result<i32> {
        self.require_legacy_container_write()?;
        Ok(self.legacy_containers.create_array())
    }

    fn map_create(&mut self) -> wasmtime::Result<i32> {
        self.require_legacy_container_write()?;
        Ok(self.legacy_containers.create_map())
    }

    fn count(&mut self, handle: i32) -> wasmtime::Result<i32> {
        self.require_legacy_container_read()?;
        Ok(self.legacy_containers.count(handle))
    }

    fn clear(&mut self, handle: i32) -> wasmtime::Result<bool> {
        self.require_legacy_container_write()?;
        Ok(self.legacy_containers.clear(handle))
    }

    fn release(&mut self, handle: i32) -> wasmtime::Result<bool> {
        self.require_legacy_container_write()?;
        Ok(self.legacy_containers.release(handle))
    }

    fn array_add(
        &mut self,
        handle: i32,
        value: wit_legacy_containers::Value,
        index: Option<u32>,
    ) -> wasmtime::Result<bool> {
        self.require_legacy_container_write()?;
        let value = sdk_legacy_container_value(value);
        Ok(self.legacy_containers.array_add(handle, value, index))
    }

    fn array_get(
        &mut self,
        handle: i32,
        index: i32,
    ) -> wasmtime::Result<Option<wit_legacy_containers::Value>> {
        self.require_legacy_container_read()?;
        Ok(self
            .legacy_containers
            .array_get(handle, index)
            .map(wit_legacy_container_value))
    }

    fn array_set(
        &mut self,
        handle: i32,
        index: i32,
        value: wit_legacy_containers::Value,
    ) -> wasmtime::Result<bool> {
        self.require_legacy_container_write()?;
        let value = sdk_legacy_container_value(value);
        Ok(self.legacy_containers.array_set(handle, index, value))
    }

    fn array_erase(&mut self, handle: i32, index: i32) -> wasmtime::Result<bool> {
        self.require_legacy_container_write()?;
        Ok(self.legacy_containers.array_erase(handle, index))
    }

    fn map_get(
        &mut self,
        handle: i32,
        key: String,
    ) -> wasmtime::Result<Option<wit_legacy_containers::Value>> {
        self.require_legacy_container_read()?;
        Ok(self
            .legacy_containers
            .map_get(handle, &key)
            .map(wit_legacy_container_value))
    }

    fn map_has_key(&mut self, handle: i32, key: String) -> wasmtime::Result<bool> {
        self.require_legacy_container_read()?;
        Ok(self.legacy_containers.map_has_key(handle, &key))
    }

    fn map_set(
        &mut self,
        handle: i32,
        key: String,
        value: wit_legacy_containers::Value,
    ) -> wasmtime::Result<bool> {
        self.require_legacy_container_write()?;
        let value = sdk_legacy_container_value(value);
        Ok(self.legacy_containers.map_set(handle, key, value))
    }

    fn map_remove(&mut self, handle: i32, key: String) -> wasmtime::Result<bool> {
        self.require_legacy_container_write()?;
        Ok(self.legacy_containers.map_remove(handle, &key))
    }
}
