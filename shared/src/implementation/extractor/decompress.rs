// .sav container decompression ported from tools/save-data-cli/node-save-tool/lib.mjs decompressSav.
// Detects and decompresses PlZ (zlib/double-zlib), PlM (Oodle), and CNK (chunked-zlib),
// then verifies the result against the header's uncompressedSize.
// Error strings intentionally match the TypeScript implementation.
use super::gvas::GvasReader;
use std::io::Read;

const MAGIC_PLZ: u32 = 0x005a_6c50;
const MAGIC_PLM: u32 = 0x004d_6c50;
const MAGIC_CNK: u32 = 0x004b_4e43;

/// Maximum decompressed size. uncompressedSize is an untrusted u32 at the start of the file.
/// Reading while the game rewrites Level.sav can observe a corrupt value. Reject it before
/// allocation to avoid a request of up to 4 GiB and an unrecoverable allocation failure.
/// Real saves are only tens of megabytes even for very large worlds.
const MAX_UNCOMPRESSED_BYTES: usize = 512 * 1024 * 1024;

#[derive(Debug)]
pub struct DecompressedSav {
    /// Matches the TypeScript compression field. Currently used only by tests.
    #[allow(dead_code)]
    pub kind: &'static str,
    pub payload: Vec<u8>,
}

/// Reads decompressed output with a limit. Avoids direct read_to_end because highly compressed,
/// corrupt zlib input could otherwise exhaust memory.
fn inflate_zlib(data: &[u8]) -> Result<Vec<u8>, String> {
    let decoder = flate2::read::ZlibDecoder::new(data);
    let mut limited = decoder.take(MAX_UNCOMPRESSED_BYTES as u64 + 1);
    let mut decoded = Vec::new();
    limited
        .read_to_end(&mut decoded)
        .map_err(|error| error.to_string())?;
    if decoded.len() > MAX_UNCOMPRESSED_BYTES {
        return Err(oversize_message());
    }
    Ok(decoded)
}

fn oversize_message() -> String {
    format!("Decompressed payload exceeds the {MAX_UNCOMPRESSED_BYTES} byte limit.")
}

/// Decompresses Oodle input. oozextract can panic instead of returning Err for corrupt input,
/// so convert the panic into a recoverable error that the watch loop can retry.
fn decompress_oodle(payload: &[u8], uncompressed_size: usize) -> Result<Vec<u8>, String> {
    let payload = payload.to_vec();
    let outcome = std::panic::catch_unwind(move || {
        let mut decoded = vec![0u8; uncompressed_size];
        oozextract::Extractor::new()
            .read_from_slice(&payload, &mut decoded)
            .map(|written| {
                decoded.truncate(written);
                decoded
            })
            .map_err(|error| format!("{error:?}"))
    });
    match outcome {
        Ok(result) => result,
        Err(_) => Err("Oodle decompression failed on malformed payload.".to_string()),
    }
}

