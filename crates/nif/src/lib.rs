//! NIF file parser for Gamebryo .nif files.
//!
//! Parses the binary NIF format used by Gamebryo 2.3 and derivative engines
//! (Oblivion, Skyrim, Fallout 3/4). Three-phase loading: parse → link → scene.
//!
//! # Usage
//! ```ignore
//! let bytes = std::fs::read("mesh.nif")?;
//! let scene = byroredux_nif::parse_nif(&bytes)?;
//! for block in &scene.blocks {
//!     println!("{}", block.block_type_name());
//! }
//! ```

pub mod anim;
pub mod blocks;
pub mod header;
pub mod import;
pub mod kfm;
pub mod rotation;
pub mod scene;
pub mod shader_flags;
pub mod stream;
pub mod types;
pub mod version;

use blocks::{parse_block, NiObject};
use header::NifHeader;
use scene::NifScene;
use std::collections::HashMap;
use std::io;
use std::sync::Arc;
use stream::NifStream;

/// Options for NIF parsing — allows skipping block categories for performance.
#[derive(Debug, Clone, Default)]
pub struct ParseOptions {
    /// Skip animation blocks (controllers, interpolators, animation data).
    /// Reduces parse time by 40-60% for character NIFs. Skipped blocks are
    /// replaced with NiUnknown placeholders (via block_size).
    /// Only effective when the NIF header has block sizes (v20.2.0.7+).
    pub skip_animation: bool,
    /// Hand-registered skip sizes for Oblivion-era unknown block types.
    ///
    /// Oblivion NIFs (v20.0.0.4/5) have no `block_sizes` table, so when a
    /// block parser returns `Err` the main loop cannot resume — it stops
    /// and marks the scene truncated, losing every subsequent block.
    ///
    /// When a type name is registered here, the loop instead seeks forward
    /// by the given size and inserts an `NiUnknown` placeholder, letting
    /// the rest of the file parse normally. Intended for rare/undiscovered
    /// block types whose size is known from an external source (Gamebryo
    /// 2.3 headers, nif_stats corpus analysis, modder documentation) but
    /// whose full schema has not been implemented yet. See #224.
    pub oblivion_skip_sizes: HashMap<String, u32>,
    /// Run [`crate::scene::NifScene::validate_refs`] after parse and
    /// store the dangling-ref count in `scene.link_errors`. Off by
    /// default — the walk visits every block once and follows every
    /// trait-exposed `BlockRef` plus `NiNode.children` / `effects`,
    /// so on a 1k-block NIF it adds a few µs to the parse path.
    /// Useful for debug builds, `nif_stats`, and the
    /// `tests/parse_real_nifs.rs` integration sweep that wants a
    /// link-integrity histogram. See #892.
    pub validate_links: bool,
}

/// Animation block type names that can be skipped in geometry-only mode.
fn is_animation_block(type_name: &str) -> bool {
    matches!(
        type_name,
        "NiControllerManager"
            | "NiControllerSequence"
            | "NiMultiTargetTransformController"
            | "NiTransformController"
            | "NiVisController"
            | "bhkBlendController"
            | "NiAlphaController"
            | "BSNiAlphaPropertyTestRefController"
            | "NiFloatExtraDataController"
            | "NiLightColorController"
            | "NiLightDimmerController"
            | "NiLightIntensityController"
            | "NiLightRadiusController"
            | "NiMaterialColorController"
            | "NiTextureTransformController"
            | "NiGeomMorpherController"
            | "NiTransformInterpolator"
            | "BSRotAccumTransfInterpolator"
            | "NiTransformData"
            | "NiKeyframeData"
            | "NiFloatInterpolator"
            | "NiFloatData"
            | "NiPoint3Interpolator"
            | "NiPosData"
            | "NiColorInterpolator"
            | "NiColorData"
            | "NiBoolInterpolator"
            | "NiBoolTimelineInterpolator"
            | "NiBoolData"
            | "NiBlendTransformInterpolator"
            | "NiBlendFloatInterpolator"
            | "NiBlendPoint3Interpolator"
            | "NiBlendBoolInterpolator"
            | "NiTextKeyExtraData"
            | "NiDefaultAVObjectPalette"
            | "NiMorphData"
            // #394 — newly dispatched Oblivion-era animation blocks.
            // Listed so `ParseOptions::skip_animation_blocks` can
            // fast-skip them in geometry-only mode.
            | "NiPathInterpolator"
            | "NiFlipController"
            | "NiBSBoneLODController"
    )
}

