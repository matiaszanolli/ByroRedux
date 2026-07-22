# FidelityFX SDK provenance

This directory contains a curated, engine-side subset of AMD FidelityFX SDK
tag `v1.1.4`. That SDK release exposes the decoupled FFX API and reports the
Vulkan FSR 3 upscaler provider as version `3.1.4`.

- Upstream: <https://github.com/GPUOpen-LibrariesAndSDKs/FidelityFX-SDK>
- Tag: `v1.1.4`
- Source archive SHA-256:
  `25c46398a656150397597f78d44bf7cb445e9e177dd116b309bbfdf50d50cc9f`
- License: AMD MIT; see [`LICENSE.txt`](LICENSE.txt)

## Curated scope

The checked-in source is limited to the decoupled FFX API, the FSR 3 upscaler
provider/component, the common host/GPU headers they require, the shared shader
blob support, and the Vulkan backend. The provider registry in
`crates/fsr3-sys/native/upscaler_provider_registry.cpp` registers only the FSR 3
upscaler.

FSR 2, optical flow, frame interpolation, and frame-generation providers and
shader blobs are not compiled or shipped. A few shared API/resource headers
retain frame-generation or FSR 2 declarations because upstream FSR 3 headers
include them directly. The one frame-generation-named function in
`upscaler_only_stubs.cpp` is a rejecting ABI stub required by the shared Vulkan
interface; it does not contain or invoke frame-generation code.

`generated-vk/` contains 200 headers produced by 40 invocations of the official
`FidelityFX_SC.exe`: ten FSR 3 upscaler passes, each compiled as FP32/FP16 and
wave/wave64 variants. The regeneration script forces one compiler worker so
the deduplicated permutation-table ordering is byte-for-byte reproducible.
Regenerate them from an unpacked official `v1.1.4` archive with:

```text
scripts/generate-fsr3-vulkan-shaders.sh /path/to/FidelityFX-SDK-1.1.4
```

The compiler binary is not redistributed here.

## ByroRedux portability patches

The upstream source is preserved except for these narrowly scoped deltas:

1. Linux case-forwarding headers map upstream's mixed-case `FidelityFx/host`
   includes to canonical `FidelityFX/host` paths.
2. `crates/fsr3-sys/native/byro_ffx_portability.h` supplies the secure-CRT
   helpers absent from libc and MinGW. The Linux build uses the SDK's
   Windows-compatible 16-bit `wchar_t` ABI.
3. `ffx_provider.h` includes the standard allocation, placement-new, and string
   headers that MSVC supplies transitively but MinGW requires explicitly.
4. `ffx_provider_fsr3upscale.cpp` uses a portable unsigned 64-bit literal suffix
   and aligns the opaque upscaler context storage to 64 bytes; the private
   context stored there contains pointer-aligned fields.
5. `ffx_vk.cpp` aligns each array carved from backend scratch storage to 64
   bytes, covering the backend's explicitly aligned effect contexts.
6. `ffx_vk.cpp` retains a host-visible, device-local Vulkan memory type as a
   fallback for UMA/software devices instead of rejecting every valid type.
7. `ffx_vk.cpp` converts the SDK's ASCII shader binding labels directly instead
   of using libstdc++ wide-string conversion with the forced 16-bit ABI.
8. The native wrapper supplies 64-byte-aligned FFX allocation callbacks.
9. The upscaler-only ABI stub rejects the optional Vulkan frame-generation
   callback rather than linking the frame-interpolation swapchain backend.

Every delta is marked either `ByroRedux portability patch` in upstream-derived
source or lives under `crates/fsr3-sys/native`. Re-audit all nine items when
updating the pinned SDK.
