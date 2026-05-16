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
mod tests {
    use super::*;

    /// Build a complete minimal NIF file (v20.2.0.7, Skyrim-style)
    /// containing a single NiNode block with known field values.
    fn build_test_nif_with_node() -> Vec<u8> {
        let mut buf = Vec::new();

        // ── Header ──────────────────────────────────────────────────
        buf.extend_from_slice(b"Gamebryo File Format, Version 20.2.0.7\n");
        buf.extend_from_slice(&0x14020007u32.to_le_bytes()); // version
        buf.push(1); // little-endian
        buf.extend_from_slice(&11u32.to_le_bytes()); // user_version (FNV)
        buf.extend_from_slice(&1u32.to_le_bytes()); // num_blocks = 1
        buf.extend_from_slice(&34u32.to_le_bytes()); // user_version_2 (FNV)

        // Short strings (author, process, export)
        buf.push(1);
        buf.push(0);
        buf.push(1);
        buf.push(0);
        buf.push(1);
        buf.push(0);

        // Block types: 1 type "NiNode"
        buf.extend_from_slice(&1u16.to_le_bytes());
        buf.extend_from_slice(&6u32.to_le_bytes());
        buf.extend_from_slice(b"NiNode");

        // Block type indices: block 0 → type 0
        buf.extend_from_slice(&0u16.to_le_bytes());

        // ── Build NiNode block data first to know its size ──────────
        let mut block = Vec::new();

        // NiObjectNET: name (string table index 0 = "SceneRoot")
        block.extend_from_slice(&0i32.to_le_bytes());
        // extra_data_refs: count=0
        block.extend_from_slice(&0u32.to_le_bytes());
        // controller_ref: -1 (null)
        block.extend_from_slice(&(-1i32).to_le_bytes());

        // NiAVObject: flags (u32 for version >= 20.2.0.7)
        block.extend_from_slice(&14u32.to_le_bytes());
        // transform: translation (1.0, 2.0, 3.0)
        block.extend_from_slice(&1.0f32.to_le_bytes());
        block.extend_from_slice(&2.0f32.to_le_bytes());
        block.extend_from_slice(&3.0f32.to_le_bytes());
        // identity rotation (9 floats)
        for r in &[1.0f32, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0] {
            block.extend_from_slice(&r.to_le_bytes());
        }
        // scale: 1.0
        block.extend_from_slice(&1.0f32.to_le_bytes());
        // properties: count=0
        block.extend_from_slice(&0u32.to_le_bytes());
        // collision_ref: -1
        block.extend_from_slice(&(-1i32).to_le_bytes());

        // NiNode: children count=0
        block.extend_from_slice(&0u32.to_le_bytes());
        // effects count=0
        block.extend_from_slice(&0u32.to_le_bytes());

        // ── Back to header: block sizes ─────────────────────────────
        buf.extend_from_slice(&(block.len() as u32).to_le_bytes());

        // String table: 1 string "SceneRoot"
        buf.extend_from_slice(&1u32.to_le_bytes()); // num_strings
        buf.extend_from_slice(&9u32.to_le_bytes()); // max_string_length
        buf.extend_from_slice(&9u32.to_le_bytes()); // "SceneRoot" length
        buf.extend_from_slice(b"SceneRoot");

        // num_groups = 0
        buf.extend_from_slice(&0u32.to_le_bytes());

        // ── Block data ──────────────────────────────────────────────
        buf.extend_from_slice(&block);

        buf
    }

    /// Regression test for issue #175: `NifScene.truncated` defaults to
    /// `false` on a happy-path parse, and can be distinguished from a
    /// genuinely-truncated scene by downstream consumers. The full
    /// end-to-end "Oblivion block parser errors mid-file" path is
    /// exercised by the ignored `parse_rate_oblivion` integration test
    /// against real .nif corpora — this unit test just pins the public
    /// field surface so that a future refactor of the error path can't
    /// silently drop the field.
    #[test]
    fn nif_scene_truncated_flag_defaults_false_on_clean_parse() {
        let data = build_test_nif_with_node();
        let scene = parse_nif(&data).unwrap();
        assert!(
            !scene.truncated,
            "clean parse must not set the truncated flag"
        );
        assert_eq!(scene.len(), 1);
    }

