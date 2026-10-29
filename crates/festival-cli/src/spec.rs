//! Value parsers for level selections and durations.

use std::fmt::{self, Display};
use std::time::Duration;

/// One segment of a level selection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Segment {
    /// A single 1-based level number.
    Single(usize),
    /// An inclusive 1-based level range.
    Range(usize, usize),
}

impl Segment {
    fn describe(self) -> String {
        match self {
            Self::Single(level) => format!("level {level}"),
            Self::Range(start, end) => format!("level range {start}..{end}"),
        }
    }
}

/// A parsed `--level` selection.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LevelSpec {
    segments: Vec<Segment>,
    all: bool,
}

impl LevelSpec {
    /// Resolves the selection into zero-based level indices, deduplicated in
    /// selection order.
    pub fn select(&self, count: usize) -> Result<Vec<usize>, SelectError> {
        if self.all {
            return Ok((0..count).collect());
        }
        let mut selected = Vec::new();
        for &segment in &self.segments {
            let (start, end) = match segment {
                Segment::Single(level) => (level, level),
                Segment::Range(start, end) => (start, end),
            };
            if end > count {
                return Err(SelectError::out_of_bounds(segment.describe()));
            }
            for level in start..=end {
                let index = level - 1;
                if !selected.contains(&index) {
                    selected.push(index);
                }
            }
        }
        Ok(selected)
    }
}

/// Parses a level selection such as `all`, `3`, `1..5` or `2,7,9`.
pub fn parse_level_spec(text: &str) -> Result<LevelSpec, String> {
    let text = text.trim();
    if text.eq_ignore_ascii_case("all") {
        return Ok(LevelSpec {
            segments: Vec::new(),
            all: true,
        });
    }

    let mut segments = Vec::new();
    for part in text.split(',') {
        let part = part.trim();
        if part.is_empty() {
            return Err(format!("invalid level selection `{text}`"));
        }
        let segment = if let Some((start, end)) = part.split_once("..") {
            let start =
                parse_level_number(start).map_err(|_| format!("invalid level range `{part}`"))?;
            let end =
                parse_level_number(end).map_err(|_| format!("invalid level range `{part}`"))?;
            if start > end {
                return Err(format!("invalid level range `{part}`"));
            }
            Segment::Range(start, end)
        } else {
            Segment::Single(parse_level_number(part)?)
        };
        segments.push(segment);
    }
    Ok(LevelSpec {
        segments,
        all: false,
    })
}

fn parse_level_number(text: &str) -> Result<usize, String> {
    let level = text
        .parse::<usize>()
        .map_err(|_| format!("invalid level `{text}`"))?;
    if level == 0 {
        return Err("level numbers start at 1".to_string());
    }
    Ok(level)
}

/// An error resolving a [`LevelSpec`] against the levels of an input.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SelectError {
    description: String,
}

impl SelectError {
    fn out_of_bounds(description: impl Into<String>) -> Self {
        Self {
            description: description.into(),
        }
    }
}

impl Display for SelectError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} out of bounds", self.description)
    }
}

impl std::error::Error for SelectError {}

/// Parses a duration such as `500ms`, `30s`, `5m` or `1h`.
///
/// A missing unit is interpreted as seconds.
pub fn parse_duration(text: &str) -> Result<Duration, String> {
    let text = text.trim();
    let split = text
        .find(|char: char| !char.is_ascii_digit() && char != '.')
        .unwrap_or(text.len());
    let (value, unit) = text.split_at(split);
    let value: f64 = value
        .parse()
        .map_err(|_| format!("invalid duration `{text}`"))?;
    let seconds = match unit.to_ascii_lowercase().as_str() {
        "" | "s" => value,
        "ms" => value / 1000.0,
        "m" => value * 60.0,
        "h" => value * 3600.0,
        _ => return Err(format!("invalid duration unit in `{text}`")),
    };
    if !seconds.is_finite() {
        return Err(format!("invalid duration `{text}`"));
    }
    Ok(Duration::from_secs_f64(seconds))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(text: &str) -> LevelSpec {
        parse_level_spec(text).expect("valid spec")
    }

    #[test]
    fn parses_and_selects_all() {
        assert_eq!(spec("all").select(3).unwrap(), vec![0, 1, 2]);
        assert_eq!(spec("ALL").select(2).unwrap(), vec![0, 1]);
    }

    #[test]
    fn parses_single_levels() {
        assert_eq!(spec("3").select(5).unwrap(), vec![2]);
    }

    #[test]
    fn parses_ranges() {
        assert_eq!(spec("1..5").select(5).unwrap(), vec![0, 1, 2, 3, 4]);
    }

    #[test]
    fn parses_lists_and_combinations() {
        assert_eq!(spec("2,7,9").select(10).unwrap(), vec![1, 6, 8]);
        assert_eq!(spec("1..3,10").select(10).unwrap(), vec![0, 1, 2, 9]);
    }

    #[test]
    fn deduplicates_in_selection_order() {
        assert_eq!(spec("3,2..4,2").select(5).unwrap(), vec![2, 1, 3]);
    }

    #[test]
    fn rejects_invalid_specs() {
        for text in [
            "", "  ", "0", "1..", "..5", "5..3", "1,,2", ",1", "1,", "x", "1.5",
        ] {
            assert!(
                parse_level_spec(text).is_err(),
                "spec `{text}` should be rejected"
            );
        }
    }

    #[test]
    fn rejects_out_of_bounds_selections() {
        assert_eq!(
            spec("1..5").select(3).unwrap_err().to_string(),
            "level range 1..5 out of bounds"
        );
        assert_eq!(
            spec("9").select(3).unwrap_err().to_string(),
            "level 9 out of bounds"
        );
    }

    #[test]
    fn parses_durations() {
        assert_eq!(parse_duration("500ms").unwrap(), Duration::from_millis(500));
        assert_eq!(parse_duration("30s").unwrap(), Duration::from_secs(30));
        assert_eq!(parse_duration("5m").unwrap(), Duration::from_secs(300));
        assert_eq!(parse_duration("1h").unwrap(), Duration::from_secs(3600));
        assert_eq!(parse_duration("2").unwrap(), Duration::from_secs(2));
        assert_eq!(parse_duration("0.5s").unwrap(), Duration::from_millis(500));
        assert_eq!(parse_duration("0").unwrap(), Duration::ZERO);
    }

    #[test]
    fn rejects_invalid_durations() {
        for text in ["", "abc", "-5", "1x", "1..5", "s"] {
            assert!(
                parse_duration(text).is_err(),
                "duration `{text}` should be rejected"
            );
        }
    }
}
