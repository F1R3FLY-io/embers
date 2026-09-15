//! Deterministic binary assets the stub serves: a constant, valid 1×1 PNG and a
//! minimal, valid PCM WAV whose payload is derived from the request text.
//!
//! Both are built from first principles rather than embedded as opaque base64 so
//! that every byte is auditable and provably well-formed: the PNG is assembled
//! chunk-by-chunk with correct CRC-32 and Adler-32 checksums, and the WAV is a
//! canonical 44-byte RIFF/WAVE header followed by a SHA-256-derived data chunk
//! with internally consistent size fields.

use std::sync::LazyLock;

use sha2::{Digest, Sha256};

/// CRC-32 (ISO-HDLC / "IEEE", reflected, polynomial `0xEDB8_8320`) as required by
/// the PNG specification for every chunk.
fn crc32(bytes: &[u8]) -> u32 {
    let mut crc: u32 = 0xFFFF_FFFF;
    for &byte in bytes {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            // `mask` is all-ones when the low bit is set, all-zeros otherwise, so
            // the polynomial is XORed in exactly on those iterations.
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
        }
    }
    !crc
}

/// Adler-32 checksum, the trailing integrity field of a zlib stream.
fn adler32(bytes: &[u8]) -> u32 {
    const MOD_ADLER: u32 = 65521;
    let mut a: u32 = 1;
    let mut b: u32 = 0;
    for &byte in bytes {
        a = (a + u32::from(byte)) % MOD_ADLER;
        b = (b + a) % MOD_ADLER;
    }
    (b << 16) | a
}

/// Append a PNG chunk (`length`, `type`, `data`, `CRC-32(type ++ data)`).
fn push_png_chunk(out: &mut Vec<u8>, kind: [u8; 4], data: &[u8]) {
    out.extend_from_slice(
        &u32::try_from(data.len())
            .expect("PNG chunk length fits in u32")
            .to_be_bytes(),
    );
    out.extend_from_slice(&kind);
    out.extend_from_slice(data);

    let mut crc_input = Vec::with_capacity(kind.len() + data.len());
    crc_input.extend_from_slice(&kind);
    crc_input.extend_from_slice(data);
    out.extend_from_slice(&crc32(&crc_input).to_be_bytes());
}

/// Canonical PCM parameters for the stub's WAV: mono, 8-bit, 8 kHz.
const WAV_SAMPLE_RATE: u32 = 8_000;
const WAV_CHANNELS: u16 = 1;
const WAV_BITS_PER_SAMPLE: u16 = 8;

/// Build a valid 1×1 opaque-black RGB PNG.
///
/// The single scanline is stored uncompressed inside a zlib "stored" deflate
/// block, which keeps the encoder trivial while remaining spec-compliant.
fn build_one_by_one_png() -> Vec<u8> {
    const SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];

    let mut png = Vec::with_capacity(SIGNATURE.len() + 64);
    png.extend_from_slice(&SIGNATURE);

    // IHDR: 1×1, 8-bit depth, colour type 2 (truecolour RGB), no interlace.
    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend_from_slice(&1u32.to_be_bytes()); // width
    ihdr.extend_from_slice(&1u32.to_be_bytes()); // height
    ihdr.push(8); // bit depth
    ihdr.push(2); // colour type: RGB
    ihdr.push(0); // compression method: deflate
    ihdr.push(0); // filter method: adaptive
    ihdr.push(0); // interlace method: none
    push_png_chunk(&mut png, *b"IHDR", &ihdr);

    // Raw image data before compression: one scanline of a filter byte (0 = None)
    // followed by the RGB triple for a single black pixel.
    let raw_scanline: [u8; 4] = [0, 0, 0, 0];

    // Wrap the raw scanline in a zlib stream containing a single final, stored
    // (uncompressed) deflate block.
    let mut zlib = Vec::with_capacity(11 + raw_scanline.len());
    zlib.extend_from_slice(&[0x78, 0x01]); // zlib header (CMF/FLG), no preset dict
    zlib.push(0x01); // BFINAL = 1, BTYPE = 00 (stored)
    let len = u16::try_from(raw_scanline.len()).expect("scanline length fits in u16");
    zlib.extend_from_slice(&len.to_le_bytes()); // LEN
    zlib.extend_from_slice(&(!len).to_le_bytes()); // NLEN = ones-complement of LEN
    zlib.extend_from_slice(&raw_scanline);
    zlib.extend_from_slice(&adler32(&raw_scanline).to_be_bytes());
    push_png_chunk(&mut png, *b"IDAT", &zlib);

    push_png_chunk(&mut png, *b"IEND", &[]);
    png
}