    #[test]
    fn nif_scene_struct_carries_truncated_field() {
        // Hand-constructed marker: verifies the field exists on the
        // struct surface so consumers like `cell_loader` can branch on
        // it without fear of the field being silently removed.
        let scene = NifScene {
            blocks: Vec::new(),
            root_index: None,
            truncated: true,
            dropped_block_count: 3,
            recovered_blocks: 0,
            link_errors: 0,
            drift_histogram: std::collections::BTreeMap::new(),
            stubbed_drift_histogram: std::collections::BTreeMap::new(),
        };
        assert!(scene.truncated);
        assert_eq!(scene.dropped_block_count, 3);
        assert_eq!(scene.recovered_blocks, 0);
        assert_eq!(scene.link_errors, 0);
        assert!(scene.drift_histogram.is_empty());
        assert!(scene.is_empty());
    }

    /// Regression: #568 (SK-D5-06). A NIF whose header advertises a
    /// block type the dispatch table doesn't know lands on
    /// `parse_block`'s unknown-type fallback, which returns
    /// `Ok(NiUnknown)`. Pre-fix the outer loop silently counted that
    /// as a clean parse; the `record_success` path on `nif_stats`
    /// kept the headline rate at 100% and hid under-consuming parser
    /// bugs like #546. Post-fix `NifScene.recovered_blocks` increments
    /// for every such placeholder, and the integration gate routes
    /// these scenes through `record_truncated`.
    #[test]
    fn recovered_blocks_flagged_for_unknown_dispatch_fallback() {
        // Build a minimal NIF whose single block advertises a type
        // name that's NOT in the dispatch table. The parser's
        // dispatch-level unknown-type recovery seeks past via
        // block_size and substitutes an `NiUnknown` placeholder.
        let mut buf = Vec::new();
        buf.extend_from_slice(b"Gamebryo File Format, Version 20.2.0.7\n");
        buf.extend_from_slice(&0x14020007u32.to_le_bytes());
        buf.push(1); // little-endian
        buf.extend_from_slice(&11u32.to_le_bytes()); // user_version (FNV-like)
        buf.extend_from_slice(&1u32.to_le_bytes()); // num_blocks = 1
        buf.extend_from_slice(&34u32.to_le_bytes()); // user_version_2

        // Short strings (author / process / export, each 1 byte empty).
        for _ in 0..3 {
            buf.push(1);
            buf.push(0);
        }

        // 1 block type — "NiImaginaryBlockFromSK-D5-06".
        const UNKNOWN: &str = "NiImaginaryBlockFromSK-D5-06";
        buf.extend_from_slice(&1u16.to_le_bytes());
        buf.extend_from_slice(&(UNKNOWN.len() as u32).to_le_bytes());
        buf.extend_from_slice(UNKNOWN.as_bytes());
        buf.extend_from_slice(&0u16.to_le_bytes()); // block 0 → type 0

        // Block payload: 4 arbitrary bytes the parser will skip.
        let block_payload = [0xAAu8, 0xBB, 0xCC, 0xDD];
        buf.extend_from_slice(&(block_payload.len() as u32).to_le_bytes()); // block_size

        // String table: empty.
        buf.extend_from_slice(&0u32.to_le_bytes()); // num_strings
        buf.extend_from_slice(&0u32.to_le_bytes()); // max_string_length
        buf.extend_from_slice(&0u32.to_le_bytes()); // num_groups

        buf.extend_from_slice(&block_payload);

        let scene = parse_nif(&buf).expect("unknown-type fallback must produce Ok");
        assert_eq!(
            scene.len(),
            1,
            "placeholder block still lives at its original index"
        );
        assert_eq!(
            scene.recovered_blocks, 1,
            "unknown dispatch fallback must bump recovered_blocks"
        );
        assert!(
            !scene.truncated,
            "truncated is reserved for blocks dropped past the abort point"
        );
        assert_eq!(
            scene.blocks[0].block_type_name(),
            "NiUnknown",
            "placeholder is an NiUnknown"
        );
    }

