//! Standalone .jpg / .png image size from the file header (03 §1); pixels are decoded by the renderer.

#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum ImgError {
    #[error("not a png or jpeg image")]
    Format,
    #[error("image header truncated")]
    Truncated,
}

const PNG_MAGIC: [u8; 8] = [0x89, b'P', b'N', b'G', b'\r', b'\n', 0x1A, b'\n'];

fn be16(b: &[u8], at: usize) -> Result<u16, ImgError> {
    b.get(at..at + 2)
        .map(|s| u16::from_be_bytes([s[0], s[1]]))
        .ok_or(ImgError::Truncated)
}

fn be32(b: &[u8], at: usize) -> Result<u32, ImgError> {
    b.get(at..at + 4)
        .map(|s| u32::from_be_bytes([s[0], s[1], s[2], s[3]]))
        .ok_or(ImgError::Truncated)
}

// PNG: signature, then the IHDR chunk (length, "IHDR", width, height; big-endian).
fn png_dims(b: &[u8]) -> Result<(u32, u32), ImgError> {
    if b.get(12..16) != Some(b"IHDR") {
        return Err(ImgError::Truncated);
    }
    Ok((be32(b, 16)?, be32(b, 20)?))
}

// JPEG: walk marker segments after SOI to the first SOFn frame header (precision, height, width; big-endian).
fn jpeg_dims(b: &[u8]) -> Result<(u32, u32), ImgError> {
    let mut at = 2;
    loop {
        if *b.get(at).ok_or(ImgError::Truncated)? != 0xFF {
            return Err(ImgError::Format);
        }
        let m = *b.get(at + 1).ok_or(ImgError::Truncated)?;
        match m {
            // Fill byte before a marker.
            0xFF => at += 1,
            // Standalone markers without a length: TEM, RSTn.
            0x01 | 0xD0..=0xD7 => at += 2,
            // SOF0–SOF15 except DHT (C4), JPG (C8), DAC (CC).
            0xC0..=0xCF if !matches!(m, 0xC4 | 0xC8 | 0xCC) => {
                let h = be16(b, at + 5)?;
                let w = be16(b, at + 7)?;
                return Ok((u32::from(w), u32::from(h)));
            }
            0xD9 | 0xDA => return Err(ImgError::Format),
            _ => at += 2 + usize::from(be16(b, at + 2)?),
        }
    }
}

/// Width and height of a .png or .jpg file (03 §1), read from its header only.
pub fn dims(bytes: &[u8]) -> Result<(u32, u32), ImgError> {
    if bytes.starts_with(&PNG_MAGIC) {
        png_dims(bytes)
    } else if bytes.starts_with(&[0xFF, 0xD8]) {
        jpeg_dims(bytes)
    } else {
        Err(ImgError::Format)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn png_header() {
        let mut b = PNG_MAGIC.to_vec();
        b.extend_from_slice(&13u32.to_be_bytes());
        b.extend_from_slice(b"IHDR");
        b.extend_from_slice(&640u32.to_be_bytes());
        b.extend_from_slice(&480u32.to_be_bytes());
        assert_eq!(dims(&b), Ok((640, 480)));
        assert_eq!(dims(&b[..20]), Err(ImgError::Truncated));
    }

    #[test]
    fn jpeg_header() {
        let mut b = vec![0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x04, 0x4A, 0x46];
        b.extend_from_slice(&[0xFF, 0xFF, 0xC0, 0x00, 0x11, 0x08, 0x01, 0xE0, 0x02, 0x80]);
        assert_eq!(dims(&b), Ok((640, 480)));
        assert_eq!(dims(&[0xFF, 0xD8, 0xFF, 0xD9]), Err(ImgError::Format));
        assert_eq!(dims(b"GIF89a"), Err(ImgError::Format));
    }

    // Every .jpg / .png of th08.dat (03 §1: 37 + 2): header size equals the decoded image's size.
    #[test]
    fn vanilla_images_match_decoder() {
        let Some(ar) = crate::testdata::archive() else {
            return;
        };
        let mut n = 0;
        for name in ar
            .names()
            .iter()
            .filter(|n| n.ends_with(".jpg") || n.ends_with(".png"))
        {
            let b = ar.read(name).unwrap();
            let img = image::load_from_memory(&b).unwrap();
            assert_eq!(dims(&b), Ok((img.width(), img.height())), "{name}");
            n += 1;
        }
        assert_eq!(n, 39);
    }
}