/// Return true for the Havok constraint block types whose parsers
/// intentionally consume only the 16-byte `bhkConstraintCInfo` base
/// and delegate the rest of the payload to the outer block_sizes
/// reconciliation path (#117).
///
/// The stubs are correct — every skeleton NIF contains 10–50 constraint
/// blocks and all of them under-consume by design. Without this list
/// the reconciliation path would fire a `warn!` for each, drowning
/// real parser-drift signals in an actor-spawn log (#462). When a full
/// CInfo parser lands for any of these types, remove it from here so
/// the drift detector goes back to catching real mistakes.
fn is_havok_constraint_stub(type_name: &str) -> bool {
    matches!(
        type_name,
        "bhkBallAndSocketConstraint"
            | "bhkHingeConstraint"
            | "bhkLimitedHingeConstraint"
            | "bhkPrismaticConstraint"
            | "bhkRagdollConstraint"
            | "bhkStiffSpringConstraint"
            | "bhkMalleableConstraint"
            | "bhkGenericConstraint"
            // #979 / NIF-D5-NEW-03
            | "bhkBallSocketConstraintChain"
    )
}

/// Parse a NIF file from raw bytes.
///
/// Performs all three phases: parse header → parse blocks → build scene.
pub fn parse_nif(data: &[u8]) -> io::Result<NifScene> {
    parse_nif_with_options(data, &ParseOptions::default())
}

