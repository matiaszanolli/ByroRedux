# xWMA Audio Decoding — Phase 0 findings

Evidence collected on 2026-11-24 for [PRD §9](../prd/xwma-audio.md#9-delivery-sequence).
The [proposed decoder design](xwma-decoding.md) records the streaming
bound and the provenance and acceptance rules. The vanilla probes used local
installs. The results below contain metadata, record IDs and digests.

| Item | Result |
|---|---|
| V1 | Verified the RIFF form, chunks, tag and cumulative byte semantics. Microsoft documents the API. The games' use of that API remains unverified. |
| V2 | Negative search result. The public descriptions examined do not provide enough information to implement the WMA v2 decoder under the proposed provenance rules. |
| V3 | The installed FFmpeg `wmav2` is a candidate oracle. The probe supports a provisional tolerance of one signed 16-bit PCM count. One intact fixture exposes a frame-count disagreement. |
| V4 | Verified palette separators and loop units in both titles. The zero-count sentinel and exact Fallout 4 runtime scheduling remain unverified. |
| V5 | Verified the authored inputs for both titles, including Location music. The overlap order and Fallout 4's final fallback remain unverified. |
| V6 | Zero `.xwm` entries in all 129 installed Starfield BA2s surveyed. |

## V1 — RIFF xWMA, format tag and packet table

- Microsoft's XAudio2 loading example identifies the FourCCs `RIFF`, `XWMA`,
  `fmt `, `data` and `dpds` (lines 45–51). Microsoft's DirectXTK reference
  reader accepts the `XWMA` RIFF form, reads the format and payload, and
  looks up the xWMA `dpds` table for the WMA tags (lines 166–193, 229–230,
  303–317 and 589–600). It also recognizes WMA v3. The PRD's vanilla corpus
  and scope select v2. [XAudio2 file loading][xaudio-loading].
  [DirectXTK WAVFileReader.cpp][directxtk].
- Microsoft's subtype table identifies the `mmreg.h` constant as
  “WAVE_FORMAT_WMAUDIO2 (0x0161)”. The Windows SDK header excerpt, printed
  page 51, shows the corresponding `#define`. The decoder documentation
  names the format Windows Media Audio Standard. [Audio Subtype GUIDs][subtypes].
  [Windows SDK mmreg.h excerpt][mmreg]. [Windows Media Audio Decoder][wma-decoder].
- `XAUDIO2_BUFFER_WMA::pDecodedPacketCumulativeBytes` has one entry for each
  packet. Microsoft describes each entry as “the number of bytes accumulated
  after the corresponding xWMA packet is decoded in order” (lines 43–49).
  DirectXTK reads the table as `uint32_t` elements (lines 420–470). These are
  decoded byte totals. For the tested 16-bit xWMA files, dividing the terminal
  total by `2 × channels` gives the declared PCM frames. Consecutive entries
  give each packet's contribution to that declaration. V3 shows why to
  compare the declaration to actual output. [XAUDIO2_BUFFER_WMA][buffer-wma].
  [DirectXTK WAVFileReader.cpp][directxtk].
- Microsoft documents XAudio2 support for the format. That evidence alone
  cannot verify the Skyrim or Fallout 4 executable's playback path.
  [XAudio2 file loading][xaudio-loading]. [XAUDIO2_BUFFER_WMA][buffer-wma].

## V2 — Public WMA v2 descriptions

The search covered Microsoft's WMA Standard and XAudio2 documentation, public
codec notes, Microsoft patents and research papers, and projects describing
their source provenance. Microsoft documents codec tags, stream setup and
interfaces. Its scalable audio patent describes transform-coefficient
coding, including run-length coding and Huffman or Tunstall coding, under
“Run-Length Encoding of Transform Coefficients.” These describe techniques
and possible implementations. A usable v2 spec also needs the exact
configuration flags, frame and reservoir syntax, codebooks and run/level
mapping, and the reconstruction and overlap rules that apply to the shipped
streams. [Windows Media Audio Decoder][wma-decoder].
[Microsoft patent US6115689, description][malvar-patent].
[Microsoft patent US20030193900, description][wma-patent].

The public codec note indexes `0x0161` and sample files. The public OxideAV
research summary describes a patent-based trace and explicitly identifies
bitstream gaps. WaxFlow's author identifies FFmpeg as the primary source for
its WMA codebooks and run/level tables in its third-party notices. Those
implementations provide leads about missing documentation. Their code and
tables were excluded from this investigation as sources for implementing
the proposed decoder. [MultimediaWiki codec note][multimedia-wma].
[OxideAV research summary][oxideav]. [WaxFlow third-party notices][waxflow].

**Result.** No standalone public description sufficient for the required
vanilla WMA v2 corpus was found. This is a negative result for the search
performed and triggers the research condition in Q1. It leaves room for
new public documentation or independently documented black-box experiments.

## V2 addendum — existing decoders for Alternative B

Added 2026-11-24, after the owner chose Alternative B in Q1 (link an existing WMA
decoder). Candidates were found with `cargo search wma`, `cargo search xwma`
and `cargo info`, and the first candidate's published source was read, not built.

| Candidate | Licence | What it is | Verdict |
|---|---|---|---|
| FFmpeg `libavcodec`, `wmav2` | LGPL-2.1-or-later for the library as a whole; the licence of the files a `wmav2`-only build compiles is V7's to confirm | A mature C decoder. The installed `libavcodec` here is 62.28.102, and the 8.1.2 `ffmpeg` command is the V3 oracle. | **Chosen** (Q1): vendor a pinned release, link statically. |
| `oxideav-wma` 0.0.5 (crates.io, 2026-10-23) | MIT | Pure Rust, `WMA v1 + v2 baseline`. | Set aside. Its README says the tables were "extracted as bytes from the vendor WMA Standard decoder module's own PE data sections" and the frame layout "traced statically from the vendor decoder's bit-parse call graph", and its checks ran against "the sandboxed vendor decoder". The decoder is derived from Microsoft's binary, which the MIT label does not clear. It also states "clean-room rebuild in progress", validates against six vendor streams, and lists open gaps (two noise-substitution rows "the black-box reference contradicts"). |
| `rockbox-codecs` 0.2.2 (feature `wma`) | GPL-2.0-or-later | Rockbox's decoders as a library. | Excluded: it cannot be linked into this repository's MIT binary. |
| Symphonia 0.6 | MPL-2.0 | The engine's Rust audio stack. | No WMA codec (`crates/audio/src/fuz.rs`; its README lists none). |

FFmpeg's wmav2 reads the `fmt ` chunk's extradata when it has one and falls back to defaults
without it. The xWMA `fmt ` chunk has no extradata, and V7 measured the consequence: handed
packets of `block_align` bytes with no extradata, the decoder produced too few frames on
every input. The wrapper must supply the six setup bytes FFmpeg's own xWMA demuxer supplies
([V7](#v7--static-wmav2-build-and-corpus-comparison)).

## V3 — Candidate oracle and tolerance

The installed `/usr/bin/ffmpeg` reports version `8.1.2-2ubuntu2` and
`-decoders` lists `wmav2` as “Windows Media Audio 2.” Run it out of process
on a loose or extracted xWMA stream, stripping the FUZE wrapper first for
voice fixtures. This is the successful probe command, with an optional
`-threads 1` or `-cpuflags 0 -threads 1` before `-c:a wmav2`.

```sh
ffmpeg -hide_banner -nostdin -v error -xerror -c:a wmav2 -i input.xwm \
  -map 0:a:0 -c:a pcm_s16le -f s16le -
```

The 12-file probe covered four Fallout 4 effect channel/rate profiles, two
Fallout 4 music rates, three Skyrim music rates, a mono voice from each game
and each archive's longest music track. All 12 were identical between the
default and single-thread runs. Disabling CPU optimizations changed samples
in 11 files, with peak absolute difference **1** in interleaved signed 16-bit
PCM. Frame count stayed identical across all three FFmpeg runs. In 11 files
it matched the terminal `dpds` declaration. An intact Fallout 4 radio asset
had a shorter successful decode.

| Probe | Declared frames | FFmpeg frames | Scalar vs default peak |
|---|---:|---:|---:|
| Fallout 4 `sound\fx\mus\radio\mus_patriotic_presidential.xwm`, stereo, 44,100 Hz | 3,109,376 | 3,108,864 | 1 |
| Skyrim longest music track | 23,762,944 | 23,762,944 | 1 |
| Fallout 4 longest music track | 18,132,992 | 18,132,992 | 1 |

Use **≤ 1 signed 16-bit PCM count per sample** as the provisional
differential criterion, with exact channel count, rate, alignment and frame
count as separate checks. The measured tolerance describes two execution
paths in the same FFmpeg build. Agreement between independent decoders and
coverage of the full vanilla corpus still need measurement. FFmpeg's FATE
file uses a different metric for its `wmav2` encoder round trip and sets no
per-sample threshold for this xWMA comparison. The radio asset's 512-frame
gap needs independent adjudication before FFmpeg can be the sole oracle
for the entire corpus. Microsoft's Windows Media Audio Standard decoder
(`CLSID_CWMADecMediaObject`) is a candidate second implementation for that
check. [FFmpeg FATE wma.mak, lines 38–49][fate].
[Windows Media Audio Decoder][wma-decoder].

Follow the `binkaudio-clean` comparator's whole-file measurement and report
the number of differing samples and the peak absolute difference. Require
equal lengths before considering a file a pass and report the first mismatch
with separate length diagnostics. The precedent is
`/mnt/data/src/binkaudio-clean/src/bin/bacmp.rs:5–9,57–76`.

## V4 — Palette layout, loop data and Fallout 4 behavior

The xEdit copies used for file:line references are
`/mnt/data/tmp/xedit/wbDefinitionsTES5.pas` and
`/mnt/data/tmp/xedit/wbDefinitionsFO4.pas`. The requested
`/mnt/data/src/reference` tree contains the wiki dumps. The Pascal files
were found in the cached xEdit sources outside that tree. The upstream
links below identify the corresponding definitions.

| Topic | Source and result |
|---|---|
| Track kinds and layers | The Skyrim CK describes palette layers playing simultaneously with random track selection (`ck-uesp-wiki/(main)/Music Track.wiki:8–11`). UESP states “Form IDs of value zero are used to separate layers” (`uesp-wiki/Skyrim Mod/Mod File Format/MUST.wiki:30–31`). Both xEdit schemas read `MUST.SNAM` as a flat array of `MUST` or null FormIDs (TES5:10296–10315, FO4:12602–12620). The Fallout CK names the Palette and Layer controls (`falloutck-uesp-wiki/(main)/Music Track.wiki:14–18`). Preserve nulls through parsing and split the array at each null. [Skyrim CK Music Track][ck-track]. [UESP MUST][uesp-must]. [TES5 xEdit][xedit-tes5]. [FO4 xEdit][xedit-fo4]. [Fallout CK Music Track][fock-track]. |
| Loop fields | Both xEdit schemas define `LNAM` as two floats followed by a `u32` (TES5:10308–10312, FO4:12613–12617). Skyrim CK line 17 gives seconds for loop points and UESP lines 33–36 confirms both units. The Fallout CK labels “Loop Begins (secs)” and “Loop Ends (secs)” at lines 28–29. The count is a number of iterations. Neither CK page defines zero with a present loop range. [Skyrim CK Music Track][ck-track]. [Fallout CK Music Track][fock-track]. [UESP MUST][uesp-must]. |
| Music Type fields and timing | The Skyrim CK defines the five playback flags, priority, ducking and fade (Music Type lines 6–13). The Fallout CK lists the same controls plus Doesn't Queue (lines 10–18). xEdit agrees on the common bits, the `PNAM` fields and centi-dB storage, and `WNAM` fade (TES5:10194–10210, FO4:12499–12515). The Fallout CK page labels controls and gives no exact runtime transition schedule. UESP MUSC line 21 gives Skyrim priority direction: **1 is highest, 100 is lowest**. Vanilla battle versus ambient priorities corroborate the direction for the authored Fallout 4 data (battle 8, Commonwealth `MUSzExplore` 90). [Skyrim CK Music Type][ck-type]. [Fallout CK Music Type][fock-type]. [UESP MUSC][uesp-musc]. |

Vanilla checks walked `MUST` subrecords in Skyrim SE's base, Update and three
official DLC masters and Fallout 4's base and six gameplay DLC masters,
including compressed records. In `Skyrim.esm`, `MUSExploreSovngardeDayPalette`
(`000B9DCD`) has a 120-byte `SNAM` with 30 entries, including two nulls between
the A, B and C groups. In `Fallout4.esm`, `MUSExploreFoothillsSanctuaryDayPalette01`
(`002409EA`) has a 100-byte `SNAM` with 25 entries and the same two separators.
Silent `MUST` FormIDs inside a layer remain entries in that layer.

Skyrim shipped one `LNAM` in those masters (`MUSSovngardeChantLP`,
`0001713C`, 0.0 to 209.24 s, count 90). Fallout 4 had 27 (`Fallout4.esm`
23, `DLCRobot.esm` 4). One is `MUSSpecialChargenRunForTheVault01`
(`001A8675`), 0.001 to 59.899 s, count 50. None of the 28 loop records used
count zero. A controlled authoring and playback probe with a nonempty loop
range and count zero would distinguish no repeats from indefinite or other
behavior. Equivalent timed probes in the original Fallout 4 runtime would
establish transition timing and flag interactions.

## V5 — Location music sources and precedence

| Source | Skyrim | Fallout 4 |
|---|---|---|
| Cell | `CELL.XCMO → MUSC`. CK Common Data line 5 calls it “Default music type for this cell.” xEdit TES5:7499 and UESP CELL line 132 agree. [Skyrim CK Common Data][ck-cell]. [UESP CELL][uesp-cell]. | `CELL.XCMO → MUSC`, xEdit FO4:9816. Fallout CK Cell line 22 calls it the cell's default music type. [Fallout CK Cell][fock-cell]. |
| Location | `CELL.XLCN` or `WRLD.XLCN` identifies an `LCTN`. `LCTN.NAM1 → MUSC` and `PNAM → LCTN` are separate fields, xEdit TES5:7479, 9605–9606, 13785. UESP LCTN lines 53–55 and CK Location line 26 corroborate the controls. [UESP LCTN][uesp-lctn]. [Skyrim CK Location][ck-location]. | Same fields, xEdit FO4:9786, 11984–11985, 16195. Fallout CK Location lines 11 and 18 lists Parent Location and Music. [Fallout CK Location][fock-location]. |
| Region | `REGN.RDMO → MUSC` in the sound entry, xEdit TES5:13112–13142. CK Region line 78 ties playback to being inside the region. Lines 12–13 describe the override flag and priority for overlapping region data of the same type. UESP REGN line 37 confirms `RDMO`. `CELL.XCLR` lists region references (TES5:7478). [Skyrim CK Region][ck-region]. [UESP REGN][uesp-regn]. | Same fields in xEdit FO4:9785, 15573–15602. The vanilla records confirm authors use them. The Skyrim CK's override and tie rules do not establish Fallout 4's runtime region order. [FO4 xEdit][xedit-fo4]. |
| Worldspace | `WRLD.ZNAM → MUSC`, xEdit TES5:13860 and UESP WRLD line 106. CK World Spaces line 42 calls it the worldspace default. [Skyrim CK World Spaces][ck-world]. [UESP WRLD][uesp-wrld]. | `WRLD.ZNAM → MUSC`, xEdit FO4:16265. Fallout CK WorldSpace line 48 calls it the worldspace default. [Fallout CK WorldSpace][fock-world]. |
| Default objects | xEdit TES5:9767 names `DFMS` Default Music. `Skyrim.esm` `DOBJ` `00000031` maps it to `MUSExploreTestb` (`00089A9C`). `BTMS` and `DTMS` are separate battle and death requests. [TES5 xEdit][xedit-tes5]. | The FO4 xEdit DOBJ use list names `BTMS` and `DTMS` (FO4:12211, 12240). `Fallout4.esm` `DOBJ` `00000800` authors both and has no `DFMS` entry. Commonwealth's `ZNAM` points to `MUSzExplore` (`0001ED25`). That is an authored worldspace default. [FO4 xEdit][xedit-fo4]. |

The base-master scan found these authored fields. Counts count records
carrying the relevant subrecord.

| Master | `CELL.XCMO` | `LCTN.NAM1` | `REGN.RDMO` | `WRLD.ZNAM` |
|---|---:|---:|---:|---:|
| `Skyrim.esm` | 701 | 8 | 28 | 27 |
| `Fallout4.esm` | 281 | 67 | 14 | 4 |

The CK pages establish that a cell and a worldspace each have a default
and that Skyrim regions can request music and override other regions.
They leave cross-source precedence, parent-Location inheritance and the
ultimate Fallout 4 fallback unspecified. The vanilla records show concrete
conflicts. In Skyrim, `Mzulft01` (`0001523F`) points to `0005615B` in
`XCMO` and its `MzulftLocation` (`00019281`) points to `0002D4C2` in
`NAM1`. In Fallout 4, `Switchboard` (`000B42E4`) has `XCMO`
`000CF937` and its `SwitchboardLocation` (`000B477F`) has `NAM1`
`0002D4C2`. At `Vault111Ext` (`0000DD41`) the cell, Commonwealth and
two regions author distinct `MUSC` values. These records identify where
to test. Observing the selected track in the original runtime with one
distinct test track per source would settle the precedence and whether
sources compete, layer or fall back.

## V6 — Starfield archive inventory

From the repository root, the following command ran once per archive with
the path quoted. The survey included every top-level installed `.ba2` in
`/mnt/data/SteamLibrary/steamapps/common/Starfield/Data`, including the
base game, patch, installed add-ons and third-party archives. The example
lists member names and reports extensions case-insensitively
(`crates/bsa/examples/ba2_extensions.rs:14–29`).

```sh
for archive in "/mnt/data/SteamLibrary/steamapps/common/Starfield/Data"/*.ba2; do
  cargo run --release -p byroredux-bsa --example ba2_extensions -- "$archive"
done
```

| Archive set | Opened | Entries | `.bnk` | `.wem` | `.xwm` |
|---|---:|---:|---:|---:|---:|
| `Starfield - WwiseSounds01`–`05` and `WwiseSoundsPatch` | 6 | 30,462 | 33 | 30,410 | 0 |
| `Starfield - Voices01`, `Voices02`, `VoicesPatch` | 3 | 246,061 | 0 | 246,061 | 0 |
| All installed Starfield Data BA2s, including the preceding rows | 129 | 1,773,984 | 42 | 385,139 | 0 |

All 129 commands exited successfully. The result covers named archive
members in this installation. Embedded contents, loose files and other
installs are outside the survey.

## V7 — Static `wmav2` build and corpus comparison

**Negative result for empty extradata.** A scalar, static FFmpeg 9.1 build
decoded fewer frames than the installed V3 oracle on every input. With the
six-byte codec setup that FFmpeg's xWMA demuxer normally supplies, the same
harness matched the oracle on all 13 retained fixtures and 7,191 archive
entries to within one s16 count, with equal channel count, sample rate and
decoded frames. Both decoders produced fewer frames than `dpds` declared
for 82 Fallout 4 entries.

### Release and build

FFmpeg 9.1 “Gauss” (`n9.1`, released 2026-11-20) was the latest stable
release listed on 2026-11-24. The downloaded `ffmpeg-9.1.tar.xz` had SHA-256
`1a4d394ae63d190a526cd052eb4415c156b5c5e2df37efab41a7de0b8fde2433`.
The build used the untouched source. [FFmpeg downloads][v7-release].
[Release tarball][v7-tarball].

From the extracted source directory on Linux x86-64, with `TMPDIR` set to
`/mnt/data/tmp/xwma-v7`, the exact configure line and build were:

```sh
./configure --disable-everything --enable-decoder=wmav2 --disable-asm --disable-faan --enable-static --disable-shared --disable-programs --disable-doc --disable-autodetect --disable-iconv --disable-network --disable-avdevice --disable-avformat --disable-avfilter --disable-swscale --disable-swresample
make -j12 V=1
```

The build produced `libavcodec.a` (63.24.100) and `libavutil.a` (61.19.100).
The only registered decoder was `wmav2`, which selects `sinewin` and
`wma_freqs`. Common library code includes internal source and sink bitstream
filters. The build has zero programs, demuxers, muxers, parsers, encoders
and external FFmpeg library dependencies. `--disable-faan` removes the
default video DCT helpers. The libraries retain native pthread support for
shared initialization and independent contexts.

The verified Linux toolchain was GCC 15.3.0 (C17), glibc 2.44 development
headers and libraries, GNU Make 4.4.1, GNU binutils 2.47 (`as`, `ld`, `ar`,
`ranlib`, `nm`, `strip`), a POSIX shell, and standard shell utilities. A
second build with a restricted `PATH` succeeded with `awk`, `cat`, `chmod`,
`cmp`, `cp`, `cut`, `dirname`, `expr`, `grep`, `head`, `mkdir`, `mktemp`,
`rm`, `sed`, `sort`, `tail`, `touch`, `tr` and `uname` alongside those
tools. NASM was absent, so `--disable-asm` was also a build requirement.
The compiler still invokes the system assembler for its own C output.
Git, a C++ compiler, pkg-config, Perl and documentation tools were absent
from the restricted build's `PATH`. The generated link metadata lists
`-pthread -lm -latomic`. The V7 harness linked with `-lm -pthread` and its
dynamic dependencies were the host loader, `libc` and `libm`.

### Packet harness and corpus

The scratch work is in `/mnt/data/tmp/xwma-v7`. `harness.c` reads the RIFF
and validates the `fmt `, `dpds` and `data` bounds, format tag, 18-byte format
with zero `cbSize`, and packet-table shape. It initializes the codec from
the format fields and sends padded buffers of exactly `block_align` bytes
to `avcodec_send_packet`. It receives and drains frames, checks their
format, rate and channel count, and interleaves planar float PCM, scaling
by 32,768, saturating and rounding to s16le. `dpds` is measured independently
of output length. The comparison consumes PCM in process memory and records
only counts and digests. The harness links to the two static archives and
uses no demuxer in either run. It was built from the scratch directory:

```sh
cc -std=c11 -O2 -Wall -Wextra -Werror -Iffmpeg-9.1-minimal harness.c \
  ffmpeg-9.1-minimal/libavcodec/libavcodec.a ffmpeg-9.1-minimal/libavutil/libavutil.a \
  -lm -pthread -o harness-minimal
V7_HARNESS=harness-minimal V7_REPORTS=minimal python3 -u compare.py
```

The default run leaves `AVCodecContext.extradata` null. `--control` supplies
the demuxer's six bytes with `0x1f` at offset four and all other bytes zero.
In 9.1, `wmav2` initializes its
flags to zero with empty extradata. The demuxer's setup enables exponent VLC,
the bit reservoir and variable block lengths. The bitrate fields in these
inputs require none of the demuxer's bitrate substitutions.
[Pinned xWMA demuxer][v7-demuxer]. [Pinned WMA decoder][v7-decoder].

`compare.py` ran the [V3 invocation](#v3--candidate-oracle-and-tolerance)
with `/usr/bin/ffmpeg` `8.1.2-2ubuntu2` and checked stream shape using the
installed `ffprobe`. The retained V3 scratch directory and its `oracle.log`
contained 13 distinct inputs, including the two xWMA payloads already
stripped from FUZE. All 13 oracle PCM digests reproduced. The installed
oracle is a different release from the pinned library, as requested for V7.

`extract.rs` batches `list_files` and `extract` from the
[`bsa_extract_one`](../../crates/bsa/examples/bsa_extract_one.rs) and
[`ba2_extract_one`](../../crates/bsa/examples/ba2_extract_one.rs) examples,
reusing each open archive. The unmodified examples reproduced one batch
entry each byte for byte. The run used `BYROREDUX_SKYRIMSE_DATA` and
`BYROREDUX_FO4_DATA` with local Steam data roots as defaults because both
variables were unset. Entries with case-insensitive `.xwm` extensions were
sorted by archive member path. The archive SHA-256 digests are below.

| Archive | Entries tested | Archive SHA-256 |
|---|---:|---|
| `Skyrim - Sounds.bsa` | 269 | `c18244acb9500db0d6e695ca27e9fd535483c9c1a0ec0d9fa7800a00231fc3b4` |
| `Fallout4 - Sounds.ba2` | 6,922 | `e7d863b2ec8e795c7f9c6f75da2db4a2d889ef77a1be3fbf9fc4662254af9ffd` |

### Results

| Set | CLI frames | Empty extradata frames | Empty extradata decode errors | Empty extradata passes | Setup control passes | Setup control exact |
|---|---:|---:|---:|---:|---:|---:|
| V3 fixtures | 76,169,216 | 249,856 | 9 | 0 / 13 | 13 / 13 | 1 |
| Skyrim archive | 869,476,352 | 8,960,000 | 254 | 0 / 269 | 269 / 269 | 0 |
| Fallout 4 archive | 2,545,983,488 | 125,419,520 | 1,218 | 0 / 6,922 | 6,922 / 6,922 | 1,084 |

A pass requires both decoders to succeed, exact channels, rate and frames,
and peak difference at most one. Every RIFF parsed successfully. With
empty extradata, the peak absolute difference over the shared output
prefix reached 45,554 counts on the fixtures, 56,341 in Skyrim and 65,089
in Fallout 4. The CLI and setup control decoded every entry successfully.
In Fallout 4, 81 entries decoded 512 frames below the `dpds` declaration
and one decoded 960 frames below it. These include the V3 radio fixture,
which emitted 3,108,864 of 3,109,376 declared frames. The 960-frame case
emitted 6,244,352 of 6,245,312 declared frames. Independent adjudication of
the presentation-length rule under XWMA-DEC-002 remains open.

[Per-file results](xwma-v7-results.tsv) contain 7,204 rows, SHA-256
`c3ffa68f7fba49054c901cf63fc4dcee2f132d99fa5083099a6487a545480bb7`.
The numeric `group` is 0 for V3 fixtures (scratch filename order), 1 for
Skyrim and 2 for Fallout 4. `entry` is the zero-based index within each
group. The input SHA-256 identifies content, including repeated content at
different entries. `empty_` and `control_` identify the two harness runs.
`declared` is the final `dpds` byte count divided by bytes per PCM frame.
`packet` counts completed sends on success and gives the zero-based packet
index on a decode error. `rc` records the libavcodec result (`-1094995529`
is `AVERROR_INVALIDDATA`). Diff indices are zero-based interleaved sample
indices. `first_diff_sample` and `max_abs_diff` compare the common prefix,
and `first_missing_sample` records a length mismatch. A blank diff index
means none and a blank peak means the runs shared no samples.

The mono 44,100 Hz fixture with input SHA-256
`ea896d63258ba5ded40e558a0f6e6778e77f5bbe1c88e348025d7e0e6428595d`
was bit-identical at the default CPU flags, forced zero and forced
SSE2/AVX/AVX2 (`0xc010`). The empty-extradata runs each emitted 2,048 frames
with PCM SHA-256 `ad7facb2586fc6e966c004d7d1d16b024f5805ff7cb47c7a85dabd8b48892ca7`.
The control runs each emitted 16,384 frames, with oracle-identical PCM
SHA-256 `db90ff70e8ed3bd5589af80ce0eac838a29c82be925e9c0cc998ed54dbb6f80e`.
This verifies CPU flag invariance for the test binary and fixture on one
host. Cross-target floating-point reproducibility remains unmeasured.

### Compiled source licences

The `make V=1` compile commands and compiler `.d` dependencies account for
145 `.c` translation units, four textually included `.c` templates and 193
`.h` files from the FFmpeg tree. `licence-headers.txt` in scratch captures
each leading notice. The path-by-path classification is `licence-files.tsv`,
SHA-256 `a80e6f29119c4fc35c38f22ab077abe7ac48d2d14c221c4ca1fbb93e281ea03b`.
Of the 342 files, 333 carry LGPL-2.1-or-later notices, including BSD-3-Clause
notices in `fixed_dsp.c`, `fixed_dsp.h` and `uuid.c` and BSD SHA-2 provenance
in `sha.c` and `sha512.c`. `avsscanf.c` carries MIT terms, `adler32.c`
carries Zlib terms, and seven generated configuration, version or registry
files have no separate licence headers. No GPL, LGPLv3-only, nonfree or
Independent JPEG Group files appear in the final dependency inventory.
The combined libraries are **LGPL-2.1-or-later**, with the permissive
notices preserved. [Pinned FFmpeg licence][v7-licence].

### Vendoring and cross-platform builds

Follow [Ruffle's `PATCHES.md`](../../third_party/ruffle/PATCHES.md) with an
unpacked, pinned source tree under `third_party/ffmpeg/`. Keep the release
tag, upstream tarball URL and SHA-256, the exact configure recipe, licences
and notices, and the source audit manifest there. Record local layout and
patches, if any, and the re-vendoring steps in `PATCHES.md`. The tested
build needs zero upstream patches. Generate configuration and archives in
the build output directory from the checked-in source. Supply the matching
source and build scripts, licence and attribution, and the application
source or object material and instructions needed to relink against a
modified FFmpeg when distributing statically linked binaries. Verify that
relink procedure for each release target. [FFmpeg distribution guidance][v7-legal].
[Pinned LGPL-2.1 text, §6][v7-lgpl].

The [Cross-Platform Builds PRD](../prd/cross-platform-builds.md) would add
this native build to the hosted CI and release lanes. Linux and Steam Deck
use a C compiler and the shell/Make/binutils toolchain above. The Windows
MSVC lane needs a POSIX build shell with GNU Make, such as MSYS2, and the
Visual Studio compiler, linker, librarian and SDK with
`--toolchain=msvc`. The macOS arm64 lane, if enabled, needs Xcode's C
toolchain and SDK plus GNU Make. Keep `--disable-asm` on all targets.
The Windows and macOS recipes still need validation of the target ABI,
static link, compiled-file notices and PCM digests. Include a build and
fixture digest check in each hosted lane. The installed CLI and game data
are needed for the corpus test only. [FFmpeg platform build guide][v7-platform].

[v7-release]: https://ffmpeg.org/download.html#release_9.1
[v7-tarball]: https://ffmpeg.org/releases/ffmpeg-9.1.tar.xz
[v7-demuxer]: https://github.com/FFmpeg/FFmpeg/blob/n9.1/libavformat/xwma.c
[v7-decoder]: https://github.com/FFmpeg/FFmpeg/blob/n9.1/libavcodec/wmadec.c
[v7-licence]: https://github.com/FFmpeg/FFmpeg/blob/n9.1/LICENSE.md
[v7-lgpl]: https://github.com/FFmpeg/FFmpeg/blob/n9.1/COPYING.LGPLv2.1#L271
[v7-legal]: https://ffmpeg.org/legal.html
[v7-platform]: https://ffmpeg.org/platform.html
[xaudio-loading]: https://learn.microsoft.com/en-us/windows/win32/xaudio2/how-to--load-audio-data-files-in-xaudio2
[directxtk]: https://github.com/microsoft/DirectXTK/blob/main/Audio/WAVFileReader.cpp
[subtypes]: https://learn.microsoft.com/en-us/windows/win32/medfound/audio-subtype-guids
[mmreg]: https://www.mmsp.ece.mcgill.ca/Documents/AudioFormats/WAVE/Docs/Pages%20from%20mmreg.h.pdf
[wma-decoder]: https://learn.microsoft.com/en-us/windows/win32/medfound/windowsmediaaudiodecoder
[buffer-wma]: https://learn.microsoft.com/en-us/windows/win32/api/xaudio2/ns-xaudio2-xaudio2_buffer_wma
[malvar-patent]: https://patents.google.com/patent/US6115689A/en
[wma-patent]: https://patents.google.com/patent/US20030193900A1/en
[multimedia-wma]: https://wiki.multimedia.cx/index.php/Windows_Media_Audio
[oxideav]: https://github.com/OxideAV/oxideav-wma#readme
[waxflow]: https://github.com/ColeSpringer/WaxFlow/blob/main/THIRD-PARTY-NOTICES.md
[fate]: https://github.com/FFmpeg/FFmpeg/blob/master/tests/fate/wma.mak
[xedit-tes5]: https://github.com/TES5Edit/TES5Edit/blob/dev-4.1.6/Core/wbDefinitionsTES5.pas
[xedit-fo4]: https://github.com/TES5Edit/TES5Edit/blob/dev-4.1.6/Core/wbDefinitionsFO4.pas
[ck-track]: https://skyrimck.uesp.net/wiki/Music_Track
[ck-type]: https://skyrimck.uesp.net/wiki/Music_Type
[ck-cell]: https://skyrimck.uesp.net/wiki/Common_Data
[ck-location]: https://skyrimck.uesp.net/wiki/Location
[ck-region]: https://skyrimck.uesp.net/wiki/Region
[ck-world]: https://skyrimck.uesp.net/wiki/World_Spaces
[fock-track]: https://falloutck.uesp.net/wiki/Music_Track
[fock-type]: https://falloutck.uesp.net/wiki/Music_Type
[fock-cell]: https://falloutck.uesp.net/wiki/Cell
[fock-location]: https://falloutck.uesp.net/wiki/Location
[fock-world]: https://falloutck.uesp.net/wiki/WorldSpace
[uesp-must]: https://en.uesp.net/wiki/Skyrim_Mod:Mod_File_Format/MUST
[uesp-musc]: https://en.uesp.net/wiki/Skyrim_Mod:Mod_File_Format/MUSC
[uesp-cell]: https://en.uesp.net/wiki/Skyrim_Mod:Mod_File_Format/CELL
[uesp-lctn]: https://en.uesp.net/wiki/Skyrim_Mod:Mod_File_Format/LCTN
[uesp-regn]: https://en.uesp.net/wiki/Skyrim_Mod:Mod_File_Format/REGN
[uesp-wrld]: https://en.uesp.net/wiki/Skyrim_Mod:Mod_File_Format/WRLD
