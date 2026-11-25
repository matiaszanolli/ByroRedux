# Vendored FFmpeg `wmav2`

[FFmpeg 9.1 “Gauss”](https://ffmpeg.org/download.html#release_9.1), upstream
tag `n9.1`, released 2026-11-20. The
[release tarball](https://ffmpeg.org/releases/ffmpeg-9.1.tar.xz) has SHA-256
`1a4d394ae63d190a526cd052eb4415c156b5c5e2df37efab41a7de0b8fde2433`.

## Layout and patches

The release tarball is **outside the repository**. The
[workspace build script](../../crates/xwma/build.rs) downloads the tarball above, checks its
SHA-256, extracts it under Cargo's `OUT_DIR`, applies
[`wmav2-stream-frames.patch`](wmav2-stream-frames.patch), builds it, keeps
`libavcodec.a` and `libavutil.a`, and
deletes the extracted source and every object file. Only the verified tarball stays, in
`target/<profile>/xwma-ffmpeg-cache/`, so a rebuild does not download again; `cargo clean` removes it.
`XWMA_FFMPEG_TARBALL` points the build at a local copy for offline builds and `XWMA_FFMPEG_CACHE`
moves the cache.

The patch yields one codec frame per call while retaining the compressed superframe and
its bit cursor. Upstream allocates PCM for the entire superframe, up to 30,720 frames.
The patch also clears saved overlap, exponent history and the noise cursor on flush so
replay from the start produces the original PCM. It modifies two existing paths in
the V7 [file inventory](licence-files.tsv) and follows their LGPL notices.
The build runs `configure` and GNU Make out of tree, then links the two archives
statically. This is the Linux configure line verified in
[V7](../../docs/engine/xwma-phase0-findings.md#v7--static-wmav2-build-and-corpus-comparison).

```sh
./configure --disable-everything --enable-decoder=wmav2 --disable-asm --disable-faan --enable-static --disable-shared --disable-programs --disable-doc --disable-autodetect --disable-iconv --disable-network --disable-avdevice --disable-avformat --disable-avfilter --disable-swscale --disable-swresample
make
```

For a manual out-of-tree build, run the source tree's `configure` by its
absolute path from an empty build directory. `wmav2` is the sole registered
codec. The archives also contain its supporting code. Linux requires a C
toolchain, binutils, a POSIX shell, GNU Make, `patch` and standard shell utilities.
The macOS recipe uses Xcode's C toolchain and SDK with GNU Make. The
Windows MSVC recipe uses an MSYS2 shell and GNU Make with the Visual
Studio compiler, linker, librarian and SDK, and adds `--toolchain=msvc`.
The Windows and macOS builds still need validation.

## Licence and compiled sources

The FFmpeg libraries built by this recipe are **LGPL-2.1-or-later**.
See [FFmpeg's licence notice](https://github.com/FFmpeg/FFmpeg/blob/n9.1/LICENSE.md) and the
[LGPL 2.1 text](https://github.com/FFmpeg/FFmpeg/blob/n9.1/COPYING.LGPLv2.1). FFmpeg contributors retain
copyright as stated in the source headers. The [V7 file inventory](licence-files.tsv)
covers the translation units, included templates and headers used on Linux
x86-64. Its SHA-256 is
`a80e6f29119c4fc35c38f22ab077abe7ac48d2d14c221c4ca1fbb93e281ea03b`.
The source preserves the BSD and SHA-2 notices in `fixed_dsp.c`,
`fixed_dsp.h`, `uuid.c`, `sha.c` and `sha512.c`, the MIT notice in
`avsscanf.c`, and the Zlib notice in `adler32.c`. The inventory includes
no GPL, LGPLv3-only or nonfree code. The inventory and original headers
together identify the terms for each compiled file.

## Relinking a distributed static build

Ship this notice, FFmpeg's licence notice and the LGPL 2.1 text with any
binary linked against these archives. Supply the corresponding source for
that binary, including the complete FFmpeg tree with any modifications,
the application source, `Cargo.lock`, and the build scripts and settings
used for that release. Include the source with the distribution or make it
available alongside the binary. See [FFmpeg's distribution guidance](https://ffmpeg.org/legal.html)
and [LGPL 2.1 §6](https://github.com/FFmpeg/FFmpeg/blob/n9.1/COPYING.LGPLv2.1#L271).

To relink, unpack the pinned tarball and apply the patch in the source tree
with `patch -p1 < /path/to/wmav2-stream-frames.patch`. Make your changes, set
`XWMA_FFMPEG_SOURCE` to the unpacked directory (this bypasses the download and the digest
check, and the build then rebuilds on every run and leaves that directory untouched),
and rebuild the release package and feature set with the same C tools and
Rust toolchain. Preserve the FFmpeg interfaces the Rust wrapper calls.
Use a fresh target directory to regenerate both archives and link them
into the rebuilt executable. For example, to relink the Slice A smoke
test from the workspace root on Linux:

```sh
relink_cargo=$(rustup which --toolchain 1.100.0 cargo)
export PATH="$(dirname "$relink_cargo"):$PATH"
"$relink_cargo" test --locked --release --target-dir target/relink -p byroredux-xwma
```

For a release application, use `cargo build --locked --release` with the
distributed binary's package, target and feature flags in place of the test
command. Verify the rebuilt executable and relink recipe for each release
target before distributing it.

## Moving to a new FFmpeg release

1. Verify the new upstream tarball's SHA-256. Update the release, URL and
   digest constants in `crates/xwma/build.rs`. Apply any required patches
   and document them here.
2. Run the same configure recipe and audit the compilation commands and
   dependency files. Update the file inventory, its digest, and the notices
   for the sources and headers compiled by the new release.
3. Run the crate tests, verify the linked executable has no dynamic FFmpeg
   dependencies, and repeat the corpus and relink checks before release.
