//! `PGRD` — Oblivion's pathgrid, the only navigation format that title
//! ships (#3598).
//!
//! Oblivion authors **zero** `NAVI` and **zero** `NAVM` — the formats this
//! engine already parses — and 8,228 `PGRD` records, which it did not.
//! Every other supported title has navigation and Oblivion had none, so its
//! 7,209 successfully-parsed `PACK` records had no graph for the
//! sandbox/travel procedures to path on.
//!
//! # Layout
//!
//! Measured over all 8,228 `Oblivion.esm` PGRD records rather than taken
//! from a wiki. 8,224 of them are zlib-compressed, which is why a raw byte
//! scan finds only 4 — the census had to decompress first.
//!
//! | Sub-record | Present | Layout, and how it was established |
//! |---|---:|---|
//! | `DATA` | 8,228 | `u16` point count. |
//! | `PGRP` | 8,228 | `points × 16` bytes, **exact on 8,228 of 8,228**. Per point: `x`/`y`/`z` `f32`, `connections` `u8`, 3 pad. |
//! | `PGRR` | 8,181 | Flat `u16` point indices. Length equals `sum(connections) × 2` on **8,181 of 8,181**, which is what identifies it as the intra-cell edge list laid out per point in `PGRP` order. `0xFFFF` is a sentinel meaning "this edge leaves the cell": of 2,887,675 entries, 92,115 are `0xFFFF` and **zero** are otherwise out of range, and all 4,351 grids carrying one also carry a `PGRI` (no exceptions). |
//! | `PGRI` | 6,509 | `n × 16`, exact on 6,509 of 6,509. Per entry: `u16` local point index, 2 pad, then `x`/`y`/`z` `f32` of the connected point in the neighbouring cell. The first field is a `u16`: read as one it is `< point_count` on **290,661 of 290,661** entries, whereas reading it as a `u32` fails on 31,751. |
//! | `PGRL` | 260 | Point-to-reference links. Not decoded — 260 records is too thin a corpus to establish a field layout from, and guessing one would be worse than the honest absence. |
//! | `PGAG` | 8,224 | Opaque blob (Bethesda's own compressed "AG" data). Not decoded. |

use crate::esm::reader::SubRecord;
use crate::esm::sub_reader::SubReader;

/// One pathgrid node.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct PathGridPoint {
    /// World position, raw Gamebryo Z-up (unconverted, like every other
    /// parser output — the Z-up → Y-up flip belongs at the import boundary).
    pub position: [f32; 3],
    /// How many entries this point owns in the `PGRR` edge list.
    pub connection_count: u8,
}

/// A connection from a point in this cell to a position in a neighbouring
/// one (`PGRI`).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct InterCellConnection {
    /// Index into [`PathGridRecord::points`].
    pub point_index: u16,
    /// The connected point's world position in the neighbouring cell, raw
    /// Z-up.
    pub position: [f32; 3],
}

/// A parsed `PGRD` record — one pathgrid, attached to one CELL.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PathGridRecord {
    pub form_id: u32,
    /// `DATA`'s authored point count. Kept alongside `points` because a
    /// truncated or absent `PGRP` must be visible as a disagreement rather
    /// than silently becoming the real count.
    pub declared_point_count: u16,
    pub points: Vec<PathGridPoint>,
    /// Per-point adjacency, in `points` order: `edges[i]` holds what
    /// `points[i]` connects to. `None` is the `0xFFFF` sentinel — the edge
    /// leaves this cell and is resolved through [`Self::inter_cell`].
    /// Modelled as `Option` rather than raw `u16` so a consumer physically
    /// cannot index `points` with `65535`.
    ///
    /// Empty when the record authored no `PGRR` (47 of 8,228 vanilla).
    ///
    /// The sentinel is NOT positionally paired with `inter_cell`: sentinel
    /// count equals `PGRI` entry count on only 538 of the 4,351 grids that
    /// have both, so any 1:1 mapping would be invented. Resolving which
    /// external point an edge reaches is the consumer's problem, with the
    /// positions in `inter_cell` as its input.
    pub edges: Vec<Vec<Option<u16>>>,
    pub inter_cell: Vec<InterCellConnection>,
}

