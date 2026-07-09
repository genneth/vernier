// SPDX-License-Identifier: AGPL-3.0-or-later

/// Points per inch in PDF user space (1 pt = 1/72").
const PT_PER_INCH: f64 = 72.0;
const MM_PER_INCH: f64 = 25.4;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Unit {
    Mm,
    M,
    Ft,
    In,
}

impl Unit {
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

/// Parse "3000 mm", "2.5m", "10 ft", "8 in" — or a bare number, which defaults
/// to millimetres (the common case for architectural drawings).
pub fn parse_length(s: &str) -> Option<(f64, Unit)> {
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
        (s, Unit::Mm) // bare number -> millimetres
    };
    rest.trim().parse::<f64>().ok().map(|v| (v, unit))
}

/// Parse an architectural ratio like "1:50" into its scale factor (real/paper),
/// e.g. "1:50" -> 50.0. Also accepts a bare "50".
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
    (ratio > 0.0).then_some(ratio)
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Scale {
    pub units_per_point: f64,
    pub unit: Unit,
}

impl Scale {
    pub fn from_measurement(page_len_points: f64, real_len: f64, unit: Unit) -> Scale {
        Scale {
            units_per_point: real_len / page_len_points,
            unit,
        }
    }

    /// Build a scale from a plot ratio (e.g. 50 for "1:50"), assuming the PDF is
    /// at true plot size (1 pt = 1/72"). Result expressed in `unit`.
    pub fn from_ratio(ratio: f64, unit: Unit) -> Scale {
        let mm_per_point = ratio * MM_PER_INCH / PT_PER_INCH;
        Scale {
            units_per_point: unit.from_mm(mm_per_point),
            unit,
        }
    }

    pub fn apply(&self, page_len_points: f64) -> f64 {
        page_len_points * self.units_per_point
    }

    pub fn format(&self, page_len_points: f64) -> String {
        format!("{:.1} {}", self.apply(page_len_points), self.unit.suffix())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_lengths_with_units() {
        assert_eq!(parse_length("3000 mm"), Some((3000.0, Unit::Mm)));
        assert_eq!(parse_length("2.5m"), Some((2.5, Unit::M)));
        assert_eq!(parse_length("10 ft"), Some((10.0, Unit::Ft)));
        assert_eq!(parse_length("8 in"), Some((8.0, Unit::In)));
    }

    #[test]
    fn bare_number_defaults_to_mm() {
        assert_eq!(parse_length("1428"), Some((1428.0, Unit::Mm)));
        assert_eq!(parse_length("nonsense"), None);
    }

    #[test]
    fn parses_ratio() {
        assert_eq!(parse_ratio("1:50"), Some(50.0));
        assert_eq!(parse_ratio("2:100"), Some(50.0));
        assert_eq!(parse_ratio("50"), Some(50.0));
        assert_eq!(parse_ratio("1:0"), None);
    }

    #[test]
    fn scale_round_trips() {
        let s = Scale::from_measurement(100.0, 3000.0, Unit::Mm);
        assert_eq!(s.apply(100.0), 3000.0);
        assert_eq!(s.format(50.0), "1500.0 mm");
    }

    #[test]
    fn ratio_scale_uses_true_plot_size() {
        // 1:50 -> 1 point (1/72") represents 50/72" = 50*25.4/72 mm ≈ 17.6389 mm.
        let s = Scale::from_ratio(50.0, Unit::Mm);
        assert!((s.apply(1.0) - 17.6389).abs() < 1e-3);
        // 1:1 in inches -> 1 point = 1/72 inch.
        let i = Scale::from_ratio(1.0, Unit::In);
        assert!((i.apply(72.0) - 1.0).abs() < 1e-6);
    }
}
