//! `world_spatial` WIT host interface.

use crate::runtime::*;

impl world_spatial::Host for HostState {
    fn nearby(
        &mut self,
        x: f32,
        y: f32,
        z: f32,
        radius: f32,
        limit: u32,
    ) -> wasmtime::Result<world_spatial::SpatialQueryResult> {
        self.require_world_spatial_read()?;
        if !self.accepting_commands {
            wasmtime::bail!("spatial queries are only available during a callback");
        }
        let result = self
            .spatial_snapshot
            .nearby([x, y, z], radius, limit as usize)
            .map_err(|error| wasmtime::Error::msg(error.to_string()))?;
        Ok(wit_spatial_query_result(&result))
    }
}