/// Parse a `PGRD` record.
///
/// Soft-fails per sub-record: a `PGRR` whose length disagrees with the
/// declared connection counts yields no edges rather than a mis-sliced
/// graph, and the points survive either way. A pathgrid with nodes and no
/// edges is degraded; one with edges built from a mis-read buffer is wrong.
pub fn parse_pgrd(form_id: u32, subs: &[SubRecord]) -> PathGridRecord {
    let mut out = PathGridRecord {
        form_id,
        ..Default::default()
    };
    let mut pgrr: Option<&[u8]> = None;

    for sub in subs {
        match &sub.sub_type {
            b"DATA" if sub.data.len() >= 2 => {
                out.declared_point_count = SubReader::new(&sub.data).u16_or_default();
            }
            b"PGRP" => {
                for chunk in sub.data.chunks_exact(16) {
                    let mut r = SubReader::new(chunk);
                    let x = r.f32_or_default();
                    let y = r.f32_or_default();
                    let z = r.f32_or_default();
                    out.points.push(PathGridPoint {
                        position: [x, y, z],
                        connection_count: r.u8_or_default(),
                    });
                }
            }
            // Deferred: the edge slicing needs `PGRP`'s per-point counts,
            // and sub-record order is not guaranteed.
            b"PGRR" => pgrr = Some(&sub.data),
            b"PGRI" => {
                for chunk in sub.data.chunks_exact(16) {
                    let mut r = SubReader::new(chunk);
                    let point_index = r.u16_or_default();
                    let _pad = r.u16_or_default();
                    let x = r.f32_or_default();
                    let y = r.f32_or_default();
                    let z = r.f32_or_default();
                    out.inter_cell.push(InterCellConnection {
                        point_index,
                        position: [x, y, z],
                    });
                }
            }
            // `PGRL` (260 records) and `PGAG` (8,224) are not decoded — see
            // the module docs.
            _ => {}
        }
    }

    if let Some(data) = pgrr {
        out.edges = slice_edges(&out.points, data);
    }
    out
}

/// The `PGRR` value meaning "this edge leaves the cell". Established
/// exhaustively: of 2,887,675 vanilla entries, 92,115 are `0xFFFF` and zero
/// are out of range for any other reason.
const EDGE_LEAVES_CELL: u16 = 0xFFFF;