/// Parse a NIF file with options (e.g., skip animation blocks).
pub fn parse_nif_with_options(data: &[u8], options: &ParseOptions) -> io::Result<NifScene> {
    // Phase 1: Parse header
    let (header, block_data_offset) = NifHeader::parse(data)?;
    log::debug!(
        "NIF version {}, {} blocks, {} strings",
        header.version,
        header.num_blocks,
        header.strings.len()
    );

    // Validate endianness — we only support little-endian (all PC game content).
    // Big-endian NIFs (Xbox 360 console ports) would produce silently wrong data.
    if !header.little_endian {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Big-endian NIF files are not supported (console format)",
        ));
    }

    // Phase 2: Parse blocks
    let block_data = &data[block_data_offset..];
    let mut stream = NifStream::new(block_data, &header);
    // #388: bound `num_blocks` against the remaining stream so a header
    // u32 that drifted past validation can't trip a 16-bytes-per-slot
    // multi-GB allocation. Each block consumes at least one byte
    // (BlockRef in older streams) so num_blocks > remaining is corrupt.
    let mut blocks: Vec<Box<dyn NiObject>> = stream.allocate_vec(header.num_blocks)?;
    // Set to `true` if an Oblivion-style (no block-sizes) parse bails out
    // early — `NifScene.truncated` exposes the state to downstream
    // consumers so they can decide how to handle the incomplete graph.
    let mut truncated = false;
    let mut dropped_block_count: usize = 0;
    // Count of blocks that fell into the NiUnknown recovery path. Bumped
    // inside the block-size recovery, runtime size cache, and
    // oblivion_skip_sizes recovery branches below, plus once per block
    // the dispatch-level unknown-type fallback returns. Surfaces via
    // `NifScene.recovered_blocks` so the parse-rate gate treats these
    // NIFs as non-clean — pre-#568 the warn-and-continue path left the
    // scene flagged as clean and hid under-consuming parser bugs like
    // #546. See #568.
    let mut recovered_blocks: usize = 0;

    // Per-block-type recovery counters. Bumped every time the
    // block_size-driven `Err` recovery fires, the runtime size cache
    // skip hint fires, or the oblivion_skip_sizes fallback fires.
    // Aggregated at end of parse into a single `log::warn!` summary
    // line — pre-#565 every recovery emitted its own `warn!`, so a
    // single `Skyrim - Meshes0.bsa` walk fired thousands of warnings
    // (BSLagBoneController × 3,300, BSLODTriShape × 50,
    // BSWaterShaderProperty × 50, etc.) drowning every other log line.
    // The per-block detail still surfaces at `log::debug!` for
    // targeted parser debugging. See #565 / SK-D5-04.
    let mut recovered_by_type: std::collections::HashMap<String, u32> =
        std::collections::HashMap::new();
    // Companion counter for the success-branch drift detector — the
    // parser returned `Ok` but consumed != block_size, so block_size
    // realigns the stream while the (possibly-wrong) parsed struct
    // stays in the scene. Pre-#565 each instance emitted its own
    // `warn!` line ("Block N 'TypeX': expected A bytes, consumed B.
    // Adjusting position."). Same aggregation pattern, separate
    // summary line so consumers can distinguish "block lost to
    // NiUnknown" from "block parsed with stream drift" at a glance.
    let mut drifted_by_type: std::collections::HashMap<String, u32> =
        std::collections::HashMap::new();
    // Per-block-type drift histogram — keyed by `declared - consumed`.
    // Populated alongside `drifted_by_type` so the parse-end summary log
    // stays unchanged while a richer, structured surface is also
    // available via `NifScene.drift_histogram`. Used by `nif_stats
    // --drift-histogram` to aggregate byte-level parser drift across
    // archive walks: a `100 % clean` rate can paper over a parser
    // that's consistently 1 byte short on every instance of a given
    // type, and only the magnitude (not the count) surfaces the
    // pattern. See #939.
    let mut drift_histogram: std::collections::HashMap<
        String,
        std::collections::HashMap<i64, u32>,
    > = std::collections::HashMap::new();
    // Parallel histogram for blocks intentionally skipped from
    // `drift_histogram` because the parser is a known stub (Havok
    // constraint CInfos — see `is_havok_constraint_stub`). The real
    // histogram excludes these so a future audit running
    // `nif_stats --drift-histogram` doesn't see ~45 systematic
    // under-reads per skeleton load and falsely conclude
    // constraints parse cleanly. Surfacing them separately means
    // the same audit can still spot a new stub regression
    // (constraint type drifts from its expected stub size) without
    // polluting the real-parser signal. See NIF-D3-NEW-06 (audit
    // 2026-05-12).
    let mut stubbed_drift_histogram: std::collections::HashMap<
        String,
        std::collections::HashMap<i64, u32>,
    > = std::collections::HashMap::new();

    // For Oblivion-era NIFs (no block_sizes table), track the consumed
    // byte count for each successfully parsed block type. When a block
    // fails to parse, the cache provides a skip hint for that type —
    // same concept as oblivion_skip_sizes but self-calibrating from the
    // file itself rather than a manually-maintained table. See #324.
    let mut parsed_size_cache: std::collections::HashMap<String, Vec<u32>> =
        std::collections::HashMap::new();

    // Bump a per-type counter without paying `to_string()` on the hot
    // (already-seen) path. `HashMap::entry(K)` takes K by value, so a
    // direct `entry(name.to_string())` would allocate every block —
    // see #832 for the audit numbers (~150 KB/cell on Oblivion).
    fn bump_counter(map: &mut std::collections::HashMap<String, u32>, key: &str) {
        if let Some(c) = map.get_mut(key) {
            *c += 1;
        } else {
            map.insert(key.to_string(), 1);
        }
    }

    // Record one drift event into the per-type histogram. Uses the same
    // get/insert split as `bump_counter` to skip `to_string()` on the
    // common already-seen-this-type path.
    fn bump_drift(
        map: &mut std::collections::HashMap<String, std::collections::HashMap<i64, u32>>,
        key: &str,
        drift: i64,
    ) {
        if let Some(inner) = map.get_mut(key) {
            *inner.entry(drift).or_insert(0) += 1;
        } else {
            let mut inner = std::collections::HashMap::new();
            inner.insert(drift, 1u32);
            map.insert(key.to_string(), inner);
        }
    }

    let no_block_sizes = header.block_sizes.is_empty() && header.num_blocks > 0;

    if no_block_sizes {
        log::debug!(
            "NIF v{} has no block sizes — runtime size cache will track parse sizes for error recovery",
            header.version,
            );
    }

    // Pre-Gamebryo NetImmerse files (NIF v < 5.0.0.1, e.g. Morrowind at
    // v4.0.0.2) inline each block's type name as a sized string instead of
    // using a global block-type table. We read them inline in the loop below.
    let inline_type_names = header.block_types.is_empty() && header.num_blocks > 0;
    if inline_type_names {
        log::debug!(
            "NIF v{} uses inline block type names (pre-Gamebryo, {} blocks)",
            header.version,
            header.num_blocks
        );
    }

    for i in 0..header.num_blocks as usize {
        // Resolve block type name: from header table (Gamebryo+) or
        // inline string (pre-Gamebryo). The four `NiUnknown` recovery
        // sites below clone the `Arc<str>` produced here rather than
        // calling `Arc::from(&str)` per dispatch failure (#834). The
        // header-table path refcount-clones the header's existing
        // `Arc<str>` storage; the (rare) inline path allocates one
        // fresh `Arc<str>` per block since each inline name lives
        // for one block only.
        let type_name_arc: Arc<str> = if inline_type_names {
            // Pre-Gamebryo: each block is prefixed by a u32-length-prefixed type name string.
            // Use truncation rather than hard-Err if the inline name read fails (e.g. a
            // corrupt-by-design debug NIF whose type-name length field overflows the alloc
            // cap — #698 Oblivion `marker_radius.nif`).
            match stream.read_sized_string() {
                Ok(name) => Arc::from(name),
                Err(e) => {
                    log::warn!(
                        "Block {} inline type-name read failed: {} — truncating (keeping {} blocks)",
                        i, e, blocks.len()
                    );
                    truncated = true;
                    dropped_block_count = header.num_blocks as usize - i;
                    break;
                }
            }
        } else {
            match header.block_type_name_arc(i) {
                Some(arc) => Arc::clone(arc),
                None => {
                    log::warn!(
                        "Block {} has no type name in header table — truncating (keeping {} blocks)",
                        i, blocks.len()
                    );
                    truncated = true;
                    dropped_block_count = header.num_blocks as usize - i;
                    break;
                }
            }
        };
        let type_name: &str = type_name_arc.as_ref();

        let block_size = header.block_sizes.get(i).copied();
        let start_pos = stream.position();

        // Skip animation blocks when geometry-only parsing is requested.
        if options.skip_animation && is_animation_block(type_name) {
            if let Some(size) = block_size {
                stream.skip(size as u64)?;
                blocks.push(Box::new(blocks::NiUnknown {
                    type_name: Arc::clone(&type_name_arc),
                    data: Vec::new(), // Don't store data — just a placeholder
                }));
                continue;
            }
            // No block_size (Oblivion) — must parse, can't skip
        }

        match parse_block(type_name, &mut stream, block_size) {
            Ok(block) => {
                let consumed = stream.position() - start_pos;
                log::trace!(
                    "Block {} '{}': offset {}, consumed {} bytes",
                    i,
                    type_name,
                    start_pos,
                    consumed
                );
                // Verify we consumed exactly block_size bytes (if known)
                if let Some(size) = block_size {
                    let consumed = stream.position() - start_pos;
                    if consumed != size as u64 {
                        // Havok constraint stubs (per #117) intentionally
                        // read only the 16-byte `bhkConstraintCInfo` base
                        // and let the block_sizes table reconcile the
                        // payload. These under-consumes are by design and
                        // fire on every skeleton NIF load (~45 warnings
                        // per actor — see #462). Downgrade the known-stub
                        // case to `trace!` so real parser drift stays
                        // visible. The finished constraint parsers will
                        // remove this exception entirely.
                        if is_havok_constraint_stub(type_name) {
                            log::trace!(
                                "Block {} '{}': stub consumed {}/{} bytes (block_size reconciled).",
                                i,
                                type_name,
                                consumed,
                                size,
                            );
                            // Record into the parallel `stubbed_drift_
                            // histogram` so audit telemetry can see the
                            // stub under-reads without contaminating the
                            // real drift signal. See NIF-D3-NEW-06.
                            let drift = size as i64 - consumed as i64;
                            bump_drift(&mut stubbed_drift_histogram, type_name, drift);
                        } else {
                            // #565: downgraded from `warn!` — the
                            // per-NIF summary at the end of this
                            // function rolls these into a single
                            // `warn!` line. Per-block detail stays
                            // visible at `debug!` for parser-author
                            // debugging. #939: log the signed drift
                            // explicitly so per-block grep'ing
                            // (`drift=+1`) picks out the canonical
                            // 1-byte-short `NiTexturingProperty`
                            // pattern without arithmetic.
                            let drift = size as i64 - consumed as i64;
                            log::debug!(
                                "Block {} '{}': declared={} consumed={} drift={:+} — adjusting position.",
                                i,
                                type_name,
                                size,
                                consumed,
                                drift,
                            );
                            bump_counter(&mut drifted_by_type, type_name);
                            bump_drift(&mut drift_histogram, type_name, drift);
                        }
                        stream.set_position(start_pos + size as u64);
                    }
                }
                // Cache consumed size for Oblivion recovery (#324).
                if no_block_sizes {
                    let final_consumed = (stream.position() - start_pos) as u32;
                    // Drift detector — debug builds only (#395). Catches
                    // the earliest sign of an upstream parser drifting
                    // the stream by an unexpected number of bytes; the
                    // perpetrator is silent in the success path so
                    // without this hook the first symptom is a much
                    // later block surfacing garbage enum values.
                    #[cfg(debug_assertions)]
                    if let Some(prior) = parsed_size_cache.get(type_name) {
                        if let Some(msg) = drift_warning(final_consumed, prior) {
                            log::warn!(
                                "Stream drift suspect: block {} '{}' (offset {}) {} \
                                 — a previous block likely under- or over-consumed; \
                                 treat downstream garbage reads as symptoms, not the \
                                 cause. See #395.",
                                i,
                                type_name,
                                start_pos,
                                msg
                            );
                        }
                    }
                    // #832 — same allocation-on-every-block bug as the
                    // drifted_by_type bump above. This site fires on
                    // EVERY successful parse on Oblivion-no-block-sizes
                    // files (~7500 blocks per cell load).
                    if let Some(v) = parsed_size_cache.get_mut(type_name) {
                        v.push(final_consumed);
                    } else {
                        parsed_size_cache.insert(type_name.to_string(), vec![final_consumed]);
                    }
                }
                // Dispatch-level unknown-type recovery: `parse_block`'s
                // fallback at `blocks/mod.rs` returns `Ok(NiUnknown)`
                // when the header advertised a type that isn't in the
                // dispatch table. Bump the recovery counter so these
                // NIFs don't count as clean on the parse-rate gate —
                // same rationale as the three Err-branch recoveries
                // below. See #568.
                //
                // Also bump the per-type rollup so the
                // `recovered N block(s) via …: {per_type_rollup}`
                // summary tells operators which unknown types are
                // flooding (e.g. a newly-shipped Bethesda block
                // class showing up in a content sweep). Pre-fix the
                // dispatch path bumped the total but left
                // `recovered_by_type` empty, so the summary listed
                // only the err-branch recoveries. See NIF-D3-NEW-05
                // (audit 2026-05-12).
                if type_name != "NiUnknown" && block.block_type_name() == "NiUnknown" {
                    recovered_blocks += 1;
                    bump_counter(&mut recovered_by_type, type_name);
                }
                blocks.push(block);
            }
            Err(e) => {
                let consumed = stream.position() - start_pos;
                if let Some(size) = block_size {
                    // With block_size we can recover: seek to the expected end of
                    // the block, record an NiUnknown placeholder, and keep going.
                    // Without this, a single buggy block parser (e.g. a Havok
                    // layout quirk) takes down the entire NIF. The unit tests
                    // still exercise the happy path; this is the belt-and-braces
                    // path that keeps `parse_rate_*` integration tests meaningful.
                    //
                    // #565 / SK-D5-04: downgraded from `warn!` to `debug!`
                    // because Skyrim Meshes0 has thousands of these per
                    // archive walk; the per-NIF summary at the end of this
                    // function rolls the counts up into a single `warn!`
                    // line so the high-level signal stays visible without
                    // drowning every other log.
                    log::debug!(
                        "Block {} '{}' (size {}, offset {}, consumed {}): {} — \
                         seeking past block and inserting NiUnknown",
                        i,
                        type_name,
                        size,
                        start_pos,
                        consumed,
                        e
                    );
                    stream.set_position(start_pos + size as u64);
                    blocks.push(Box::new(blocks::NiUnknown {
                        type_name: Arc::clone(&type_name_arc),
                        data: Vec::new(),
                    }));
                    recovered_blocks += 1;
                    bump_counter(&mut recovered_by_type, type_name);
                    continue;
                }
                // Without block_size (Oblivion), there's no header-driven
                // recovery. Try three fallbacks in order:
                //
                // 1. Runtime size cache: if we successfully parsed another
                //    instance of this type earlier in the file, use its
                //    median consumed size as a skip hint. Self-calibrating
                //    and file-specific — handles variable-size types where
                //    instances in one NIF tend to be similarly sized. #324.
                //
                // 2. oblivion_skip_sizes: caller-registered fixed-size hints
                //    for rare types that only appear once per NIF. #224.
                //
                // 3. Truncation: discard remaining blocks.
                if let Some(sizes) = parsed_size_cache.get(type_name) {
                    if !sizes.is_empty() {
                        let mut sorted = sizes.clone();
                        sorted.sort_unstable();
                        let median_size = sorted[sorted.len() / 2];
                        stream.set_position(start_pos);
                        if stream.skip(median_size as u64).is_ok() {
                            log::info!(
                                "Block {} '{}' (offset {}): skipped {} bytes via \
                                 runtime size cache (median of {} prior parses; was: {})",
                                i,
                                type_name,
                                start_pos,
                                median_size,
                                sizes.len(),
                                e
                            );
                            blocks.push(Box::new(blocks::NiUnknown {
                                type_name: Arc::clone(&type_name_arc),
                                data: Vec::new(),
                            }));
                            recovered_blocks += 1;
                            bump_counter(&mut recovered_by_type, type_name);
                            continue;
                        }
                    }
                }
                if let Some(&skip_size) = options.oblivion_skip_sizes.get(type_name) {
                    // Rewind whatever the failed parse consumed, then skip
                    // the full registered size. `set_position` is safe; the
                    // stream is backed by an in-memory slice.
                    stream.set_position(start_pos);
                    if stream.skip(skip_size as u64).is_ok() {
                        log::info!(
                            "Block {} '{}' (offset {}): skipped {} bytes via \
                             oblivion_skip_sizes hint (was: {})",
                            i,
                            type_name,
                            start_pos,
                            skip_size,
                            e
                        );
                        blocks.push(Box::new(blocks::NiUnknown {
                            type_name: Arc::clone(&type_name_arc),
                            data: Vec::new(),
                        }));
                        recovered_blocks += 1;
                        bump_counter(&mut recovered_by_type, type_name);
                        continue;
                    }
                    // If the skip would go past EOF, fall through to the
                    // truncation path — the caller's hint was wrong.
                    log::warn!(
                        "Block {} '{}' (offset {}): oblivion_skip_sizes hint of {} \
                         bytes would exceed file length; truncating",
                        i,
                        type_name,
                        start_pos,
                        skip_size
                    );
                }

                // Stop parsing but keep blocks parsed so far. This allows
                // geometry blocks to be imported even when collision blocks
                // fail. `truncated = true` is exposed via NifScene so
                // consumers that care about completeness can detect the
                // partial state. See #175.
                let dropped = header.num_blocks as usize - i;
                log::warn!(
                    "Block {} '{}' (offset {}, consumed {}): {} — stopping parse; \
                     keeping {} blocks, DISCARDING {} subsequent blocks (scene marked truncated)",
                    i,
                    type_name,
                    start_pos,
                    consumed,
                    e,
                    blocks.len(),
                    dropped
                );
                truncated = true;
                dropped_block_count = dropped;
                break;
            }
        }
    }

    // #565: emit aggregated `warn!` summaries for the per-block-type
    // recovery + drift counts collected during the parse loop.
    // Pre-#565 every recovery / drift emitted its own `warn!` line,
    // drowning out everything else when an archive walk hit thousands
    // of recoverable blocks. The per-block detail is preserved at
    // `log::debug!` for targeted debugging; these summaries keep the
    // high-level signal visible at the default log level. Empty maps
    // → no log output.
    fn format_type_count_map(map: &std::collections::HashMap<String, u32>) -> String {
        // Sort by count descending then type-name for deterministic
        // output (helps log diffs across runs and aggregator scripts).
        let mut entries: Vec<(&String, &u32)> = map.iter().collect();
        entries.sort_by(|a, b| b.1.cmp(a.1).then_with(|| a.0.cmp(b.0)));
        entries
            .iter()
            .map(|(name, count)| format!("{name}={count}"))
            .collect::<Vec<_>>()
            .join(", ")
    }
    if !recovered_by_type.is_empty() {
        log::warn!(
            "NIF parse recovered {} block(s) via block_size / runtime-cache / \
             oblivion_skip_sizes paths: {} (set \
             RUST_LOG=byroredux_nif=debug for per-block detail)",
            recovered_blocks,
            format_type_count_map(&recovered_by_type),
        );
    }
    if !drifted_by_type.is_empty() {
        let drift_total: u32 = drifted_by_type.values().sum();
        log::warn!(
            "NIF parse: {} block(s) parsed Ok but consumed != block_size; \
             stream realigned by header size table: {} (set \
             RUST_LOG=byroredux_nif=debug for per-block detail)",
            drift_total,
            format_type_count_map(&drifted_by_type),
        );
    }

    // Phase 3: Identify root. Root is typically the first NiNode (or
    // any of its specialised Bethesda subclasses: BSTreeNode, NiSwitchNode,
    // BSMultiBoundNode, etc. — see `is_ni_node_subclass`). When the
    // scene is truncated that "first NiNode" may be a subtree rather
    // than the real root — the warning above documents the risk.
    //
    // Pre-#611 this matched only the literal `"NiNode"`. Scenes rooted
    // at a subclass with its own Rust struct (BSTreeNode for SpeedTree,
    // BsValueNode for FO3/FNV metadata roots, NiSwitchNode for furniture
    // states, etc.) skipped past their real root and picked the first
    // plain-NiNode child — typically a leaf bone container — yielding
    // 0 imported meshes for tree LODs, weapon-state switches, and
    // similar content. `BSFadeNode` / `BSLeafAnimNode` happened to
    // survive only because `parse_block` aliases them to `NiNode`
    // (their `block_type_name` already returns `"NiNode"`).
    let root_index = if !blocks.is_empty() {
        blocks
            .iter()
            .position(|b| is_ni_node_subclass(b.block_type_name()))
            .or(Some(0))
    } else {
        None
    };

    // Convert the per-parse `HashMap<String, HashMap<i64, u32>>` into the
    // deterministic `BTreeMap<String, BTreeMap<i64, u32>>` surface
    // documented on `NifScene.drift_histogram`. The HashMap is the
    // hot-path-friendly shape inside the loop; the BTreeMap gives
    // diff-friendly iteration order to downstream consumers
    // (`nif_stats --drift-histogram`, baseline regression tests). See #939.
    let scene_drift_histogram: std::collections::BTreeMap<
        String,
        std::collections::BTreeMap<i64, u32>,
    > = drift_histogram
        .into_iter()
        .map(|(type_name, inner)| (type_name, inner.into_iter().collect()))
        .collect();
    // Same hashmap → btreemap conversion for the stubbed signal —
    // see `scene_drift_histogram` above. Visible alongside the
    // real histogram so `nif_stats --drift-histogram` can opt in.
    let scene_stubbed_drift_histogram: std::collections::BTreeMap<
        String,
        std::collections::BTreeMap<i64, u32>,
    > = stubbed_drift_histogram
        .into_iter()
        .map(|(type_name, inner)| (type_name, inner.into_iter().collect()))
        .collect();

    let mut scene = NifScene {
        blocks,
        root_index,
        truncated,
        dropped_block_count,
        recovered_blocks,
        link_errors: 0,
        drift_histogram: scene_drift_histogram,
        stubbed_drift_histogram: scene_stubbed_drift_histogram,
    };
    // Opt-in dangling-ref walk (#892). Off by default; debug builds,
    // `nif_stats`, and integration sweeps flip
    // `ParseOptions::validate_links` to surface link-integrity drift
    // that the parse-rate gate alone wouldn't catch (a parser
    // regression that produces technically-Ok scenes with
    // semantically-broken `BlockRef`s would otherwise only show up
    // as a render artifact).
    if options.validate_links {
        let errors = scene.validate_refs();
        if !errors.is_empty() {
            log::warn!(
                "parse_nif: validate_links found {} dangling BlockRef(s) \
                 — first 3: {:?}",
                errors.len(),
                errors.iter().take(3).collect::<Vec<_>>(),
            );
        }
        scene.link_errors = errors.len();
    }
    Ok(scene)
}

