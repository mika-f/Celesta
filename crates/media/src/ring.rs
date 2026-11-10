//! How big an export's ring of frame slots is (see
//! `celesta_plugin::shared::RingLayout`).

use crate::provider::{PIPE_WINDOW, PixelFormat};
use celesta_plugin::shared::RingLayout;

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
        let layout = RingLayout { slots, slot_bytes };
        (layout.segment_bytes()? <= MAX_RING_BYTES).then_some(layout)
    })
}

/// How many `frame_bytes`-byte frames a pipeline without a ring keeps in
/// flight: [`PIPE_WINDOW`], fewer when that many frames would exceed
/// [`MAX_RING_BYTES`] (the bound a ring has), and at least one.
pub fn pipe_depth(frame_bytes: usize) -> usize {
    (MAX_RING_BYTES / frame_bytes.max(1)).clamp(1, PIPE_WINDOW)
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
    fn an_absurd_frame_saturates_and_gets_no_ring() {
        for format in PixelFormat::OFFERABLE {
            assert_eq!(format.frame_bytes(u32::MAX, u32::MAX), usize::MAX);
        }
        assert_eq!(ring_layout(&[Rgba16], u32::MAX, u32::MAX), None);
        assert_eq!(ring_layout(&[], u32::MAX, u32::MAX), None);
        assert_eq!(pipe_depth(Rgba16.frame_bytes(u32::MAX, u32::MAX)), 1);
    }

    #[test]
    fn ring_layout_takes_the_largest_offerable_format() {
        assert_eq!(
            ring_layout(&[Yuv420p, Rgba8], 1920, 1080),
            layout(3, 8_294_400)
        );
        assert_eq!(ring_layout(&[Yuv420p], 1920, 1080), layout(3, 3_110_400));
        assert_eq!(ring_layout(&[], 1920, 1080), layout(3, 16_588_800));
        assert_eq!(ring_layout(&[Nv12, P010], 1920, 1080), layout(3, 6_220_800));
    }

    #[test]
    fn ring_layout_rounds_a_slot_up_to_a_multiple_of_8() {
        assert_eq!(ring_layout(&[Yuv420p], 33, 17), layout(3, 872));
    }

    #[test]
    fn the_pipes_depth_is_bounded_by_the_rings_ceiling() {
        assert_eq!(pipe_depth(Rgba8.frame_bytes(1920, 1080)), 3);
        assert_eq!(pipe_depth(Rgba16.frame_bytes(3840, 2160)), 3);
        assert_eq!(pipe_depth(Rgba16.frame_bytes(7680, 4320)), 1);
        assert_eq!(pipe_depth(Rgba8.frame_bytes(7680, 4320)), 2);
        assert_eq!(pipe_depth(Rgba16.frame_bytes(8192, 4320)), 1);
        assert_eq!(pipe_depth(MAX_RING_BYTES / 2), 2);
        assert_eq!(pipe_depth(MAX_RING_BYTES / 3), 3);
        assert_eq!(pipe_depth(0), 3);
        assert_eq!(pipe_depth(usize::MAX), 1);
    }

    #[test]
    fn ring_layout_shrinks_to_the_cap() {
        assert_eq!(ring_layout(&[Rgba16], 3840, 2160), layout(3, 66_355_200));
        assert_eq!(ring_layout(&[Rgba8], 7680, 4320), layout(2, 132_710_400));
        assert_eq!(ring_layout(&[Rgba16], 7680, 4320), layout(1, 265_420_800));
        assert_eq!(ring_layout(&[Rgba16], 8192, 4320), None);
    }
}
