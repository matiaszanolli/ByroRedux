//! FUZE (`.fuz`) container parsing (#5912).
//!
//! Skyrim/FO4/FO76 voice lines ship as `.fuz`: a 12-byte `FUZE` header, the raw
//! `.lip` sync bytes, then a complete `RIFF`/`XWMA` stream. The header may
//! declare zero lip bytes. The container layout was verified by
//! `skyrim_fuz_container_shape` (#5464).
//!
//! The voice cache decodes the embedded WMA v2 in process through the
//! statically linked xWMA decoder (#7415). It applies the static PCM limit
//! and records failures for the dialogue caller's subtitle fallback.

/// Static xWMA decoded PCM limit and default voice cache budget (64 MiB).
pub const MAX_PCM_BYTES: usize = 64 * 1024 * 1024;

/// A parsed FUZE container: the `.lip` payload and the whole embedded
/// `RIFF`/`XWMA` image (it is a complete RIFF file of its own).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FuzContainer<'a> {
    /// The `FUZE` header's version field (1 on every observed file).
    pub version: u32,
    /// The raw `.lip` sync bytes — exposed verbatim; no consumer yet
    /// (mouth animation is unstarted), but the range is validated.
    pub lip: &'a [u8],
    /// The complete `RIFF`/`XWMA` audio stream.
    pub xwma: &'a [u8],
}

/// Parse the FUZE container: magic + version + `.lip` size, the `.lip`
/// payload, then the whole remaining tail as the xWMA image (validated
/// to at least open as `RIFF....XWMA`).
pub fn parse_fuz(data: &[u8]) -> Option<FuzContainer<'_>> {
    if data.len() < 16 || &data[0..4] != b"FUZE" {
        return None;
    }
    let version = u32::from_le_bytes(data[4..8].try_into().ok()?);
    let lip_size = u32::from_le_bytes(data[8..12].try_into().ok()?) as usize;
    // The header is exactly 12 bytes; the `.lip` payload follows
    // immediately (its own `01 00 00 00` version word is INSIDE the lip
    // data, not container padding — the empirical probe that pinned the
    // layout put the RIFF at 12 + lip_size and found it there).
    let lip_end = 12usize.checked_add(lip_size)?;
    let lip = data.get(12..lip_end)?;
    let xwma = data.get(lip_end..)?;
    if xwma.len() < 12 || &xwma[0..4] != b"RIFF" || &xwma[8..12] != b"XWMA" {
        return None;
    }
    Some(FuzContainer {
        version,
        lip,
        xwma,
    })
}

/// Extract the xWMA image from a FUZE-marked voice asset. Pass other voice formats through.
pub fn voice_payload(bytes: Vec<u8>) -> std::io::Result<Vec<u8>> {
    if !bytes.starts_with(b"FUZE") {
        return Ok(bytes);
    }
    let container = parse_fuz(&bytes).ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "invalid FUZE container or missing RIFF/XWMA",
        )
    })?;
    Ok(container.xwma.to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn container_preserves_lip_and_inner_stream() {
        let xwma = b"RIFF\x04\0\0\0XWMA";
        let mut fuz = b"FUZE\x01\0\0\0\x09\0\0\0LIPSLIPSY".to_vec();
        fuz.extend_from_slice(xwma);

        let container = parse_fuz(&fuz).expect("container parses");
        assert_eq!(container.version, 1);
        assert_eq!(container.lip, b"LIPSLIPSY");
        assert_eq!(container.xwma, xwma);
    }

    #[test]
    fn malformed_containers_decline() {
        assert_eq!(parse_fuz(b"NOPE"), None, "bad magic");
        assert_eq!(parse_fuz(b"FUZE\x01"), None, "truncated header");
        assert_eq!(
            parse_fuz(b"FUZE\x01\0\0\0\xff\xff\xff\xffRIFF\x04\0\0\0XWMA"),
            None,
            "lip size exceeds input"
        );
        assert_eq!(
            parse_fuz(b"FUZE\x01\0\0\0\0\0\0\0RIFF\x04\0\0\0WAVE"),
            None,
            "non-XWMA form"
        );
    }

    /// A vanilla SE line parses and the shared static loader decodes it.
    #[test]
    fn real_voice_line_parses_and_decodes() {
        let Some(data_dir) = std::env::var("BYROREDUX_SKYRIMSE_DATA")
            .ok()
            .filter(|v| std::path::Path::new(v).is_dir())
            .or_else(|| {
                let fallback = std::path::Path::new(
                    "/mnt/data/SteamLibrary/steamapps/common/Skyrim Special Edition/Data",
                );
                fallback.is_dir().then(|| fallback.to_string_lossy().into_owned())
            })
        else {
            if std::env::var("BYROREDUX_REQUIRE_GAME_DATA").is_ok() {
                panic!("BYROREDUX_REQUIRE_GAME_DATA set but no Skyrim SE data found");
            }
            eprintln!("[fuz] skipping: no Skyrim SE data");
            return;
        };
        let bsa_path = std::path::Path::new(&data_dir).join("Skyrim - Voices_en0.bsa");
        if !bsa_path.is_file() {
            eprintln!("[fuz] skipping: no Voices archive");
            return;
        }
        let archive = byroredux_bsa::BsaArchive::open(&bsa_path).expect("open voices bsa");
        let path = archive
            .list_files()
            .into_iter()
            .find(|f| f.to_ascii_lowercase().ends_with(".fuz"))
            .expect("the voice archive ships .fuz lines");
        let bytes = archive.extract(path).expect("extract");
        let container = parse_fuz(&bytes).expect("vanilla container parses");
        let metadata = byroredux_xwma::Decoder::open(container.xwma)
            .expect("vanilla xWMA parses")
            .metadata();
        let duration = f64::from(metadata.frame_count) / f64::from(metadata.sample_rate);
        assert!(
            duration > 0.2 && duration < 60.0,
            "a dialogue line lasts a sane span, got {duration}s"
        );
        let sound = crate::load_named_sound_from_bytes(path, container.xwma.to_vec())
            .expect("vanilla xWMA decodes");
        assert!(
            (sound.duration().as_secs_f64() - duration).abs() < 0.5,
            "decoded duration {:.3} ~= dpds duration {duration:.3}",
            sound.duration().as_secs_f64()
        );
    }
}