    /// Regression for #611 / SK-D5-02 — `parse_nif` must pick a
    /// NiNode-subclass-rooted scene as root, not skip past it to a
    /// plain-NiNode child. Pre-fix the predicate was the literal
    /// `matches!(block_type_name(), "NiNode")`; this guarantees every
    /// dedicated subclass with its own dispatch arm in `parse_block`
    /// is also recognised.
    ///
    /// The list mirrors the dedicated subclass dispatch arms in
    /// `crate::blocks::parse_block` (around line 144-216). Update both
    /// sites when adding a new NiNode-derived block type.
    #[test]
    fn is_ni_node_subclass_recognises_every_dedicated_subclass() {
        // Plain NiNode + the aliased ones (BSFadeNode, BSLeafAnimNode,
        // RootCollisionNode, AvoidNode, NiBSAnimationNode,
        // NiBSParticleNode) all parse as `NiNode` and report their
        // type as `"NiNode"`, so the single arm covers them all.
        assert!(is_ni_node_subclass("NiNode"));

        // Dedicated subclass parsers — each has its own dispatch arm
        // and reports its own block_type_name. These were the
        // regression surface in #611.
        for name in [
            "BSOrderedNode",
            "BSValueNode",
            "BSMultiBoundNode",
            "BSDistantObjectInstancedNode",
            "BSTreeNode",
            "NiBillboardNode",
            "NiSwitchNode",
            "NiLODNode",
            "NiSortAdjustNode",
            "BSRangeNode",
        ] {
            assert!(
                is_ni_node_subclass(name),
                "{name} must be recognised as a NiNode-subclass for root \
                 selection in is_ni_node_subclass()"
            );
        }

        // Negative controls — block types that are NOT NiNode subclasses.
        // A scene rooted at one of these (extremely unusual) would fall
        // back to `Some(0)` (block at index 0) regardless. None of these
        // should match the helper.
        for name in [
            "NiCamera",
            "NiTriShape",
            "BsTriShape",
            "NiSkinPartition",
            "BSLightingShaderProperty",
            "NiAlphaProperty",
            "NiUnknown",
        ] {
            assert!(
                !is_ni_node_subclass(name),
                "{name} must not be recognised as a NiNode-subclass"
            );
        }
    }

    /// End-to-end regression for #611 / SK-D5-02. The recogniser test
    /// above pins the predicate; this one pins the actual root-pick
    /// against a synthesised NIF that mirrors vanilla Skyrim tree LODs:
    /// `BSTreeNode` at block 0 followed by a plain `NiNode` at block 1.
    /// Pre-fix the predicate matched only the literal `"NiNode"` and
    /// returned `Some(1)`, causing the importer to descend from a leaf
    /// bone container and import 0 of N geometry shapes.
    #[test]
    fn root_pick_prefers_bstreenode_root_over_plain_ninode_child() {
        // NiNode body — same wire layout used by `build_test_nif_with_node`.
        let mut ninode_body = Vec::new();
        ninode_body.extend_from_slice(&(-1i32).to_le_bytes()); // name index — none
        ninode_body.extend_from_slice(&0u32.to_le_bytes()); // extra_data_refs count
        ninode_body.extend_from_slice(&(-1i32).to_le_bytes()); // controller_ref
        ninode_body.extend_from_slice(&0u32.to_le_bytes()); // flags (u32 @ v20.2.0.7)
        ninode_body.extend_from_slice(&0.0f32.to_le_bytes()); // tx
        ninode_body.extend_from_slice(&0.0f32.to_le_bytes()); // ty
        ninode_body.extend_from_slice(&0.0f32.to_le_bytes()); // tz
        for r in &[1.0f32, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0] {
            ninode_body.extend_from_slice(&r.to_le_bytes());
        }
        ninode_body.extend_from_slice(&1.0f32.to_le_bytes()); // scale
        ninode_body.extend_from_slice(&0u32.to_le_bytes()); // properties count
        ninode_body.extend_from_slice(&(-1i32).to_le_bytes()); // collision_ref
        ninode_body.extend_from_slice(&0u32.to_le_bytes()); // children count
        ninode_body.extend_from_slice(&0u32.to_le_bytes()); // effects count

        // BSTreeNode = NiNode body + two empty bone-ref lists.
        let mut bstreenode_body = ninode_body.clone();
        bstreenode_body.extend_from_slice(&0u32.to_le_bytes()); // num_bones_1
        bstreenode_body.extend_from_slice(&0u32.to_le_bytes()); // num_bones_2

        let mut buf = Vec::new();
        // Header — FNV-style configuration (user_version_2 = bsver = 34).
        // The #611 root-pick bug is bsver-agnostic, but matching FNV keeps
        // the wire layout aligned with `build_test_nif_with_node` (the
        // properties list at bsver<=34 is part of NiAVObject body).
        buf.extend_from_slice(b"Gamebryo File Format, Version 20.2.0.7\n");
        buf.extend_from_slice(&0x14020007u32.to_le_bytes());
        buf.push(1); // little-endian
        buf.extend_from_slice(&11u32.to_le_bytes()); // user_version (FNV)
        buf.extend_from_slice(&2u32.to_le_bytes()); // num_blocks = 2
        buf.extend_from_slice(&34u32.to_le_bytes()); // user_version_2 (FNV)

        // Three short strings (author / process / export, empty)
        for _ in 0..3 {
            buf.push(1);
            buf.push(0);
        }

        // Block types: 2 — "BSTreeNode" (idx 0), "NiNode" (idx 1)
        buf.extend_from_slice(&2u16.to_le_bytes());
        buf.extend_from_slice(&10u32.to_le_bytes());
        buf.extend_from_slice(b"BSTreeNode");
        buf.extend_from_slice(&6u32.to_le_bytes());
        buf.extend_from_slice(b"NiNode");

        // Block type indices: block 0 → type 0 (BSTreeNode), block 1 → type 1 (NiNode)
        buf.extend_from_slice(&0u16.to_le_bytes());
        buf.extend_from_slice(&1u16.to_le_bytes());

        // Block sizes
        buf.extend_from_slice(&(bstreenode_body.len() as u32).to_le_bytes());
        buf.extend_from_slice(&(ninode_body.len() as u32).to_le_bytes());

        // String table — empty
        buf.extend_from_slice(&0u32.to_le_bytes()); // num_strings
        buf.extend_from_slice(&0u32.to_le_bytes()); // max_string_length
        buf.extend_from_slice(&0u32.to_le_bytes()); // num_groups

        // Block data
        buf.extend_from_slice(&bstreenode_body);
        buf.extend_from_slice(&ninode_body);

        let scene = parse_nif(&buf).expect("two-block scene must parse cleanly");
        assert_eq!(scene.len(), 2, "both blocks landed in the scene");
        assert_eq!(
            scene.root_index,
            Some(0),
            "root must be the BSTreeNode at block 0, not the plain NiNode at block 1"
        );
        let root = scene.root().expect("root must resolve");
        assert_eq!(
            root.block_type_name(),
            "BSTreeNode",
            "root_index points at a NiNode subclass, not the trailing plain NiNode"
        );
    }

