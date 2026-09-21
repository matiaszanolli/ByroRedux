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

## Minimal AgX (Benjamin Wrensch / IOLITE engine)

The AgX display transform in `crates/renderer/shaders/presentation.frag`
(and its host mirror, `crates/renderer/src/tonemap.rs`) is transcribed from
the "Minimal AgX Implementation" © 2023 Benjamin Wrensch
(<https://iolite-engine.com/blog_posts/minimal_agx_implementation>), itself
a compact port of Troy Sobotka's AgX display transform. Licensed under the
MIT License:

> Permission is hereby granted, free of charge, to any person obtaining a
> copy of this software and associated documentation files (the "Software"),
> to deal in the Software without restriction, including without limitation
> the rights to use, copy, modify, merge, publish, distribute, sublicense,
> and/or sell copies of the Software, and to permit persons to whom the
> Software is furnished to do so, subject to the following conditions: The
> above copyright notice and this permission notice shall be included in
> all copies or substantial portions of the Software.
>
> THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
> IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
> FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL
> THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
> LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING
> FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
> DEALINGS IN THE SOFTWARE.

The transcription is behaviour-preserving (tables and coefficients verbatim;
output convention adjusted to the linear-output/`B8G8R8A8_SRGB`-swapchain
pairing documented in the shader header).
