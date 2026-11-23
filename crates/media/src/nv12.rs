use crate::{DecodedPixelFormat, MediaError};

/// Packs positive-stride NV12 planes, omitting row padding. The final row
/// need not include trailing padding. Both dimensions must be even.
pub fn pack_nv12(
    width: u32,
    height: u32,
    y: &[u8],
    y_stride: usize,
    uv: &[u8],
    uv_stride: usize,
) -> Result<Vec<u8>, MediaError> {
    let bad = || MediaError::Codec("invalid NV12 plane layout".into());
    let bytes = DecodedPixelFormat::Nv12.frame_bytes(width, height)?;
    if !width.is_multiple_of(2) || !height.is_multiple_of(2) {
        return Err(bad());
    }
    let row = width as usize;
    let planes = [
        (y, y_stride, height as usize),
        (uv, uv_stride, height as usize / 2),
    ];
    for (plane, stride, rows) in planes {
        let needed = (rows - 1)
            .checked_mul(stride)
            .and_then(|n| n.checked_add(row));
        if stride < row || needed.is_none_or(|n| n > plane.len()) {
            return Err(bad());
        }
    }
    let mut pixels = Vec::with_capacity(bytes);
    for (plane, stride, rows) in planes {
        for index in 0..rows {
            pixels.extend_from_slice(&plane[index * stride..index * stride + row]);
        }
    }
    Ok(pixels)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packs_tight_and_padded_planes_without_final_padding() {
        let expected = vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12];
        assert_eq!(
            pack_nv12(2, 4, &expected[..8], 2, &expected[8..], 2).unwrap(),
            expected
        );
        assert_eq!(
            pack_nv12(
                2,
                4,
                &[1, 2, 0, 3, 4, 0, 5, 6, 0, 7, 8],
                3,
                &[9, 10, 0, 0, 11, 12],
                4
            )
            .unwrap(),
            expected
        );
    }

    #[test]
    fn rejects_empty_odd_short_and_overflowing_layouts() {
        for (w, h, ys, uvs) in [
            (0, 2, 2, 2),
            (2, 0, 2, 2),
            (3, 2, 3, 3),
            (2, 3, 2, 2),
            (2, 4, 1, 2),
            (2, 4, 2, 1),
            (2, 4, usize::MAX, 2),
            (u32::MAX - 1, u32::MAX - 1, 2, 2),
        ] {
            assert!(pack_nv12(w, h, &[0; 8], ys, &[0; 4], uvs).is_err());
        }
        assert!(pack_nv12(2, 4, &[], 2, &[0; 4], 2).is_err());
        assert!(pack_nv12(2, 4, &[0; 7], 2, &[0; 4], 2).is_err());
        assert!(pack_nv12(2, 4, &[0; 8], 2, &[0; 3], 2).is_err());
    }
}