    #[test]
    fn parse_nif_minimal_node() {
        let data = build_test_nif_with_node();
        let scene = parse_nif(&data).unwrap();

        assert_eq!(scene.len(), 1);
        assert_eq!(scene.root_index, Some(0));

        let root = scene.root().unwrap();
        assert_eq!(root.block_type_name(), "NiNode");

        // Downcast and verify fields
        let node = scene.get_as::<blocks::node::NiNode>(0).unwrap();
        assert_eq!(node.av.net.name.as_deref(), Some("SceneRoot"));
        assert_eq!(node.av.flags, 14);
        assert_eq!(node.av.transform.translation.x, 1.0);
        assert_eq!(node.av.transform.translation.y, 2.0);
        assert_eq!(node.av.transform.translation.z, 3.0);
        assert_eq!(node.av.transform.scale, 1.0);
        assert!(node.children.is_empty());
        assert!(node.effects.is_empty());
        assert!(node.av.net.controller_ref.is_null());
        assert!(node.av.collision_ref.is_null());
    }

    #[test]
    fn parse_nif_empty_file() {
        // Build a NIF with 0 blocks
        let mut buf = Vec::new();
        buf.extend_from_slice(b"Gamebryo File Format, Version 20.2.0.7\n");
        buf.extend_from_slice(&0x14020007u32.to_le_bytes());
        buf.push(1);
        buf.extend_from_slice(&12u32.to_le_bytes()); // user_version
        buf.extend_from_slice(&0u32.to_le_bytes()); // num_blocks = 0
        buf.extend_from_slice(&83u32.to_le_bytes()); // user_version_2

        buf.push(1);
        buf.push(0); // author
        buf.push(1);
        buf.push(0); // process
        buf.push(1);
        buf.push(0); // export

        buf.extend_from_slice(&0u16.to_le_bytes()); // num_block_types
        buf.extend_from_slice(&0u32.to_le_bytes()); // num_strings
        buf.extend_from_slice(&0u32.to_le_bytes()); // max_string_length
        buf.extend_from_slice(&0u32.to_le_bytes()); // num_groups

        let scene = parse_nif(&buf).unwrap();
        assert!(scene.is_empty());
        assert_eq!(scene.root_index, None);
    }

    #[test]
    fn parse_nif_unknown_block_skipped() {
        let mut buf = Vec::new();
        buf.extend_from_slice(b"Gamebryo File Format, Version 20.2.0.7\n");
        buf.extend_from_slice(&0x14020007u32.to_le_bytes());
        buf.push(1);
        buf.extend_from_slice(&12u32.to_le_bytes());
        buf.extend_from_slice(&1u32.to_le_bytes()); // 1 block
        buf.extend_from_slice(&83u32.to_le_bytes());

        buf.push(1);
        buf.push(0);
        buf.push(1);
        buf.push(0);
        buf.push(1);
        buf.push(0);

        // 1 block type: "BSUnknownFutureType"
        buf.extend_from_slice(&1u16.to_le_bytes());
        buf.extend_from_slice(&19u32.to_le_bytes());
        buf.extend_from_slice(b"BSUnknownFutureType");

        // Block 0 → type 0
        buf.extend_from_slice(&0u16.to_le_bytes());

        // Block size: 8 bytes of dummy data
        buf.extend_from_slice(&8u32.to_le_bytes());

        // String table: 0 strings
        buf.extend_from_slice(&0u32.to_le_bytes());
        buf.extend_from_slice(&0u32.to_le_bytes());

        // num_groups = 0
        buf.extend_from_slice(&0u32.to_le_bytes());

        // Block data: 8 bytes of garbage
        buf.extend_from_slice(&[0xDE, 0xAD, 0xBE, 0xEF, 0xCA, 0xFE, 0xBA, 0xBE]);

        let scene = parse_nif(&buf).unwrap();
        assert_eq!(scene.len(), 1);
        // Unknown block is preserved as NiUnknown
        assert_eq!(scene.get(0).unwrap().block_type_name(), "NiUnknown");
    }

