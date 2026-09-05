// SPDX-License-Identifier: AGPL-3.0-or-later
//! Real-world scale: the map from page lengths (points) to real lengths.
use crate::geometry::PageLen;

/// Points per inch in PDF user space (1 pt = 1/72").
const PT_PER_INCH: f64 = 72.0;
const MM_PER_INCH: f64 = 25.4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unit {
    Mm,
    M,
    Ft,
    In,
}

impl Unit {
    pub const ALL: [Unit; 4] = [Unit::Mm, Unit::M, Unit::Ft, Unit::In];

    pub fn suffix(&self) -> &'static str {
        match self {
            Unit::Mm => "mm",
            Unit::M => "m",
            Unit::Ft => "ft",
            Unit::In => "in",
        }
    }

    /// Convert a length in millimetres into this unit.
    pub fn from_mm(&self, mm: f64) -> f64 {
        match self {
            Unit::Mm => mm,
            Unit::M => mm / 1000.0,
            Unit::Ft => mm / (MM_PER_INCH * 12.0),
            Unit::In => mm / MM_PER_INCH,
        }
    }
}

/// A real-world length with its unit.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RealLen {
    pub value: f64,
    pub unit: Unit,
}

impl RealLen {
    pub fn new(value: f64, unit: Unit) -> RealLen {
        RealLen { value, unit }
    }

    /// The readout form, one decimal: `"1500.0 mm"`.
    pub fn format(&self) -> String {
        format!("{:.1} {}", self.value, self.unit.suffix())
    }
}

/// Parse "3000 mm", "2.5m", "10 ft", "8 in" — or a bare number, which defaults
/// to millimetres (the common case for architectural drawings). Only finite,
/// non-negative lengths parse.
pub fn parse_length(s: &str) -> Option<RealLen> {
    let s = s.trim();
    let (rest, unit) = if let Some(r) = s.strip_suffix("mm") {
        (r, Unit::Mm)
    } else if let Some(r) = s.strip_suffix("ft") {
        (r, Unit::Ft)
    } else if let Some(r) = s.strip_suffix("in") {
        (r, Unit::In)
    } else if let Some(r) = s.strip_suffix('m') {
        (r, Unit::M)
    } else {
        (s, Unit::Mm)
    };
    let value: f64 = rest.trim().parse().ok()?;
    (value.is_finite() && value >= 0.0).then_some(RealLen { value, unit })
}

/// Parse an architectural ratio like "1:50" into its scale factor (real/paper),
/// e.g. "1:50" -> 50.0. Also accepts a bare "50". Only finite, positive ratios.
pub fn parse_ratio(s: &str) -> Option<f64> {
    let s = s.trim();
    let ratio = if let Some((a, b)) = s.split_once(':') {
        let a: f64 = a.trim().parse().ok()?;
        let b: f64 = b.trim().parse().ok()?;
        if a == 0.0 {
            return None;
        }
        b / a
    } else {
        s.parse::<f64>().ok()?
    };
    (ratio.is_finite() && ratio > 0.0).then_some(ratio)
}

/// Real units per page point. Constructed only through the two named routes,
/// both of which reject degenerate inputs, so a `Scale` is always finite and
/// positive.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Scale {
    units_per_point: f64,
    unit: Unit,
}

impl Scale {
    /// From a measured page length and the real length the user says it is.
    /// `None` if either length is zero or non-finite.
    pub fn from_measurement(page_len: PageLen, real: RealLen) -> Option<Scale> {
        let upp = real.value / page_len.0;
        (page_len.0 > 0.0 && real.value > 0.0 && upp.is_finite()).then_some(Scale {
            units_per_point: upp,
            unit: real.unit,
        })
    }

    /// From a plot ratio (e.g. 50 for "1:50"), assuming the PDF is at true plot
    /// size (1 pt = 1/72"). Result expressed in `unit`.
    pub fn from_ratio(ratio: f64, unit: Unit) -> Option<Scale> {
        let mm_per_point = ratio * MM_PER_INCH / PT_PER_INCH;
        (ratio.is_finite() && ratio > 0.0).then_some(Scale {
            units_per_point: unit.from_mm(mm_per_point),
            unit,
        })
    }

    pub fn unit(&self) -> Unit {
        self.unit
    }