/// Return `true` when `block_type_name` is `NiNode` or any specialised
/// Bethesda NiNode subclass with its own dispatch arm in `parse_block`.
///
/// Drives the scene-root selection in `parse_nif_with_options`. The
/// list mirrors the dispatch arms in [`crate::blocks::parse_block`]
/// (around line 134 — `"NiNode" | "BSFadeNode" | …` and the dedicated
/// subclass branches that follow). Aliased subclasses (BSFadeNode,
/// BSLeafAnimNode, RootCollisionNode, AvoidNode, NiBSAnimationNode,
/// NiBSParticleNode) parse as `NiNode` and report their type as
/// `"NiNode"`, so they're caught by the first arm; only the dedicated
/// subclass parsers need explicit entries below.
///
/// Update both this list and the dispatch table when adding a new
/// NiNode-derived block type. See #611 / SK-D5-02.
fn is_ni_node_subclass(block_type_name: &str) -> bool {
    matches!(
        block_type_name,
        "NiNode"
            | "BSOrderedNode"
            | "BSValueNode"
            | "BSMultiBoundNode"
            // #942 — BSDistantObjectInstancedNode (FO76) extends
            // BSMultiBoundNode and is a valid root for distant-LOD NIFs.
            | "BSDistantObjectInstancedNode"
            | "BSTreeNode"
            | "NiBillboardNode"
            | "NiSwitchNode"
            | "NiLODNode"
            | "NiSortAdjustNode"
            | "BSRangeNode"
    )
}