/// Slice `PGRR`'s flat `u16` array into per-point adjacency using each
/// point's own `connection_count`.
///
/// Returns empty when the buffer doesn't match the declared total —
/// measured to hold on 8,181 of 8,181 vanilla records that author `PGRR`,
/// so a mismatch means the record is malformed and slicing it anyway would
/// invent edges between the wrong nodes.
fn slice_edges(points: &[PathGridPoint], data: &[u8]) -> Vec<Vec<Option<u16>>> {
    let declared: usize = points.iter().map(|p| p.connection_count as usize).sum();
    if data.len() != declared * 2 {
        return Vec::new();
    }
    let mut edges = Vec::with_capacity(points.len());
    let mut offset = 0usize;
    for point in points {
        let n = point.connection_count as usize;
        let mut list = Vec::with_capacity(n);
        for k in 0..n {
            let at = offset + k * 2;
            let raw = u16::from_le_bytes([data[at], data[at + 1]]);
            list.push((raw != EDGE_LEAVES_CELL).then_some(raw));
        }
        offset += n * 2;
        edges.push(list);
    }
    edges
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sub(code: &[u8; 4], data: Vec<u8>) -> SubRecord {
        SubRecord {
            sub_type: *code,
            data,
        }
    }

    fn point(x: f32, y: f32, z: f32, conns: u8) -> Vec<u8> {
        let mut v = Vec::with_capacity(16);
        v.extend_from_slice(&x.to_le_bytes());
        v.extend_from_slice(&y.to_le_bytes());
        v.extend_from_slice(&z.to_le_bytes());
        v.push(conns);
        v.extend_from_slice(&[0, 0, 0]);
        v
    }

    /// The whole record: three points in a chain, 0<->1<->2.
    #[test]
    fn a_pathgrid_parses_points_and_per_point_edges() {
        let mut pgrp = point(1.0, 2.0, 3.0, 1);
        pgrp.extend(point(4.0, 5.0, 6.0, 2));
        pgrp.extend(point(7.0, 8.0, 9.0, 1));
        // PGRR is flat, laid out per point in PGRP order: [1], [0, 2], [1].
        let pgrr: Vec<u8> = [1u16, 0, 2, 1].iter().flat_map(|v| v.to_le_bytes()).collect();

        let record = parse_pgrd(
            0x0001_0000,
            &[
                sub(b"DATA", 3u16.to_le_bytes().to_vec()),
                sub(b"PGRP", pgrp),
                sub(b"PGRR", pgrr),
            ],
        );

        assert_eq!(record.declared_point_count, 3);
        assert_eq!(record.points.len(), 3);
        assert_eq!(record.points[1].position, [4.0, 5.0, 6.0]);
        assert_eq!(
            record.edges,
            vec![vec![Some(1)], vec![Some(0), Some(2)], vec![Some(1)]]
        );
    }

    /// `PGRR` is deferred until `PGRP` is known, because sub-record order is
    /// not guaranteed and the slicing needs the per-point counts.
    #[test]
    fn pgrr_before_pgrp_still_slices_correctly() {
        let mut pgrp = point(0.0, 0.0, 0.0, 2);
        pgrp.extend(point(1.0, 0.0, 0.0, 0));
        let pgrr: Vec<u8> = [1u16, 1].iter().flat_map(|v| v.to_le_bytes()).collect();

        let record = parse_pgrd(
            1,
            &[sub(b"PGRR", pgrr), sub(b"PGRP", pgrp), sub(b"DATA", 2u16.to_le_bytes().to_vec())],
        );
        assert_eq!(record.edges, vec![vec![Some(1), Some(1)], vec![]]);
    }

    /// A `PGRR` that disagrees with the declared connection totals must
    /// yield NO edges rather than a mis-sliced graph. The points survive: a
    /// pathgrid with nodes and no edges is degraded, one with edges between
    /// the wrong nodes is wrong.
    #[test]
    fn a_mismatched_pgrr_yields_no_edges_but_keeps_the_points() {
        let pgrp = point(0.0, 0.0, 0.0, 4); // claims 4 connections
        let pgrr: Vec<u8> = [1u16].iter().flat_map(|v| v.to_le_bytes()).collect(); // supplies 1

        let record = parse_pgrd(1, &[sub(b"PGRP", pgrp), sub(b"PGRR", pgrr)]);
        assert_eq!(record.points.len(), 1);
        assert!(record.edges.is_empty());
    }

    /// `PGRI`'s leading field is a `u16` plus 2 pad, not a `u32` — read as a
    /// `u32` it exceeds the point count on 31,751 of 290,661 vanilla
    /// entries, and as a `u16` it never does.
    #[test]
    fn inter_cell_entries_read_a_u16_point_index_then_two_pad_bytes() {
        let mut pgri = Vec::new();
        pgri.extend_from_slice(&52u16.to_le_bytes());
        pgri.extend_from_slice(&0xFFFFu16.to_le_bytes()); // pad — must be ignored
        pgri.extend_from_slice(&8448.0f32.to_le_bytes());
        pgri.extend_from_slice(&30720.0f32.to_le_bytes());
        pgri.extend_from_slice(&280.0f32.to_le_bytes());

        let record = parse_pgrd(1, &[sub(b"PGRI", pgri)]);
        assert_eq!(record.inter_cell.len(), 1);
        assert_eq!(record.inter_cell[0].point_index, 52);
        assert_eq!(record.inter_cell[0].position, [8448.0, 30720.0, 280.0]);
    }

    /// `0xFFFF` means "this edge leaves the cell", not point 65535. Of
    /// 2,887,675 vanilla `PGRR` entries, 92,115 are the sentinel and ZERO
    /// are out of range for any other reason; every one of the 4,351 grids
    /// carrying a sentinel also carries a `PGRI`.
    #[test]
    fn the_edge_sentinel_is_not_a_point_index() {
        let pgrp = point(0.0, 0.0, 0.0, 2);
        let pgrr: Vec<u8> = [0xFFFFu16, 0]
            .iter()
            .flat_map(|v| v.to_le_bytes())
            .collect();

        let record = parse_pgrd(1, &[sub(b"PGRP", pgrp), sub(b"PGRR", pgrr)]);
        assert_eq!(
            record.edges,
            vec![vec![None, Some(0)]],
            "0xFFFF must decode as `leaves the cell`, never as point 65535"
        );
    }

    /// A record with no `PGRR` at all (47 of 8,228 vanilla) is valid: nodes,
    /// no intra-cell edges.
    #[test]
    fn a_pathgrid_without_pgrr_keeps_its_points() {
        let record = parse_pgrd(
            1,
            &[
                sub(b"DATA", 1u16.to_le_bytes().to_vec()),
                sub(b"PGRP", point(1.0, 1.0, 1.0, 0)),
            ],
        );
        assert_eq!(record.points.len(), 1);
        assert!(record.edges.is_empty());
    }
}