/// Equivalent to lib.mjs decompressSav. kind is zlib, double-zlib, oodle, or chunked-zlib.
pub fn decompress_sav(sav_bytes: &[u8]) -> Result<DecompressedSav, String> {
    let mut reader = GvasReader::new(sav_bytes);
    let uncompressed_size = reader.read_u32()? as usize;
    let compressed_size = reader.read_u32()? as usize;
    let magic = reader.read_u32()?;
    let magic_bytes = magic & 0x00ff_ffff;
    let save_type = (magic >> 24) & 0xff;
    let payload = reader.read_bytes(compressed_size)?;
    // Validate the header before allocation and decompression to prevent oversized allocations.
    if uncompressed_size > MAX_UNCOMPRESSED_BYTES {
        return Err(oversize_message());
    }

    let (kind, result): (&'static str, Vec<u8>) = if magic_bytes == MAGIC_PLZ && save_type == 0x32 {
        // Match JavaScript: inflate once more when the first result has the wrong size.
        let mut decoded = inflate_zlib(payload)?;
        if decoded.len() != uncompressed_size {
            decoded = inflate_zlib(&decoded)?;
        }
        ("double-zlib", decoded)
    } else if magic_bytes == MAGIC_PLZ && save_type == 0x31 {
        ("zlib", inflate_zlib(payload)?)
    } else if magic_bytes == MAGIC_PLM && save_type == 0x31 {
        ("oodle", decompress_oodle(payload, uncompressed_size)?)
    } else if magic_bytes == MAGIC_CNK && save_type == 0x30 {
        ("chunked-zlib", inflate_zlib(payload)?)
    } else {
        return Err(format!(
            "Unknown Palworld .sav compression: 0x{magic_bytes:x}/0x{save_type:x}"
        ));
    };

    if result.len() != uncompressed_size {
        return Err(format!(
            "Decompressed size mismatch. Header says {uncompressed_size}, got {}.",
            result.len()
        ));
    }

    Ok(DecompressedSav {
        kind,
        payload: result,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::implementation::extractor::gvas::extract_characters_from_gvas;
    use crate::implementation::extractor::gvas::test_fixture::{
        build_level_sav, deflate, expected_standard_characters, standard_world_characters,
        GvasWriter,
    };

    /// Rejects a corrupt oversized uncompressedSize before allocation.
    #[test]
    fn rejects_an_oversized_decompressed_size_before_allocation() {
        let payload = deflate(b"hello");
        let mut writer = GvasWriter::default();
        writer.u32v(u32::MAX);
        writer.u32v(payload.len() as u32);
        writer.u32v(MAGIC_PLZ | (0x31 << 24));
        writer.raw(&payload);
        assert_eq!(
            decompress_sav(&writer.into_bytes()).unwrap_err(),
            oversize_message()
        );
    }

    /// Ensures corrupt Oodle input becomes a recoverable error instead of terminating the agent.
    #[test]
    fn returns_an_error_instead_of_panicking_for_corrupt_oodle_data() {
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(|_| {}));
        let mut writer = GvasWriter::default();
        writer.u32v(4096);
        writer.u32v(64);
        writer.u32v(MAGIC_PLM | (0x31 << 24));
        writer.raw(&[0xa5u8; 64]);
        let result = decompress_sav(&writer.into_bytes());
        std::panic::set_hook(previous);
        assert!(result.is_err(), "corrupt Oodle data must return Err");
    }

    #[test]
    fn decompresses_a_double_zlib_plz_0x32_container() {
        let sav = build_level_sav(&standard_world_characters(), "double-zlib");
        let decompressed = decompress_sav(&sav).unwrap();
        assert_eq!(decompressed.kind, "double-zlib");
        assert_eq!(
            extract_characters_from_gvas(&decompressed.payload).unwrap(),
            expected_standard_characters()
        );
    }

    #[test]
    fn decompresses_a_single_zlib_plz_0x31_container() {
        let sav = build_level_sav(&standard_world_characters(), "zlib");
        let decompressed = decompress_sav(&sav).unwrap();
        assert_eq!(decompressed.kind, "zlib");
        assert_eq!(
            extract_characters_from_gvas(&decompressed.payload).unwrap(),
            expected_standard_characters()
        );
    }

    #[test]
    fn reports_unknown_compression_magic_and_type_in_hexadecimal() {
        let mut writer = GvasWriter::default();
        writer.u32v(10);
        writer.u32v(2);
        writer.u32v(0x9912_3456); // magicBytes=0x123456 / saveType=0x99
        writer.raw(&[0u8; 2]);
        let sav = writer.into_bytes();
        assert_eq!(
            decompress_sav(&sav).unwrap_err(),
            "Unknown Palworld .sav compression: 0x123456/0x99"
        );
    }

    #[test]
    fn rejects_output_whose_size_does_not_match_the_header() {
        let body = b"hello";
        let payload = deflate(body);
        let mut writer = GvasWriter::default();
        writer.u32v(body.len() as u32 + 1); // Make the declared size one byte too large.
        writer.u32v(payload.len() as u32);
        writer.u32v(0x005a_6c50 | (0x31 << 24)); // PlZ / zlib
        writer.raw(&payload);
        let sav = writer.into_bytes();
        assert_eq!(
            decompress_sav(&sav).unwrap_err(),
            "Decompressed size mismatch. Header says 6, got 5."
        );
    }

    #[test]
    fn verifier_temp_corrupt_header_huge_uncompressed_size() {
        // uncompressedSize=0xFFFFFFFF, compressedSize=16, PlM/0x31, garbage payload
        let mut writer = GvasWriter::default();
        writer.u32v(0xFFFF_FFFF);
        writer.u32v(16);
        writer.u32v(0x004d_6c50 | (0x31 << 24));
        writer.raw(&[0xDEu8; 16]);
        let sav = writer.into_bytes();
        let result = decompress_sav(&sav);
        eprintln!("VERIFIER RESULT: {:?}", result.as_ref().err());
        assert!(result.is_err());
    }

    #[test]
    fn rejects_truncated_compressed_data() {
        let sav = build_level_sav(&standard_world_characters(), "double-zlib");
        // Truncate before compressedSize to force a GvasReader EOF error.
        assert_eq!(
            decompress_sav(&sav[..sav.len() - 1]).unwrap_err(),
            "Unexpected end of GVAS payload."
        );
    }
}