/// Heuristic stream-drift detector for Oblivion-style NIFs (no
/// per-block size table). Returns `Some(msg)` when the freshly-parsed
/// `consumed` byte count disagrees with previously-parsed instances of
/// the same type, indicating an upstream parser likely consumed the
/// wrong number of bytes and shifted the stream forward (or backward).
///
/// Gated to debug + test builds: the call site in `parse_nif_with_options`
/// is `#[cfg(debug_assertions)]`, so this function is unreferenced in
/// release. `cfg(any(debug_assertions, test))` keeps both build paths
/// clean — release strips the dead code, and `cargo test --release` (a
/// rare but valid invocation) still compiles the regression suite.
///
/// The detector intentionally only fires when prior samples agree with
/// each other within ±2 bytes — a low-variance signature that suggests
/// a fixed-size type. Variable-size types (NiTriShapeData, NiSkinData,
/// any block carrying a `Vec<T>` whose count varies per instance)
/// generate cache entries with high natural variance and are
/// silently ignored to keep false positives near zero.
///
/// The actual buggy parser is almost always one or two blocks
/// upstream of where the warning fires — by the time a downstream
/// block sees the drift, the perpetrator has already returned `Ok`
/// with a wrong byte count. The earliest detector firing is the most
/// useful breadcrumb. See #395.
#[cfg(any(debug_assertions, test))]
fn drift_warning(consumed: u32, prior: &[u32]) -> Option<String> {
    if prior.len() < 2 {
        return None;
    }
    let &min = prior.iter().min().unwrap();
    let &max = prior.iter().max().unwrap();
    // High-variance type — drift detection unreliable; skip silently.
    if max.saturating_sub(min) > 2 {
        return None;
    }
    let dist = prior
        .iter()
        .map(|&s| (s as i64 - consumed as i64).abs())
        .min()
        .unwrap_or(0);
    if dist <= 2 {
        return None;
    }
    let mut sorted = prior.to_vec();
    sorted.sort_unstable();
    let median = sorted[sorted.len() / 2];
    Some(format!(
        "consumed {consumed} bytes, but {prior_count} prior parse(s) of this type \
         all consumed {min}±{spread} bytes (median {median})",
        prior_count = prior.len(),
        spread = max - min,
    ))
}

#[cfg(test)]
mod tests;
