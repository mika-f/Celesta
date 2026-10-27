use std::fmt;

/// A straight-alpha sRGB color, written `#RRGGBB` or `#RRGGBBAA`.
///
/// Scene and project models keep colors as the text they were written in;
/// this is the one parser every consumer reads that text with.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Color {
    pub red: u8,
    pub green: u8,
    pub blue: u8,
    pub alpha: u8,
}

impl Color {
    pub const TRANSPARENT: Self = Self::rgba(0, 0, 0, 0);
    pub const WHITE: Self = Self::rgba(255, 255, 255, 255);

    pub const fn rgba(red: u8, green: u8, blue: u8, alpha: u8) -> Self {
        Self {
            red,
            green,
            blue,
            alpha,
        }
    }

    /// `#RRGGBB` (opaque) or `#RRGGBBAA`, digits in either case; `None` for
    /// anything else, including non-ASCII text.
    pub fn from_hex(value: &str) -> Option<Self> {
        let hex = value.strip_prefix('#')?.as_bytes();
        if !matches!(hex.len(), 6 | 8) {
            return None;
        }
        let digit = |byte: u8| char::from(byte).to_digit(16);
        let byte = |offset: usize| Some((digit(hex[offset])? * 16 + digit(hex[offset + 1])?) as u8);
        Some(Self::rgba(
            byte(0)?,
            byte(2)?,
            byte(4)?,
            if hex.len() == 8 { byte(6)? } else { 255 },
        ))
    }

    pub const fn channels(self) -> [u8; 4] {
        [self.red, self.green, self.blue, self.alpha]
    }

    /// Each channel `t` of the way from `self` to `to`, rounded; `t` is not
    /// clamped, so an overshooting easing saturates at 0 or 255.
    pub fn lerp(self, to: Self, t: f64) -> Self {
        let channel = |from: u8, to: u8| {
            let (from, to) = (f64::from(from), f64::from(to));
            (from + (to - from) * t).round().clamp(0.0, 255.0) as u8
        };
        Self::rgba(
            channel(self.red, to.red),
            channel(self.green, to.green),
            channel(self.blue, to.blue),
            channel(self.alpha, to.alpha),
        )
    }
}

/// `#RRGGBBAA`, in upper case.
impl fmt::Display for Color {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "#{:02X}{:02X}{:02X}{:02X}",
            self.red, self.green, self.blue, self.alpha
        )
    }
}
