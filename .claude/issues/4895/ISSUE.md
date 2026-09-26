# REN-D5-2026-09-26-17: Device selection probes extension presence, not the feature bits `create_logical_device` force-enables; `textureCompressionBC` is neither required nor consulted

**GitHub Issue**: https://github.com/matiaszanolli/ByroRedux/issues/4895

**Labels**: low,renderer,vulkan,bug

- **Severity**: LOW
- **Dimension**: Memory/Lifecycle (device init)
- **Location**: `crates/renderer/src/vulkan/device.rs` — `is_device_suitable`, `create_logical_device`, `DeviceCapabilities::texture_compression_bc`
- **Status**: NEW
- **Description / Evidence**: `is_device_suitable` requires the three RT extensions, `shaderInt64` and `synchronization2`. `create_logical_device` additionally force-enables `independentBlend`, `fragmentStoresAndAtomics`, Vulkan 1.2 `runtimeDescriptorArray`, `descriptorBindingPartiallyBound`, `descriptorBindingSampledImageUpdateAfterBind`, `shaderSampledImageArrayNonUniformIndexing` and `bufferDeviceAddress` without probing any bit. A device lacking one fails at `vkCreateDevice` instead of being skipped for the next candidate. `texture_compression_bc` is probed and enabled-if-supported but no consumer checks it, although every shipped Bethesda texture is BC1/3/5/7.
- **Impact**: On the RT-capable hardware class every bit is present, so this is asymmetry, not a live failure. It means no fallback to a second GPU, and BC is a misleading "optional" cap that is really mandatory.
- **Suggested Fix**: Make the force-enabled bits and BC hard requirements in `is_device_suitable` (one `get_physical_device_features2` chain already exists), so failure is a clean "No suitable GPU".

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (sibling constructors / upload paths / docs)
- [ ] **TESTS**: A regression test pins this specific fix