    pub fn apply(&self, page_len: PageLen) -> RealLen {
        RealLen {
            value: page_len.0 * self.units_per_point,
            unit: self.unit,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn parses_lengths_with_units() {
        assert_eq!(
            parse_length("3000 mm"),
            Some(RealLen::new(3000.0, Unit::Mm))
        );
        assert_eq!(parse_length("2.5m"), Some(RealLen::new(2.5, Unit::M)));
        assert_eq!(parse_length("10 ft"), Some(RealLen::new(10.0, Unit::Ft)));
        assert_eq!(parse_length("8 in"), Some(RealLen::new(8.0, Unit::In)));
    }

    #[test]
    fn bare_number_defaults_to_mm() {
        assert_eq!(parse_length("1428"), Some(RealLen::new(1428.0, Unit::Mm)));
        assert_eq!(parse_length("nonsense"), None);
        assert_eq!(parse_length("-3"), None);
        assert_eq!(parse_length("inf"), None);
    }

    #[test]
    fn parses_ratio() {
        assert_eq!(parse_ratio("1:50"), Some(50.0));
        assert_eq!(parse_ratio("2:100"), Some(50.0));
        assert_eq!(parse_ratio("50"), Some(50.0));
        assert_eq!(parse_ratio("1:0"), None);
        assert_eq!(parse_ratio("0:1"), None);
        assert_eq!(parse_ratio("-1:50"), None);
    }

    #[test]
    fn scale_round_trips() {
        let s = Scale::from_measurement(PageLen(100.0), RealLen::new(3000.0, Unit::Mm)).unwrap();
        assert_eq!(s.apply(PageLen(100.0)).value, 3000.0);
        assert_eq!(s.apply(PageLen(50.0)).format(), "1500.0 mm");
    }

    #[test]
    fn degenerate_measurements_make_no_scale() {
        assert!(Scale::from_measurement(PageLen(0.0), RealLen::new(1.0, Unit::Mm)).is_none());
        assert!(Scale::from_measurement(PageLen(1.0), RealLen::new(0.0, Unit::Mm)).is_none());
        assert!(Scale::from_ratio(0.0, Unit::Mm).is_none());
    }

    #[test]
    fn ratio_scale_uses_true_plot_size() {
        // 1:50 -> 1 point (1/72") represents 50/72" = 50*25.4/72 mm ≈ 17.6389 mm.
        let s = Scale::from_ratio(50.0, Unit::Mm).unwrap();
        assert!((s.apply(PageLen(1.0)).value - 17.6389).abs() < 1e-3);
        // 1:1 in inches -> 1 point = 1/72 inch.
        let i = Scale::from_ratio(1.0, Unit::In).unwrap();
        assert!((i.apply(PageLen(72.0)).value - 1.0).abs() < 1e-6);
    }

    fn unit() -> impl Strategy<Value = Unit> {
        prop::sample::select(Unit::ALL.to_vec())
    }

    proptest! {
        /// A formatted readout parses back to the same length, up to the one
        /// decimal the readout shows.
        #[test]
        fn format_then_parse_round_trips(value in 0.0..1e7, unit in unit()) {
            let shown = RealLen::new(value, unit).format();
            let back = parse_length(&shown).unwrap();
            prop_assert_eq!(back.unit, unit);
            prop_assert!((back.value - value).abs() <= 0.05 + 1e-9);
        }

        /// Measuring the calibration segment itself always reads back the
        /// typed length.
        #[test]
        fn calibration_segment_reads_as_typed(
            page_len in 1e-3..1e5, real in 1e-3..1e7, unit in unit()
        ) {
            let s = Scale::from_measurement(PageLen(page_len), RealLen::new(real, unit)).unwrap();
            let got = s.apply(PageLen(page_len));
            prop_assert_eq!(got.unit, unit);
            prop_assert!((got.value - real).abs() <= real * 1e-9);
        }

        /// Scaling is linear in the page length.
        #[test]
        fn scale_is_linear(ratio in 1.0..1000.0, a in 0.0..1e4, b in 0.0..1e4) {
            let s = Scale::from_ratio(ratio, Unit::Mm).unwrap();
            let sum = s.apply(PageLen(a + b)).value;
            let parts = s.apply(PageLen(a)).value + s.apply(PageLen(b)).value;
            prop_assert!((sum - parts).abs() <= 1e-6 * sum.max(1.0));
        }
    }
}