    #[test]
    fn scene_get_as_wrong_type_returns_none() {
        let data = build_test_nif_with_node();
        let scene = parse_nif(&data).unwrap();

        // Block 0 is NiNode, not NiTriShape
        let result = scene.get_as::<blocks::tri_shape::NiTriShape>(0);
        assert!(result.is_none());
    }

    /// Build a minimal Oblivion (v20.0.0.5) NIF with `num_unknown` blocks of
    /// a registered unknown type, each `payload_size` bytes of garbage.
    /// v20.0.0.5 has no `block_sizes` table and no string table, which is
    /// exactly the configuration that exercises the `oblivion_skip_sizes`
    /// recovery path in the main parse loop.
    fn build_oblivion_nif_with_unknowns(
        type_name: &str,
        num_unknown: usize,
        payload_size: usize,
    ) -> Vec<u8> {
        let mut buf = Vec::new();

        // ASCII header line.
        buf.extend_from_slice(b"Gamebryo File Format, Version 20.0.0.5\n");

        // Binary header.
        buf.extend_from_slice(&0x14000005u32.to_le_bytes()); // version
        buf.push(1); // little_endian
        buf.extend_from_slice(&11u32.to_le_bytes()); // user_version (Oblivion)
        buf.extend_from_slice(&(num_unknown as u32).to_le_bytes()); // num_blocks

        // BSStreamHeader (triggered by user_version >= 3).
        buf.extend_from_slice(&11u32.to_le_bytes()); // user_version_2
        buf.push(0); // author short_string: length 0
        buf.push(0); // process_script (user_version_2 < 131)
        buf.push(0); // export_script

        // Block types table.
        buf.extend_from_slice(&1u16.to_le_bytes()); // num_block_types
        buf.extend_from_slice(&(type_name.len() as u32).to_le_bytes());
        buf.extend_from_slice(type_name.as_bytes());

        // Block type indices — all blocks point at type 0.
        for _ in 0..num_unknown {
            buf.extend_from_slice(&0u16.to_le_bytes());
        }

        // No block_sizes (version < 20.2.0.7).
        // No string table (version < 20.1.0.1).

        // num_groups = 0.
        buf.extend_from_slice(&0u32.to_le_bytes());

        // Block data: each block is `payload_size` bytes of 0xAB.
        for _ in 0..num_unknown {
            buf.extend(std::iter::repeat(0xABu8).take(payload_size));
        }

        buf
    }

    /// Regression test for issue #224: on Oblivion NIFs (no block_sizes) the
    /// caller can register `oblivion_skip_sizes` hints that let the parser
    /// skip past unknown block types instead of truncating the scene.
    #[test]
    fn oblivion_skip_sizes_hint_recovers_unknown_blocks() {
        let type_name = "BSUnknownOblivionSkipTest";
        let payload = 24;
        let data = build_oblivion_nif_with_unknowns(type_name, 3, payload);

        // Default options: no hints → parse should truncate after the first
        // failing block, keeping 0 blocks.
        let default_scene = parse_nif(&data).unwrap();
        assert!(
            default_scene.truncated,
            "unknown-type Oblivion NIF must truncate without a hint"
        );
        assert_eq!(default_scene.dropped_block_count, 3);
        assert!(default_scene.blocks.is_empty());

        // With a registered hint the parser should skip past all 3 blocks.
        let mut options = ParseOptions::default();
        options
            .oblivion_skip_sizes
            .insert(type_name.to_string(), payload as u32);
        let scene = parse_nif_with_options(&data, &options).unwrap();

        assert!(!scene.truncated, "hint must prevent truncation");
        assert_eq!(scene.dropped_block_count, 0);
        assert_eq!(scene.len(), 3);
        for i in 0..3 {
            assert_eq!(scene.get(i).unwrap().block_type_name(), "NiUnknown");
        }
    }

