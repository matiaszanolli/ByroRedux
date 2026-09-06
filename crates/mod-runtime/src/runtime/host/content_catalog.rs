//! `content_catalog` WIT host interface.

use crate::runtime::*;

impl content_catalog::Host for HostState {
    fn plugin_count(&mut self) -> wasmtime::Result<u32> {
        self.require_content_catalog_read()?;
        Ok(u32::try_from(self.content_catalog.len())
            .expect("content catalog is bounded below u32::MAX"))
    }

    fn plugin_at(&mut self, index: u32) -> wasmtime::Result<Option<content_catalog::PluginInfo>> {
        self.require_content_catalog_read()?;
        Ok(self.content_catalog.plugin(index).map(|plugin| {
            let source = plugin.source();
            content_catalog::PluginInfo {
                name: plugin.name().to_owned(),
                source_high: u64::from_be_bytes(
                    source[..8].try_into().expect("eight-byte source half"),
                ),
                source_low: u64::from_be_bytes(
                    source[8..].try_into().expect("eight-byte source half"),
                ),
                kind: match plugin.kind() {
                    PluginKind::Regular => content_catalog::PluginKind::Regular,
                    PluginKind::Light => content_catalog::PluginKind::Light,
                },
            }
        }))
    }

    fn find_plugin(&mut self, name: String) -> wasmtime::Result<Option<u32>> {
        self.require_content_catalog_read()?;
        validate_plugin_query(&name)?;
        Ok(self.content_catalog.find(&name).map(|(index, _)| index))
    }

    fn dependency_count(&mut self, plugin: u32) -> wasmtime::Result<Option<u32>> {
        self.require_content_catalog_read()?;
        Ok(self.content_catalog.plugin(plugin).map(|plugin| {
            u32::try_from(plugin.dependencies().len())
                .expect("content catalog is bounded below u32::MAX")
        }))
    }

    fn dependency_at(&mut self, plugin: u32, index: u32) -> wasmtime::Result<Option<u32>> {
        self.require_content_catalog_read()?;
        Ok(self.content_catalog.dependency(plugin, index))
    }

    fn get_record(
        &mut self,
        form: state::FormRef,
    ) -> wasmtime::Result<Option<content_catalog::RecordInfo>> {
        self.require_content_catalog_read()?;
        let mut source = [0_u8; 16];
        source[..8].copy_from_slice(&form.source_high.to_be_bytes());
        source[8..].copy_from_slice(&form.source_low.to_be_bytes());
        Ok(self
            .content_catalog
            .record(FormRef::new(source, form.local))
            .map(|record| content_catalog::RecordInfo {
                record_type: u32::from_be_bytes(record.record_type()),
            }))
    }

    fn qualify_form(
        &mut self,
        plugin: String,
        local: u32,
    ) -> wasmtime::Result<Option<state::FormRef>> {
        self.require_content_catalog_read()?;
        validate_plugin_query(&plugin)?;
        Ok(self
            .content_catalog
            .qualify_form(&plugin, local)
            .map(wit_form_ref))
    }
}
