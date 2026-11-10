//! Media Foundation's time unit: 100 ns ("hns"). Track times are in a
//! timescale's ticks; these convert between the two, rounding half away
//! from zero, so a frame time of any common rate comes back exact.

const HNS_PER_SECOND: i128 = 10_000_000;

/// `numerator / denominator` rounded half away from zero.
fn divide_rounded(numerator: i128, denominator: i128) -> i128 {
    let half = denominator / 2;
    if numerator >= 0 {
        (numerator + half) / denominator
    } else {
        -((-numerator + half) / denominator)
    }
}

/// `t` ticks of `timescale` per second, in hns.
pub(crate) fn to_hns(t: i64, timescale: u32) -> i64 {
    divide_rounded(i128::from(t) * HNS_PER_SECOND, i128::from(timescale.max(1))) as i64
}

/// `h` hns, in ticks of `timescale` per second.
pub(crate) fn from_hns(h: i64, timescale: u32) -> i64 {
    divide_rounded(i128::from(h) * i128::from(timescale), HNS_PER_SECOND) as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_times_round_trip_through_hns() {
        for (timescale, step) in [
            (30_000u32, 1001i64),
            (60_000, 1001),
            (24_000, 1001),
            (25, 1),
            (48_000, 1024),
            (44_100, 1024),
        ] {
            for k in 0..=1_000_000i64 {
                let t = k * step;
                assert_eq!(
                    from_hns(to_hns(t, timescale), timescale),
                    t,
                    "{timescale} {k}"
                );
            }
        }
    }

    #[test]
    fn negative_times_round_trip_too() {
        for (timescale, step) in [(30_000u32, 1001i64), (48_000, 1024), (44_100, 1024)] {
            for k in -1000..=0i64 {
                let t = k * step;
                assert_eq!(from_hns(to_hns(t, timescale), timescale), t);
            }
        }
        // Half away from zero, both ways.
        assert_eq!(to_hns(1, 20_000_000), 1);
        assert_eq!(to_hns(-1, 20_000_000), -1);
        assert_eq!(to_hns(1, 90_000), 111);
        assert_eq!(to_hns(-1, 90_000), -111);
    }

    #[test]
    fn known_values() {
        assert_eq!(to_hns(1001, 30_000), 333_667);
        assert_eq!(to_hns(1, 1), 10_000_000);
        assert_eq!(from_hns(10_000_000, 48_000), 48_000);
    }
}