    /// A too-large hint (past EOF) must NOT crash or advance the stream —
    /// the parser falls back to the truncation path gracefully.
    #[test]
    fn oblivion_skip_sizes_oversized_hint_falls_back_to_truncation() {
        let type_name = "BSUnknownOblivionOversize";
        let data = build_oblivion_nif_with_unknowns(type_name, 1, 16);

        let mut options = ParseOptions::default();
        // Hint is 9999 bytes but the payload is only 16 — skip would go
        // past EOF, so the parser should log a warning and truncate.
        options
            .oblivion_skip_sizes
            .insert(type_name.to_string(), 9999);
        let scene = parse_nif_with_options(&data, &options).unwrap();

        assert!(scene.truncated);
        assert_eq!(scene.dropped_block_count, 1);
        assert!(scene.blocks.is_empty());
    }

    /// Regression test for #324: Oblivion NIFs (no block_sizes) recover
    /// from a corrupted block using the runtime size cache built from
    /// earlier successful parses of the same type.
    #[test]
    fn oblivion_runtime_size_cache_recovers_corrupted_block() {
        // Build an Oblivion-style NIF (v20.0.0.5, no block_sizes) with
        // 3 NiNode blocks. Block 0 and 2 are valid; block 1 is truncated
        // (data too short → parse error). The runtime cache should learn
        // the NiNode size from block 0, use it to skip block 1, and
        // successfully parse block 2.
        let mut buf = Vec::new();

        // ── Header (Oblivion v20.0.0.5) ────────────────────────────
        buf.extend_from_slice(b"Gamebryo File Format, Version 20.0.0.5\n");
        buf.extend_from_slice(&0x14000005u32.to_le_bytes()); // version
        buf.push(1); // little-endian
        buf.extend_from_slice(&11u32.to_le_bytes()); // user_version
        buf.extend_from_slice(&3u32.to_le_bytes()); // num_blocks = 3
        buf.extend_from_slice(&21u32.to_le_bytes()); // user_version_2

        // Short strings
        buf.push(1);
        buf.push(0);
        buf.push(1);
        buf.push(0);
        buf.push(1);
        buf.push(0);

        // Block types: 1 type "NiNode"
        buf.extend_from_slice(&1u16.to_le_bytes());
        buf.extend_from_slice(&6u32.to_le_bytes());
        buf.extend_from_slice(b"NiNode");

        // Block type indices: all 3 blocks → type 0
        buf.extend_from_slice(&0u16.to_le_bytes());
        buf.extend_from_slice(&0u16.to_le_bytes());
        buf.extend_from_slice(&0u16.to_le_bytes());

        // NO block_sizes (v20.0.0.5 < 20.2.0.5 threshold).
        // NO string table (v20.0.0.5 < 20.1.0.1 threshold).
        // num_groups (v >= 5.0.0.6)
        buf.extend_from_slice(&0u32.to_le_bytes());

        // ── Build a valid NiNode block (v20.0.0.5 layout) ─────────
        fn build_ninode_block() -> Vec<u8> {
            let mut b = Vec::new();
            // NiObjectNET: name (u32 length-prefixed string, 0 = empty)
            b.extend_from_slice(&0u32.to_le_bytes());
            // extra_data_refs: count=0
            b.extend_from_slice(&0u32.to_le_bytes());
            // controller_ref: -1
            b.extend_from_slice(&(-1i32).to_le_bytes());
            // NiAVObject: flags (u16 for v20.0.0.5)
            b.extend_from_slice(&14u16.to_le_bytes());
            // translation
            b.extend_from_slice(&0.0f32.to_le_bytes());
            b.extend_from_slice(&0.0f32.to_le_bytes());
            b.extend_from_slice(&0.0f32.to_le_bytes());
            // rotation (3×3 identity)
            for &v in &[1.0f32, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0] {
                b.extend_from_slice(&v.to_le_bytes());
            }
            // scale
            b.extend_from_slice(&1.0f32.to_le_bytes());
            // properties: count=0
            b.extend_from_slice(&0u32.to_le_bytes());
            // collision_ref: -1
            b.extend_from_slice(&(-1i32).to_le_bytes());
            // NiNode: children count=0
            b.extend_from_slice(&0u32.to_le_bytes());
            // effects count=0
            b.extend_from_slice(&0u32.to_le_bytes());
            b
        }

        let good_block = build_ninode_block();
        let block_len = good_block.len();

        // Block 0: valid
        buf.extend_from_slice(&good_block);

        // Block 1: corrupted — write a huge string length (0xDEADBEEF)
        // as the first field (name), which will fail with an I/O error
        // when read_string tries to read 3.7 billion bytes. The rest is
        // valid block data padded to block_len so the cache skip lands
        // at the correct offset for block 2.
        buf.extend_from_slice(&0xDEADBEEFu32.to_le_bytes()); // poison name length
        buf.extend_from_slice(&vec![0xAA; block_len - 4]);

        // Block 2: valid
        buf.extend_from_slice(&good_block);

        let scene = parse_nif(&buf).unwrap();

        // Block 0 parsed successfully, block 1 recovered via cache → NiUnknown,
        // block 2 parsed successfully. No truncation.
        assert!(
            !scene.truncated,
            "scene should NOT be truncated — cache recovery should work"
        );
        assert_eq!(scene.len(), 3, "all 3 blocks should be present");
        assert_eq!(scene.blocks[0].block_type_name(), "NiNode");
        assert_eq!(scene.blocks[1].block_type_name(), "NiUnknown");
        assert_eq!(scene.blocks[2].block_type_name(), "NiNode");
    }

