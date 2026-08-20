# AUDIT — WATR Rain / Displacement Simulator Arbitration — 2026-08-20

**Scope.** Arbitration pass only. Two sibling audits in the 2026-08-20
comprehensive suite reached contradictory conclusions about
`crates/plugin/src/esm/records/misc/water.rs`. This report settles the conflict
offset-by-offset against shipped bytes and nothing else. No other subsystem was
audited. No `cargo` command was run. No source file was modified.

**Method / authority.** `find / -iname '*Records.pas'` returns **zero hits** —
there is no xEdit / niftools record definition on this disk, so *neither* claim's
"verified against xEdit `dev-4.1.6`" assertion could be re-checked at its stated
source. Authority therefore falls to tier 2 of the requested precedence order:
**real shipped bytes with recognisable GECK/CK default tuples pinning field
identity**, cross-validated across games and against total record size. All seven
vanilla masters were walked with a from-scratch Python GRUP/record/sub-record
reader (zlib-aware), reading every `WATR` in `Oblivion.esm`, `Fallout3.esm`,
`FalloutNV.esm`, `Skyrim.esm`, `Fallout4.esm`, `SeventySix.esm` and
`Starfield.esm` (23 / 53 / 78 / 34 / 42 / 47 / 15 records).

---

## Verdict

**Claim A (`/audit-legacy-compat` LC-D6-01) is correct. Claim B
(`/audit-fo4` FO4-D6-2026-08-20-02) is correct only about Oblivion — which
Claim A also got right — and is wrong on the contested point.** FO3/FNV and
Skyrim carry byte-identical simulator layouts to Oblivion (shifted −4) and to
FO4 (shifted 0); their decoders commit exactly the same Rain↔Displacement
`Starting Size` swap. Skyrim's `normal_magnitude` genuinely reads the
Displacement Simulator's starting size and is `0.05` on **34/34** vanilla
records, and that constant is folded into all three canonical noise amplitudes.

Both claims are additionally wrong about **FO76**: Claim A calls
`decode_dnam_fo76` correct, Claim B calls it correct-and-do-not-touch. FO76's
`DNAM` is a 148-byte Starfield-family layout, not an FO4 layout, and the FO76
decoder is misaligned across essentially its whole body. Both claims also missed
that the **primary** FO3/FNV path never reaches the simulator block at all.

---

## The two layout families (established from shipped bytes)

### Family 1 — Oblivion / FO3 / FNV / Skyrim: 5-float Rain then 5-float Displacement

The GECK/CK ships one canonical default tuple for both simulators. It appears
verbatim in three different games, which is what pins field identity:

```
Rain Simulator          Force  Velocity  Falloff  Dampner  StartingSize
                        0.1    0.6       0.985    2.0      0.01
Displacement Simulator  Force  Velocity  Falloff  Dampner  StartingSize
                        0.4    0.6       0.985    10.0     0.05
```

| Game | Rain block | Displacement block | Records carrying the tuple |
|---|---|---|---|
| Oblivion `DATA` (102 B) | 60 · 64 · 68 · **72** · **76** | 80 · 84 · 88 · 92 · **96** | 17/17 |
| FO3/FNV `DATA` (186 B) / `DNAM` (196 B) | 56 · 60 · 64 · 68 · **72** | 76 · 80 · 84 · 88 · **92** | 11/11 FO3, 8/8 FNV (long `DATA`); 41 FO3 / 69 FNV `DNAM` |
| Skyrim `DNAM` (228/232 B) | 56 · 60 · 64 · 68 · **72** | 76 · 80 · 84 · 88 · **92** | 34/34 |
| FO4 `DNAM` (201 B) | *(no rain simulator)* | 76 · 80 · 84 · 88 · **92** | 42/42 |

Independent corroboration for Oblivion — **exact size arithmetic**. xEdit's
documented TES4 `WATR.DATA` field list is 11 floats (44 B) + 3 RGBA (12 B, at
44/48/52) + 4 unused (56) + 5 rain floats (60–76) + 5 displacement floats
(80–96) + `u16` Damage (100) = **102 bytes**, which is precisely the size of
17/17 long vanilla Oblivion records. The 4 unused bytes at 56 read as garbage
(`199992`, `-4.3e8`, `3.5e-44`) in the dump, exactly as an uninitialised pad
should. This fixes every field position without needing the .pas file.

