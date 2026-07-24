# Third-party notices

## AMD FidelityFX SDK

ByroRedux vendors a curated source and generated-shader subset of AMD
FidelityFX SDK `v1.1.4` for the FSR 3.1.4 Vulkan upscaler-only integration.
FidelityFX SDK is licensed under the MIT License.

**The vendored copy is modified.** It carries nine portability deltas (Linux /
MinGW build support) and one correctness patch (a storage-image format that
upstream declares inconsistently between its C++ and its GLSL). Each is
itemized in `UPSTREAM.md`, and all of them must be re-audited when the pinned
SDK version changes.

The complete license text and source provenance are available at:

- [`third_party/fidelityfx-sdk-v1.1.4/LICENSE.txt`](third_party/fidelityfx-sdk-v1.1.4/LICENSE.txt)
- [`third_party/fidelityfx-sdk-v1.1.4/UPSTREAM.md`](third_party/fidelityfx-sdk-v1.1.4/UPSTREAM.md)