    // Real-game NIF parse coverage lives in `tests/parse_real_nifs.rs`, which
    // walks entire mesh archives and asserts a per-game success-rate threshold.
    // The old /tmp-based single-file smoke tests were removed in N23.10.

    // ── #395: stream-position drift detector ─────────────────────────

    #[test]
    fn drift_warning_silent_with_too_few_samples() {
        // Need at least two prior samples to characterise the type.
        assert!(super::drift_warning(100, &[]).is_none());
        assert!(super::drift_warning(100, &[42]).is_none());
    }

    #[test]
    fn drift_warning_silent_when_consumed_matches_cache() {
        // Fixed-size type, new sample matches → no fire.
        assert!(super::drift_warning(48, &[48, 48, 48]).is_none());
        // Within ±2 byte tolerance — still considered a match.
        assert!(super::drift_warning(50, &[48, 48, 48]).is_none());
        assert!(super::drift_warning(46, &[48, 48, 48]).is_none());
    }

    #[test]
    fn drift_warning_silent_for_high_variance_types() {
        // NiTriShapeData / NiSkinData / NiNode-with-children all have
        // wildly varying consumed sizes legitimately. The detector
        // recognises this from the cache spread (> 2 bytes) and stays
        // silent regardless of the new sample.
        let prior = [40, 200, 1024];
        assert!(super::drift_warning(48, &prior).is_none());
        assert!(super::drift_warning(99999, &prior).is_none());
    }

    #[test]
    fn drift_warning_fires_on_fixed_size_disagreement() {
        // Cache has 3 prior samples all = 48 (clearly a fixed-size
        // type). New sample 68 differs by 20 — > 2 byte tolerance,
        // unambiguous drift.
        let msg = super::drift_warning(68, &[48, 48, 48])
            .expect("drift warning should fire on +20 byte deviation from fixed-size cache");
        assert!(
            msg.contains("consumed 68 bytes"),
            "warning must report the offending consumed count, got: {msg}"
        );
        assert!(
            msg.contains("median 48"),
            "warning must report the cached median, got: {msg}"
        );
        assert!(
            msg.contains("3 prior parse(s)"),
            "warning must report sample count, got: {msg}"
        );
    }

    #[test]
    fn drift_warning_fires_on_short_consumed_too() {
        // Drift can be backward as well as forward — a parser that
        // under-consumed leaves bytes for the next reader to overshoot.
        let msg = super::drift_warning(40, &[48, 48, 48])
            .expect("drift warning should fire on -8 byte deviation");
        assert!(msg.contains("consumed 40 bytes"));
    }

    #[test]
    fn drift_warning_uses_min_distance_not_first_sample() {
        // Cache has slight variance ([46, 47, 48], range = 2 → still
        // considered fixed-size). New sample 50 is 2 away from 48
        // (within tolerance) → no fire. New sample 60 is 12 away from
        // the closest sample (48) → fire.
        assert!(super::drift_warning(50, &[46, 47, 48]).is_none());
        assert!(super::drift_warning(60, &[46, 47, 48]).is_some());
    }

    // ── #939: per-block-type drift histogram ─────────────────────────

    /// A known-good NIF must produce an empty drift histogram —
    /// `block_size` matches `consumed` for every block, so the
    /// reconciliation branch never fires.
    #[test]
    fn drift_histogram_empty_on_clean_parse() {
        let data = build_test_nif_with_node();
        let scene = parse_nif(&data).expect("clean parse");
        assert!(
            scene.drift_histogram.is_empty(),
            "clean parse must produce an empty drift histogram, got: {:?}",
            scene.drift_histogram
        );
    }