Independent corroboration for Skyrim — **cross-game tuple identity**. FO4's
displacement block is at 76/80/84/88/92 (Claim B verified this, and
`water.rs:1855-1859`'s own fixture labels those five offsets). Skyrim carries
*byte-identical authored tuples* in the same five slots:

```
FO4  ExtLakeQuannapowittWater  76..92 = 0.1  0.85  0.8  0.98   0.05
Skyrim DefaultWater            76..92 = 0.1  0.85  0.8  0.98   0.05
FO4  ExtBloodyWater            76..92 = 0.4  0.6   0.985 0.98  0.05
Skyrim DefaultMarshWater       76..92 = 0.4  0.6   0.985 3.7   0.05
```

Two authoring presets, both reproduced across the Skyrim↔FO4 boundary in the
same five slots. Skyrim `DNAM[92]` is the Displacement Starting Size. It is
`0.05` on 34/34 records; `DNAM[72]` (the Rain Starting Size) is the field that
actually varies (`0.7`×15, `0.0`×12, `0.01`×5, `1.0`, `0.1`).

Prefix corroboration: Skyrim `DNAM[52..56]` reads `0xCDCDCDCD` (`-4.31597e8`) on
**34/34** records — MSVC uninitialised-memory fill — confirming 52..56 is the
same 4-byte pad Oblivion has at 56..60, and hence that the rain block starts at
56.

### Family 2 — FO76 / Starfield

`SeventySix.esm` `DNAM` is 148 B and `Starfield.esm` `DNAM` is 152 B; they are
**the same layout**, Starfield merely appending one trailing float:

| offset | field | FO76 sample | Starfield sample |
|---|---|---|---|
| 0 | depth amount | 86 | 8 |
| 4/8/12 | float triple (absorption) | 0.0763 / 0.0104 / 0.0077 | 0.1656 / 0.0962 / 0.0763 |
| 16/20/24/28 | concentration quad | … | … |
| 32 | underwater colour (packed RGBA) | denormal | denormal |
| 36 | underwater fog amount | 1 | 1 / 0.8 |
| 40 / 44 | underwater near / far | −9000 / 850 | −150 / 75 |
| **48** | **normal magnitude** | 25 distinct / 47 | 0.5471 |
| 52/56/60 | normal falloff triple | 1 / 0.9 / 0.985 | 1 / 0.975 / 0.9979 |
| **64/68/72/76/80** | **displacement F/V/Fo/Dm/Start** | 0.1 / 0.85 / 0.8 / 0.97 / **0.05** | 0.4 / 0.5 / 0.975 / 1.0 / **0.05** |
| 84/88/92 | noise wind directions (deg) | 239.3 / 331.8 / 62.4 | 40.2 / 78.4 / 179.0 |
| 96/100/104 | noise wind speeds | 0.022 / 0.028 / 0.034 | 0.019 / 0.021 / 0.02 |
| 108/112/116 | noise amplitudes | … | … |
| 120/124/128 | noise UV scales | 335 / 223 / 112 | 72.1 / 39 / 13 |
| 132/136/140 | noise falloffs | 4096 | 100 |
| 144 | flow-map scale | 1 | 1 |
| 148 | roughness | *(absent)* | 0.08 |

FO76 `DNAM[80]` is `0.05` on **47/47**; FO76 `DNAM[48]` has 25 distinct values;
FO76 `DNAM[52]` is `1.0` on 47/47. FO4's colours at 4/8 are packed RGBA bytes
(`3a 35 21 00`), FO76's are floats — the two are *not* the same layout.

---

## Per-game verdict table

| Game | What the decoder reads for the simulator block | Authoritative layout | Authority used | Agree? |
|---|---|---|---|---|
| **Oblivion** (`decode_data_oblivion`, `:512-543`) | rain F/V/Fo/Dm ← 60/64/68/72 · `wave_amplitude`←80 · `wave_frequency`←84 · `displacement`←**[76, 88, 92]** · `rain_start_size`←**96** | rain 60/64/68/72/**76** · disp 80/84/88/92/**96** | shipped bytes (17/17 default tuple) + exact 102-byte size arithmetic | **NO** — the two `Starting Size` fields are swapped |
| **FO3 / FNV — long `DATA` (186 B)** (`decode_data_fo3nv`, `:589-608`) | rain F←56, V/Fo/Dm←60/64/68 · `wave_amplitude`←76 · `wave_frequency`←80 · `displacement`←**[72, 84, 88]** · `rain_start_size`←**92** · `normal_magnitude`←**96** | rain 56/60/64/68/**72** · disp 76/80/84/88/**92** | shipped bytes (FO3 11/11, FNV 8/8; identical tuple to Oblivion −4) | **NO** — same swap; plus `normal_magnitude`←96 is unfounded (see LOW-2) |
| **FO3 / FNV — `DNAM` (196 B), the majority path** (`decode_dnam_pre_fo4`, `:694-753`) | **nothing** — the function returns after byte 52 | rain 56…, disp 76… | shipped bytes; 42/53 FO3 and 70/78 FNV records take this path | **NO** — block never decoded at all |
| **Skyrim** (`apply_skyrim_dnam_tail`, `:760-892`) | rain F/V/Fo/Dm ← 56/60/64/68 · `wave_amplitude`←76 · `wave_frequency`←80 · `displacement`←**[72, 84, 88]** · `normal_magnitude`←**92** · `noise_falloff`←96 · `rain_start_size` **never assigned** | rain 56/60/64/68/**72** · disp 76/80/84/88/**92** | shipped bytes; byte-identical authored tuples shared with FO4 at 76..92; `0xCDCDCDCD` pad at 52 | **NO** — same swap, and `normal_magnitude` is the displacement starting size |
| **FO4** (`decode_dnam_fo4`, `:925-1055`) | `wave_amplitude`←76 · `wave_frequency`←80 · `displacement`←**[92, 84, 88]** · `normal_magnitude`←52 | disp 76/80/84/88/**92** · normal magnitude 52 | shipped bytes (42 records; `DNAM[52]` varies, 4/8 confirmed packed RGBA) | **YES — correct** |
| **FO76** (`decode_dnam_fo76`, `:1059-1155`) | `reflectivity`←64 · `fresnel`←68 · `wave_amplitude`←76 · `wave_frequency`←80 · `displacement`←**[92, 84, 88]** · `normal_magnitude`←52 | disp **64/68/72/76/80** · normal magnitude **48** · 84/88/92 = noise wind **directions in degrees** | shipped bytes (47 records); layout identical to Starfield over 0..144 | **NO** — wholesale misalignment; `displacement` receives 62°/239°/332° |
| **Starfield** (`decode_dnam_starfield`, `:1161-1266`) | `wave_amplitude`←64 · `wave_frequency`←68 · `displacement`←**[80, 72, 76]** · `normal_magnitude`←48 · dirs←84/88/92 | disp 64/68/72/76/**80** · normal magnitude 48 · dirs 84/88/92 | shipped bytes (15 records) | **YES — correct** |

---

## Sub-question 4 — what does Skyrim's `normal_magnitude` actually read, and is `0.05` authored there?

**It reads `DNAM[92]`, which is the Displacement Simulator's *Starting Size*, and
`0.05` is genuinely the byte value at that offset on 34/34 vanilla records —
because `0.05` is the CK's default displacement starting size, shipped unchanged
in Oblivion (`DATA[96]`, 17/17), FO3/FNV (`[92]`, 10/11 and 7/8), FO4 (`[92]`,
42/42) and FO76 (`[80]`, 47/47).** It is not a normal magnitude. Skyrim's `DNAM`
has no per-record field that behaves like FO4's `[52]` / FO76's `[48]` /
Starfield's `[48]` physical normal magnitude at any offset the decoder inspects;
`normal_magnitude` should be left at its neutral `1.0` sentinel until an offset
is byte-decoded and confirmed.

**Full chain, HEAD:**

1. `apply_skyrim_dnam_tail` (`water.rs:829-833`) loads
   `noise_amplitude_scales ← DNAM[184,188,192]`. Those are genuinely authored and
   genuinely varied — measured across all 34 records they span `0.0725 … 1.0`
   with 28+ distinct values (`DefaultWater` = 0.6957 / 0.6304 / 0.4746,
   `RiverWaterFlowSE` = 0.9275 / 0.9022 / 0.65, `PuddleWater` = 0.0833 / 0.0761 /
   0.163).
2. Same function, `water.rs:834-837`: `p.normal_magnitude = read_f32_at(92)` →
   `0.05` on every record.
3. `resolve_water_material` (`byroredux/src/env_translate.rs:777-784`) copies the
   authored amplitudes into `mat.noise_amplitude_scales`, clamped `[0.05, 4.0]`.
4. `env_translate.rs:815-823` clamps `normal_magnitude` to `[0.01, 8.0]` → stays
   `0.05` → **multiplies all three amplitudes by it**. `DefaultWater` becomes
   `0.0348 / 0.0315 / 0.0237`.
5. `byroredux/src/render/water.rs:279-284` packs them into `push.detail.yzw`.
6. `crates/renderer/shaders/water.frag:690-716` consumes them as
   `ampScale * max(push.detail.y, 0.05) * max(push.depth.z, 0.0)`, and
   `sampleScrollingNormal` applies the result at `:313`
   (`normalize(vec3(n.xy * ampScale, n.z))`) — i.e. it is the tangent-space tilt
   of every sampled water normal.

**Where the value ends up.** The shader's `max(…, 0.05)` floor is the last link.
Because every post-multiply amplitude lands in `[0.0025, 0.2]` and 33 of the 34
records land at or below `0.05`, the floor clamps them: **every vanilla Skyrim
water body renders with `detail.y = detail.z = detail.w = 0.05` — the shader
minimum — on every noise layer.** So Claim A's mechanism is confirmed and its
"~20× flatter" figure is the right order of magnitude (`RiverWaterFlowSE`
0.9275 → 0.05 = 18.6×; `DefaultWater` 0.6957 → 0.05 = 13.9×), but the precise
consequence is slightly different and slightly worse than Claim A stated: not a
uniform 20× attenuation, but a **total collapse of per-water authored variation
onto a single floor value**. Nothing downstream zeroes or rescues it; the
`0.05` really is authored at offset 92, and it is authored there as a
displacement starting size.

`rain_start_size` is never assigned on the Skyrim path, so
`WaterParams::rain_start_size` stays `0.0`, `resolve_water_material` skips it
(`> 0.0` gate), and `render/water.rs:303` ships the canonical default instead of
the authored `DNAM[72]` (which varies over `0.0 / 0.01 / 0.1 / 0.7 / 1.0`).

---

## Sub-question 5 — is there a test pinning `0.05`?

**Yes — and it pins an incorrect value.** `crates/plugin/src/esm/records/misc/water.rs:1788`:

```rust
assert_eq!(w.params.normal_magnitude, 0.05);
```

inside `parse_watr_decodes_dnam_skyrim_prefix`. The fixture that feeds it
(`:1731-1740`) is built from the real vanilla tuple:

```rust
data[56..60] … 0.2;    data[60..64] … 2.25;   data[64..68] … 0.5;   data[68..72] … 1.25;
data[72..76].copy_from_slice(&0.01f32.to_le_bytes());   // labelled nothing; is Rain Starting Size
data[76..80] … 0.4;    data[80..84] … 1.35;
data[84..88] … 0.985;  data[88..92] … 10.0;
data[92..96].copy_from_slice(&0.05f32.to_le_bytes());   // is Displacement Starting Size
```

and two asserts encode the swap: `:1780`
`assert_eq!(w.params.displacement, [0.01, 0.985, 10.0])` (should be
`[0.05, 0.985, 10.0]`) and `:1788` (should be `rain_start_size == 0.01`, with
`normal_magnitude` left at `1.0`).

**The file contradicts itself twenty lines later.** `parse_watr_decodes_fo4_visual_data_layout`
writes the *same five offsets with the same meaning* and labels them correctly
(`:1855-1859`):

```rust
data[76..80] … 0.4;   // displacement force
data[80..84] … 0.6;   // displacement velocity
data[84..88] … 0.985; // displacement falloff
data[88..92] … 10.0;  // displacement dampener
data[92..96] … 0.05;  // displacement starting size
```

and asserts `displacement == [0.05, 0.985, 10.0]` (`:1904`). One fixture shape,
two mutually exclusive readings of offset 92, in one file. The FO4 one is right.

Two sibling tests pin the same swap: `:1589-1590` (Oblivion —
`displacement == [0.08, 0.85, 3.5]` / `rain_start_size == 2.25`, with the
fixture comments at `:1574`/`:1579` labelling 76 "displacement start" and 96
"rain start", the exact inverse of the shipped bytes) and `:1645-1646`
(FO3/FNV, same inversion at 72/92). All three were written from the code's own
output, so no test in the tree can catch this.

---

## Findings provable at HEAD

### WATR-ARB-01 — Skyrim `normal_magnitude` reads the Displacement Starting Size, collapsing every authored noise amplitude onto the shader floor
- **Severity**: HIGH
- **Location**: `crates/plugin/src/esm/records/misc/water.rs:834-837`; consumed at `byroredux/src/env_translate.rs:815-823`; reaches `crates/renderer/shaders/water.frag:690-716,313`
- **Status**: NEW (no matching issue in `/tmp/audit/issues.json`; the closest water issues — #2782/#2784/#2787/#2789/#2790/#2804/#2870/#2872/#2887/#2888/#2889 — are all renderer- or physics-side)
- **Evidence**: `DNAM[92] == 0.05` on 34/34 vanilla `Skyrim.esm` records (1 distinct value); the same offset holds `0.05` on 42/42 FO4 records where the file's own FO4 decoder and fixture correctly name it *Displacement Starting Size*; authored amplitudes at 184/188/192 span 0.0725–1.0 across 28+ distinct values.
- **Impact**: all 34 vanilla Skyrim water types render at the shader's minimum normal tilt with zero per-water differentiation. Skyrim is WATAL's canonical reference game.
- **Fix**: delete the `normal_magnitude ← DNAM[92]` assignment; leave the `1.0` sentinel until an offset is byte-decoded. Fix `displacement`/`rain_start_size` per WATR-ARB-02 in the same change.

### WATR-ARB-02 — Rain and Displacement `Starting Size` are read from each other's block in three decoders
- **Severity**: MEDIUM
- **Location**: `water.rs:524-531` (Oblivion), `:592-606` (FO3/FNV long `DATA`), `:803-807` + `:834-837` (Skyrim)
- **Status**: NEW
- **Evidence**: the per-game verdict table above; 17/17 Oblivion, 11/11 FO3 + 8/8 FNV long-`DATA`, 34/34 Skyrim.
- **Impact**: `mat.displacement[0]` and `mat.rain_start_size` are both live to the GPU (`byroredux/src/render/water.rs:300-303`). On Oblivion every water gets a ripple starting size 5× too small and a rain ripple 5× too large; on Skyrim `rain_start_size` is never set at all.
- **Fix**: read the displacement block as F/V/Fo/Dm/**Start** at `+0/+4/+8/+12/+16` from the displacement-force offset each decoder already uses (Oblivion 80, FO3/FNV/Skyrim 76) — i.e. `zip([96, 88, 92])` / `zip([92, 84, 88])` — and `rain_start_size` from rain-force `+16` (Oblivion 76, others 72). This matches the already-correct FO4 sibling.

### WATR-ARB-03 — `decode_dnam_fo76` decodes a layout FO76 does not use; `displacement` receives wind-direction degrees
- **Severity**: MEDIUM
- **Location**: `water.rs:1059-1155`
- **Status**: NEW. **Contradicts both sibling claims**, each of which certified this function as correct.
- **Evidence**: FO76's 148-byte `DNAM` is structurally identical to Starfield's 152-byte `DNAM` over bytes 0..144 (table above). Measured over all 47 records: `[80] == 0.05` ×47 (the displacement starting size the decoder never reads), `[48]` has 25 distinct values (the real normal magnitude, never read), `[52] == 1.0` ×47 (what the decoder *does* read as normal magnitude), and `[84]/[88]/[92]` are degrees (15/17/20 distinct, e.g. 239.328 / 331.848 / 62.352) — which the decoder assigns to `displacement[1]/[2]/[0]` and `env_translate` then clamps into `[0, 10000]`. It also reads `reflectivity ← [64]` (displacement force 0.1), `fresnel ← [68]` (displacement velocity 0.85), `wave_amplitude ← [76]` (displacement dampener 0.97), `wave_frequency ← [80]` (starting size 0.05), and `read_rgb_at(4)/(8)` over what are float triples, not packed RGBA (FO4's *are* packed RGBA — `3a 35 21 00` — which is what makes the two layouts distinguishable).
- **Impact**: every FO76 water body's colour, fog, reflectivity, Fresnel, wave motion and ripple profile is decoded from the wrong fields. MEDIUM rather than HIGH only because ROADMAP lists FO76 as archive/NIF-parse coverage with no shipped playable cell.
- **Fix**: rebase `decode_dnam_fo76` on `decode_dnam_starfield`'s offset map, minus the trailing roughness at 148.

### WATR-ARB-04 — the majority FO3/FNV path (`DNAM`, 196 B) stops decoding at byte 52
- **Severity**: MEDIUM
- **Location**: `water.rs:1359` (the `_ =>` dispatch arm) → `decode_dnam_pre_fo4` (`:694-753`)
- **Status**: NEW. Missed by both claims — Claim A's and Claim B's FO3/FNV analyses both target `decode_data_fo3nv`, which vanilla FO3/FNV reaches on only a minority of records.
- **Evidence**: sub-record census. FO3: 11 records carry a 186-byte `DATA` (→ `decode_data_fo3nv`), **42 carry a 196/184-byte `DNAM`** (→ `decode_dnam_pre_fo4`). FNV: 8 vs **70**. The two forms never co-occur on the same record (0/53 and 0/78). `decode_dnam_pre_fo4` reads bytes 0..52 and returns.
- **Impact**: on 79% of FO3 and 90% of FNV vanilla water types the rain simulator, displacement simulator, three noise layers, fog amounts, underwater fog pair, noise UV scales, amplitudes and the specular tail are all left at canonical defaults. The `DNAM` head is otherwise correct — verified against shipped bytes: `[0]`=wind speed (3.0), `[4]`=direction (90), `[8]/[12]`=wave amp/freq (0.2 / 0.25), `[16]`=sun power (826), `[20]`=reflectivity, `[24]`=fresnel, `[28]`=unnamed 0, `[32]/[36]`=fog near/far (−80 / 850), `[40]/[44]/[48]`=packed RGBA.
- **Fix**: route `GameKind::Fallout3NV` `DNAM` through the same tail decode as the 186-byte `DATA` (the two are offset-identical from byte 56 onward — verified: both carry `0.1 0.6 0.985 2 0.01 | 0.4 0.6 0.985 10 0.05` at 56..92).

### WATR-ARB-05 — `decode_data_fo3nv` sources `normal_magnitude` from `DATA[96]`, which is uninitialised on some records and an 8× amplifier on others
- **Severity**: LOW
- **Location**: `water.rs:604-608`
- **Status**: NEW
- **Evidence**: across all long-`DATA` records, offset 96 reads `0xCDCDCDCD` (`-4.316e8`, MSVC uninitialised fill) on 3/11 FO3 and 2/8 FNV records, and otherwise `0.36 / 0.4 / 0.7 / 1.5 / 1.8 / 7.25 / 9.1`. The Skyrim decoder calls the same offset `noise_falloff`; the two cannot both be right, and Skyrim's own `[96]` distribution (0 / 4 / 100 / 445 / 1009 / 3770 / 4007 / 5000 / 8192) matches the FO76/Starfield noise-falloff family (4096 / 8192 / 100), while FO3/FNV's does not.
- **Impact**: negative reads fall back to the neutral `1.0`, but `9.1` clamps to `8.0` and multiplies all three noise amplitudes 8×. Bounded to the ≤19 long-`DATA` records.
- **Fix**: drop the assignment; leave `normal_magnitude` neutral until offset 96 is byte-decoded for the Fallout layout specifically.

### WATR-ARB-06 — three fixtures pin the swapped labelling as expected behaviour
- **Severity**: LOW
- **Location**: `water.rs:1574,1579,1589-1590` (Oblivion); `:1622-1625,1645-1646` (FO3/FNV); `:1736,1738,1780,1788` (Skyrim)
- **Status**: NEW
- **Evidence**: quoted in full under sub-question 5, including the direct contradiction with `:1855-1859` / `:1904` (FO4) over the identical five offsets.
- **Fix**: correct the three fixtures alongside WATR-ARB-01/02, and add the real-data guard Claim A proposed — assert in `crates/plugin/tests/parse_real_esm.rs` that no scalar folded into `noise_amplitude_scales` is invariant across a game's whole WATR population. Invariance across 34 authored records is the signal that caught this.

### WATR-ARB-07 — the 86-byte Oblivion `DATA` variant reads a falloff as `wave_amplitude`
- **Severity**: LOW
- **Location**: `water.rs:512-521`
- **Status**: NEW
- **Evidence**: 2/23 `Oblivion.esm` records ship an 86-byte `DATA` (`SwampWater`, `MS31Water`). Their bytes at 60..80 are `0.1 0.6 0.985 | 0.4 0.6 0.985` — two three-float simulators (force/velocity/falloff, no dampener or starting size) followed by the `u16` damage at 84, which is exactly 86 bytes. `decode_data_oblivion` reads `wave_amplitude ← [80]` = `0.985` (a falloff) and `wave_frequency ← [84]` = out of range.
- **Impact**: two records; both get a nonsensical `wave_amplitude`.
- **Fix**: gate the simulator reads on `data.len() >= 102`, or add the short-form arm.

---

## Which sibling findings must be amended or withdrawn

| Finding | Action |
|---|---|
| `LC-D6-01` (`/audit-legacy-compat`, HIGH) | **UPHELD in substance.** Amend two details: (a) the Skyrim consequence is not a uniform ~20× attenuation but a collapse of all 34 records onto the shader's `max(detail, 0.05)` floor — the flattening ratio is 13.9×–18.6× and *all* per-water variation is lost; (b) its "FO4 and FO76 in the same file get it right" is half wrong — **FO76 is wrong** (WATR-ARB-03). Its `0.05` count should read 34/34, not 31/31. Its suggested fix is correct as written. |
| `FO4-D6-2026-08-20-02` (`/audit-fo4`, MEDIUM) | **PARTIALLY WITHDRAWN.** Its Oblivion analysis, byte evidence and suggested fix (`zip([96, 88, 92])`, `rain_start_size ← 76`) are **correct and should be published**. Its two collateral assertions must be struck: *"the FO4/FO76/FNV/Skyrim decoders in the same file are all correct — do not 'fix' them to match"* is false for FNV, Skyrim **and** FO76, and *"the FO3/FNV sibling twenty lines up reads `[72, 84, 88]` + `rain_start_size ← 92`, which **is** correct for its layout"* is refuted by 19/19 vanilla long-`DATA` records. Only its FO4 half survives — `decode_dnam_fo4`'s simulator offsets are confirmed correct here. |
| `/audit-skyrim` corroboration of LC-D6-01 | **UPHELD.** |
| `/audit-esm`, `/audit-fnv`, `/audit-fo3`, `/audit-oblivion` downstream acceptance of LC-D6-01 | **UPHELD**, with the FO76 correction above, and with WATR-ARB-04 added for `/audit-fnv` and `/audit-fo3`: the misaligned `decode_data_fo3nv` is the *minority* path on both games; the majority `DNAM` path decodes nothing past byte 52 and is the larger defect. |
| `docs/engine/watal.md:255-256,487,690-695` | Must be corrected: "Skyrim DNAM[92]" is the displacement starting size, not a physical normal magnitude. §9 Q5 already flags offset 92 as MEDIUM-confidence "verify before relying" — this is that verification, and it fails. The FO4 half of the same row (`FO4 DNAM[52]`) is confirmed correct. `:279` ("DATA[92] … rain-ripple scale") and `:338` ("DATA[76,88,92]") carry the same inversion. |

TALLY: CRITICAL=0 HIGH=1 MEDIUM=3 LOW=3