static ONE_BY_ONE_PNG: LazyLock<Vec<u8>> = LazyLock::new(build_one_by_one_png);

/// The constant, valid 1×1 PNG served for every generated image.
#[must_use]
pub fn one_by_one_png() -> &'static [u8] {
    &ONE_BY_ONE_PNG
}

/// Build a minimal, valid mono 8-bit PCM WAV at 8 kHz.
///
/// The `data` chunk is the SHA-256 digest of `input`; the `RIFF`/`data` size
/// fields are derived from the payload length, so the container is internally
/// consistent for any input. The digest is always 32 bytes, so no size field can
/// overflow.
#[must_use]
pub fn wav_from_input(input: &str) -> Vec<u8> {
    let payload = Sha256::digest(input.as_bytes()).to_vec();
    // The SHA-256 digest is a fixed 32 bytes, so this conversion never saturates.
    let data_len = u32::try_from(payload.len()).unwrap_or(u32::MAX);

    let byte_rate = WAV_SAMPLE_RATE * u32::from(WAV_CHANNELS) * u32::from(WAV_BITS_PER_SAMPLE / 8);
    let block_align = WAV_CHANNELS * (WAV_BITS_PER_SAMPLE / 8);

    let mut wav = Vec::with_capacity(44 + payload.len());
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + data_len).to_le_bytes()); // ChunkSize
    wav.extend_from_slice(b"WAVE");

    wav.extend_from_slice(b"fmt ");
    wav.extend_from_slice(&16u32.to_le_bytes()); // Subchunk1Size (PCM)
    wav.extend_from_slice(&1u16.to_le_bytes()); // AudioFormat = 1 (PCM)
    wav.extend_from_slice(&WAV_CHANNELS.to_le_bytes());
    wav.extend_from_slice(&WAV_SAMPLE_RATE.to_le_bytes());
    wav.extend_from_slice(&byte_rate.to_le_bytes());
    wav.extend_from_slice(&block_align.to_le_bytes());
    wav.extend_from_slice(&WAV_BITS_PER_SAMPLE.to_le_bytes());

    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&data_len.to_le_bytes()); // Subchunk2Size
    wav.extend_from_slice(&payload);
    wav
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn png_has_signature_ihdr_and_iend() {
        let png = one_by_one_png();
        assert_eq!(&png[..8], &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]);
        // IHDR immediately follows the signature; width and height are both 1.
        assert_eq!(&png[12..16], b"IHDR");
        assert_eq!(&png[16..20], &1u32.to_be_bytes());
        assert_eq!(&png[20..24], &1u32.to_be_bytes());
        // The stream ends with a zero-length IEND chunk.
        assert_eq!(&png[png.len() - 8..png.len() - 4], b"IEND");
    }

    #[test]
    fn wav_header_is_internally_consistent() {
        let wav = wav_from_input("hello");
        assert_eq!(&wav[0..4], b"RIFF");
        assert_eq!(&wav[8..12], b"WAVE");
        assert_eq!(&wav[12..16], b"fmt ");
        assert_eq!(&wav[36..40], b"data");

        let riff_size = u32::from_le_bytes(wav[4..8].try_into().expect("riff size"));
        let data_size = u32::from_le_bytes(wav[40..44].try_into().expect("data size"));
        assert_eq!(data_size, 32, "SHA-256 payload is 32 bytes");
        assert_eq!(riff_size, 36 + data_size);
        assert_eq!(u32::try_from(wav.len()).expect("wav length fits u32"), 44 + data_size);
    }

    #[test]
    fn wav_payload_is_input_dependent() {
        assert_ne!(wav_from_input("a"), wav_from_input("b"));
    }
}