    /// Build a Skyrim-SE-style NIF with one NiNode whose header-declared
    /// `block_size` is intentionally `inflate_by` bytes larger than the
    /// parser actually consumes. The parser returns `Ok`, the drift
    /// reconciliation branch fires, and `scene.drift_histogram["NiNode"]`
    /// ends up with one entry at `drift = +inflate_by`. Used by the
    /// synthetic-drift regression test below.
    fn build_drifted_nif(inflate_by: u32) -> Vec<u8> {
        let mut buf = Vec::new();

        // Header — same layout as build_test_nif_with_node.
        buf.extend_from_slice(b"Gamebryo File Format, Version 20.2.0.7\n");
        buf.extend_from_slice(&0x14020007u32.to_le_bytes());
        buf.push(1); // little-endian
        buf.extend_from_slice(&11u32.to_le_bytes()); // user_version (FNV-style)
        buf.extend_from_slice(&1u32.to_le_bytes()); // num_blocks = 1
        buf.extend_from_slice(&34u32.to_le_bytes()); // user_version_2

        // Short strings.
        for _ in 0..3 {
            buf.push(1);
            buf.push(0);
        }

        // Block types: 1 type "NiNode".
        buf.extend_from_slice(&1u16.to_le_bytes());
        buf.extend_from_slice(&6u32.to_le_bytes());
        buf.extend_from_slice(b"NiNode");

        // Block type indices: block 0 → type 0.
        buf.extend_from_slice(&0u16.to_le_bytes());

        // NiNode body (same wire layout as build_test_nif_with_node).
        let mut block = Vec::new();
        block.extend_from_slice(&0i32.to_le_bytes()); // name index
        block.extend_from_slice(&0u32.to_le_bytes()); // extra_data_refs count
        block.extend_from_slice(&(-1i32).to_le_bytes()); // controller_ref
        block.extend_from_slice(&14u32.to_le_bytes()); // flags (u32 @ v20.2.0.7)
        block.extend_from_slice(&1.0f32.to_le_bytes());
        block.extend_from_slice(&2.0f32.to_le_bytes());
        block.extend_from_slice(&3.0f32.to_le_bytes());
        for r in &[1.0f32, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0] {
            block.extend_from_slice(&r.to_le_bytes());
        }
        block.extend_from_slice(&1.0f32.to_le_bytes()); // scale
        block.extend_from_slice(&0u32.to_le_bytes()); // properties count
        block.extend_from_slice(&(-1i32).to_le_bytes()); // collision_ref
        block.extend_from_slice(&0u32.to_le_bytes()); // children count
        block.extend_from_slice(&0u32.to_le_bytes()); // effects count

        // Block sizes — declared = actual + inflate_by, so the drift
        // reconciliation branch fires with drift = +inflate_by.
        let declared = block.len() as u32 + inflate_by;
        buf.extend_from_slice(&declared.to_le_bytes());

        // String table: 1 string "SceneRoot".
        buf.extend_from_slice(&1u32.to_le_bytes());
        buf.extend_from_slice(&9u32.to_le_bytes());
        buf.extend_from_slice(&9u32.to_le_bytes());
        buf.extend_from_slice(b"SceneRoot");

        // num_groups = 0.
        buf.extend_from_slice(&0u32.to_le_bytes());

        // Block data + tail padding so `stream.set_position(start_pos +
        // declared)` lands within the buffer.
        buf.extend_from_slice(&block);
        buf.extend(std::iter::repeat(0u8).take(inflate_by as usize));

        buf
    }

    #[test]
    fn drift_histogram_records_synthetic_drift() {
        // 10-byte declared-vs-consumed gap on a single NiNode block.
        let data = build_drifted_nif(10);
        let scene = parse_nif(&data).expect("synthetic drift NIF parses");
        let per_type = scene
            .drift_histogram
            .get("NiNode")
            .expect("NiNode drift bucket must exist");
        assert_eq!(
            per_type.get(&10).copied(),
            Some(1),
            "synthetic NIF inflated NiNode block_size by 10 — expected one entry at drift=+10, \
             got histogram: {:?}",
            scene.drift_histogram
        );
        // No other drift entries, single drift event total.
        let total_events: u32 = scene
            .drift_histogram
            .values()
            .flat_map(|inner| inner.values())
            .sum();
        assert_eq!(total_events, 1);
    }

    /// Drift histogram surface is keyed on the public NifScene field —
    /// pin its existence + Default-initialisation so a future refactor
    /// can't silently drop it.
    #[test]
    fn nif_scene_default_carries_empty_drift_histogram() {
        let scene = NifScene::default();
        assert!(scene.drift_histogram.is_empty());
    }
}
