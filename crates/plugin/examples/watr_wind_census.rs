//! Census the leading motion floats of every `WATR` record in an ESM.
//!
//! Evidence harness for the legacy water-layout split. Full FO3/FNV
//! `WATR.DATA` records have an opaque 16-byte prefix; their normal-layer
//! directions/speeds begin at offsets 100/112. Skyrim+ `WATR.DNAM` carries
//! the authored motion fields in its documented tail, while only record-level
//! `NAM0` supplies an explicit linear current.
//!
//! ```text
//! FalloutNV.esm  len=186 n=8   field0=["0.100"]  field1=["90.000"]
//! FalloutNV.esm  len=196 n=69  field0=["90.000"] field1=["0.200","0.500"]
//! Skyrim.esm     len=228 n=31  field0=["90.000"] field1=["0.500"]
//! Oblivion.esm   len=102 n=17  field0=["0.100","5.000","9.000","15.000"]
//!                              field1=["35.000","62.000","90.000","100.000"]
//! ```
//!
//! The old offset-0/4 interpretation was incorrect for full FO3/FNV records;
//! the parser now ignores that opaque prefix and reads the actual per-layer
//! motion fields. `WaterFlow::speed` remains separate: a WATR normal-layer
//! speed is visual scroll, not a physics current.
//!
//! Usage:
//!   cargo run -p byroredux-plugin --example watr_wind_census -- <file.esm> [...]

use byroredux_plugin::esm::reader::EsmReader;

/// One record: `(editor_id, payload_len, field0, field1)`.
type WatrRow = (String, usize, f32, f32);

fn main() -> anyhow::Result<()> {
    for path in std::env::args().skip(1) {
        let bytes = std::fs::read(&path)?;
        let mut reader = EsmReader::new(&bytes);
        let _header = reader.read_file_header()?;
        let mut rows: Vec<WatrRow> = Vec::new();
        walk_groups(&mut reader, bytes.len(), &mut rows)?;

        println!("== {path} — {} WATR records ==", rows.len());
        // Group by payload length: the layouts differ per length, and the
        // whole point is that the field meanings move with it.
        let mut by_len: std::collections::BTreeMap<usize, Vec<(f32, f32)>> = Default::default();
        for (_, len, field0, field1) in &rows {
            by_len.entry(*len).or_default().push((*field0, *field1));
        }
        for (len, values) in &by_len {
            println!(
                "  len={len:<5} n={:<4} field0(parsed as wind_speed)={:?}  \
                 field1(parsed as wind_direction)={:?}",
                values.len(),
                distinct(values.iter().map(|v| v.0)),
                distinct(values.iter().map(|v| v.1)),
            );
        }
    }
    Ok(())
}

fn distinct(values: impl Iterator<Item = f32>) -> Vec<String> {
    let mut out: Vec<String> = values.map(|v| format!("{v:.3}")).collect();
    out.sort();
    out.dedup();
    out
}

fn walk_groups(
    reader: &mut EsmReader<'_>,
    end: usize,
    rows: &mut Vec<WatrRow>,
) -> anyhow::Result<()> {
    while reader.position() < end && reader.remaining() > 0 {
        if reader.is_group() {
            let group = reader.read_group_header()?;
            let inner_end = reader.group_content_end(&group);
            walk_groups(reader, inner_end, rows)?;
            continue;
        }
        let header = reader.read_record_header()?;
        if header.record_type != *b"WATR" {
            reader.skip_record(&header);
            continue;
        }
        let subs = reader.read_sub_records(&header)?;
        let editor_id = subs
            .iter()
            .find(|sub| sub.sub_type == *b"EDID")
            .map(|sub| zstring(&sub.data))
            .unwrap_or_default();
        // Legacy records carry visual data in DATA; Skyrim+ carries it in DNAM.
        let (payload, direction_offset, speed_offset) = match subs
            .iter()
            .find(|sub| sub.sub_type == *b"DATA")
        {
            Some(sub) if sub.data.len() >= 186 => (&sub.data, 100usize, 112usize),
            Some(sub) if sub.data.len() >= 8 => (&sub.data, 4usize, 0usize),
            _ => match subs.iter().find(|sub| sub.sub_type == *b"DNAM") {
                Some(sub) if sub.data.len() >= 124 => (&sub.data, 100usize, 112usize),
                _ => continue,
            },
        };
        let read = |off: usize| {
            f32::from_le_bytes([
                payload[off],
                payload[off + 1],
                payload[off + 2],
                payload[off + 3],
            ])
        };
        rows.push((editor_id, payload.len(), read(speed_offset), read(direction_offset)));
    }
    Ok(())
}

fn zstring(bytes: &[u8]) -> String {
    let end = bytes
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(bytes.len());
    String::from_utf8_lossy(&bytes[..end]).into_owned()
}
