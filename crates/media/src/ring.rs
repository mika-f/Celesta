//! How big an export's ring of frame slots is (see
//! `celesta_plugin::shared::RingLayout`).

use crate::provider::PixelFormat;
use celesta_plugin::shared::{RingLayout, SLOT_HEADER};

/// The slots a ring has when it fits.
pub const RING_SLOTS: usize = 3;

/// The most bytes a ring may take; fewer slots fit a larger frame.
pub const MAX_RING_BYTES: usize = 256 << 20;

/// The ring for `width` x `height` frames in any of `formats`, of which
/// only those Celesta can produce ([`PixelFormat::OFFERABLE`]) count. A
/// slot holds the largest of their frames, or an RGBA16 frame when none
/// count (the encoder's formats are unknown). The slot size is rounded up to
/// a multiple of 8, as a ring of several slots needs. Slots drop from
/// [`RING_SLOTS`] while the segment would exceed [`MAX_RING_BYTES`]; `None`
/// when not even one fits, and the frames go through the pipe.
pub fn ring_layout(formats: &[PixelFormat], width: u32, height: u32) -> Option<RingLayout> {
    let slot_bytes = PixelFormat::OFFERABLE
        .into_iter()
        .filter(|format| formats.contains(format))
        .map(|format| format.frame_bytes(width, height))
        .max()
        .unwrap_or_else(|| PixelFormat::Rgba16.frame_bytes(width, height))
        .checked_next_multiple_of(8)?;
    (1..=RING_SLOTS).rev().find_map(|slots| {
        let bytes = slots.checked_mul(slot_bytes.checked_add(SLOT_HEADER)?)?;
        (bytes <= MAX_RING_BYTES).then_some(RingLayout { slots, slot_bytes })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use PixelFormat::*;

    fn layout(slots: usize, slot_bytes: usize) -> Option<RingLayout> {
        Some(RingLayout { slots, slot_bytes })
    }

    #[test]
    fn frame_bytes_follow_the_format() {
        assert_eq!(Rgba8.frame_bytes(1920, 1080), 8_294_400);
        assert_eq!(Rgba16.frame_bytes(1920, 1080), 16_588_800);
        assert_eq!(Yuv420p.frame_bytes(1920, 1080), 3_110_400);
        assert_eq!(Yuv420p10.frame_bytes(1920, 1080), 6_220_800);
        assert_eq!(
            Nv12.frame_bytes(1920, 1080),
            Yuv420p.frame_bytes(1920, 1080)
        );
        assert_eq!(
            P010.frame_bytes(1920, 1080),
            Yuv420p10.frame_bytes(1920, 1080)
        );
        assert_eq!(Yuv420p.frame_bytes(33, 17), 867);
    }

    #[test]
    fn ring_layout_takes_the_largest_offerable_format() {
        assert_eq!(
            ring_layout(&[Yuv420p, Rgba8], 1920, 1080),
            layout(3, 8_294_400)
        );
        assert_eq!(ring_layout(&[Yuv420p], 1920, 1080), layout(3, 3_110_400));
        assert_eq!(ring_layout(&[], 1920, 1080), layout(3, 16_588_800));
        assert_eq!(
            ring_layout(&[Nv12, P010], 1920, 1080),
            layout(3, 16_588_800)
        );
    }

    #[test]
    fn ring_layout_rounds_a_slot_up_to_a_multiple_of_8() {
        assert_eq!(ring_layout(&[Yuv420p], 33, 17), layout(3, 872));
    }

    #[test]
    fn ring_layout_shrinks_to_the_cap() {
        assert_eq!(ring_layout(&[Rgba16], 3840, 2160), layout(3, 66_355_200));
        assert_eq!(ring_layout(&[Rgba8], 7680, 4320), layout(2, 132_710_400));
        assert_eq!(ring_layout(&[Rgba16], 7680, 4320), layout(1, 265_420_800));
        assert_eq!(ring_layout(&[Rgba16], 8192, 4320), None);
    }
}
