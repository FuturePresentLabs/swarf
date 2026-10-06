//! Bounded comparison of observed command streams. No resampling or path matching.
use crate::wazer::{self, Command, Error, Program, Summary};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub const MAX_DIFFERENCES: usize = 64;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Tolerances {
    pub coordinates_mm: f64,
    pub feed_mm_min: f64,
    pub dwell_seconds: f64,
}
impl Tolerances {
    fn validate(self) -> Result<(), Error> {
        if [self.coordinates_mm, self.feed_mm_min, self.dwell_seconds]
            .into_iter()
            .all(|v| v.is_finite() && v >= 0.0)
        {
            Ok(())
        } else {
            Err(Error(
                "comparison tolerances must be finite and nonnegative".into(),
            ))
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    Sequence,
    Coordinates,
    Feed,
    Duration,
    Metadata,
    MissingCommand,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Difference {
    /// Zero-based command index, with comments/blank lines excluded.
    pub command_index: usize,
    pub category: Category,
    pub reference: Option<Command>,
    pub candidate: Option<Command>,
    pub reference_effective_feed_mm_min: Option<f64>,
    pub candidate_effective_feed_mm_min: Option<f64>,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Counts {
    pub sequence: usize,
    pub coordinates: usize,
    pub feed: usize,
    pub duration: usize,
    pub metadata: usize,
    pub missing_command: usize,
}
impl Counts {
    pub fn total(&self) -> usize {
        self.sequence
            + self.coordinates
            + self.feed
            + self.duration
            + self.metadata
            + self.missing_command
    }
    fn increment(&mut self, category: Category) {
        *match category {
            Category::Sequence => &mut self.sequence,
            Category::Coordinates => &mut self.coordinates,
            Category::Feed => &mut self.feed,
            Category::Duration => &mut self.duration,
            Category::Metadata => &mut self.metadata,
            Category::MissingCommand => &mut self.missing_command,
        } += 1;
    }
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    Different,
    EquivalentObservedTrace,
    UnresolvedInitialFeed,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Comparison {
    pub schema: String,
    pub tolerances: Tolerances,
    pub byte_identical: bool,
    pub commands_exactly_equal: bool,
    pub verdict: Verdict,
    pub reference_summary: Summary,
    pub candidate_summary: Summary,
    pub difference_counts: Counts,
    pub differences: Vec<Difference>,
    pub differences_truncated: bool,
    pub max_aligned_coordinate_delta_mm: f64,
    pub aligned_coordinate_pairs: usize,
    pub max_aligned_feed_delta_mm_min: f64,
    pub aligned_known_feed_pairs: usize,
    pub max_aligned_duration_delta_seconds: f64,
    pub alignment: String,
    pub controller_qualified: bool,
}
fn effective_feeds(program: &Program) -> Vec<Option<f64>> {
    let mut feed = None;
    program
        .commands
        .iter()
        .map(|c| match c {
            Command::Linear { feed_mm_min, .. } => {
                if let Some(value) = feed_mm_min {
                    feed = Some(*value);
                }
                feed
            }
            _ => None,
        })
        .collect()
}

pub fn compare(
    reference: &str,
    candidate: &str,
    tolerances: Tolerances,
) -> Result<Comparison, Error> {
    tolerances.validate()?;
    let reference_program =
        wazer::parse(reference).map_err(|error| Error(format!("reference: {}", error.0)))?;
    let candidate_program =
        wazer::parse(candidate).map_err(|error| Error(format!("candidate: {}", error.0)))?;
    let reference_summary = wazer::validate(&reference_program)?;
    let candidate_summary = wazer::validate(&candidate_program)?;
    let reference_feeds = effective_feeds(&reference_program);
    let candidate_feeds = effective_feeds(&candidate_program);
    let mut result = Comparison {
        schema: "swarf.wazer-command-comparison.v1".into(), tolerances,
        byte_identical: reference == candidate,
        commands_exactly_equal: reference_program == candidate_program,
        verdict: Verdict::Different, reference_summary, candidate_summary,
        difference_counts: Counts::default(), differences: Vec::new(), differences_truncated: false,
        max_aligned_coordinate_delta_mm: 0.0, aligned_coordinate_pairs: 0,
        max_aligned_feed_delta_mm_min: 0.0, aligned_known_feed_pairs: 0,
        max_aligned_duration_delta_seconds: 0.0,
        alignment: "same command index; no reordering, resampling, coordinate fitting or geometry equivalence inference".into(),
        controller_qualified: false,
    };
    for index in 0..reference_program
        .commands
        .len()
        .max(candidate_program.commands.len())
    {
        let left = reference_program.commands.get(index);
        let right = candidate_program.commands.get(index);
        let lf = reference_feeds.get(index).copied().flatten();
        let rf = candidate_feeds.get(index).copied().flatten();
        let mut categories = Vec::new();
        match (left, right) {
            (Some(a), Some(b)) if std::mem::discriminant(a) != std::mem::discriminant(b) => {
                categories.push(Category::Sequence)
            }
            (Some(a), Some(b)) => match (a, b) {
                (Command::TopLeft { xy_mm: a }, Command::TopLeft { xy_mm: b })
                | (Command::BottomRight { xy_mm: a }, Command::BottomRight { xy_mm: b })
                | (Command::Rapid { xy_mm: a }, Command::Rapid { xy_mm: b })
                | (Command::Linear { xy_mm: a, .. }, Command::Linear { xy_mm: b, .. }) => {
                    let delta = (a[0] - b[0]).hypot(a[1] - b[1]);
                    if !delta.is_finite() {
                        return Err(Error("coordinate comparison overflow".into()));
                    }
                    result.max_aligned_coordinate_delta_mm =
                        result.max_aligned_coordinate_delta_mm.max(delta);
                    result.aligned_coordinate_pairs += 1;
                    if delta > tolerances.coordinates_mm {
                        categories.push(Category::Coordinates);
                    }
                    if matches!(left, Some(Command::Linear { .. })) {
                        match (lf, rf) {
                            (Some(a), Some(b)) => {
                                let delta = (a - b).abs();
                                result.max_aligned_feed_delta_mm_min =
                                    result.max_aligned_feed_delta_mm_min.max(delta);
                                result.aligned_known_feed_pairs += 1;
                                if delta > tolerances.feed_mm_min {
                                    categories.push(Category::Feed);
                                }
                            }
                            (None, None) => {}
                            _ => categories.push(Category::Feed),
                        }
                    }
                }
                (Command::Dwell { seconds: a }, Command::Dwell { seconds: b })
                | (Command::HeaderPierce { seconds: a }, Command::HeaderPierce { seconds: b }) => {
                    let delta = (a - b).abs();
                    result.max_aligned_duration_delta_seconds =
                        result.max_aligned_duration_delta_seconds.max(delta);
                    if delta > tolerances.dwell_seconds {
                        categories.push(Category::Duration);
                    }
                }
                _ if a != b => categories.push(Category::Metadata),
                _ => {}
            },
            _ => categories.push(Category::MissingCommand),
        }
        for category in categories {
            result.difference_counts.increment(category);
            if result.differences.len() < MAX_DIFFERENCES {
                result.differences.push(Difference {
                    command_index: index,
                    category,
                    reference: left.cloned(),
                    candidate: right.cloned(),
                    reference_effective_feed_mm_min: lf,
                    candidate_effective_feed_mm_min: rf,
                });
            }
        }
    }
    result.differences_truncated = result.difference_counts.total() > result.differences.len();
    result.verdict = if result.difference_counts.total() > 0 {
        Verdict::Different
    } else if result.reference_summary.unresolved_feed_moves > 0
        || result.candidate_summary.unresolved_feed_moves > 0
    {
        Verdict::UnresolvedInitialFeed
    } else {
        Verdict::EquivalentObservedTrace
    };
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> &'static str {
        // Independent hand-authored protocol fixture; not a WAM export.
        "G90\nG21\nM1403\nM1405 X0 Y0\nM1406 X100 Y-100\nM1407 S2\nM1410 1.6\nM1411 Aluminum\nM1412 3 mm\nG0 X1 Y-1\nM3\nM8\nG4 S2\nG1 X5 Y-1 F120\nG1 X10 Y-1\nG4 S1\nM9\nG4 S1\nM5\nG4 S1\nM1413 00:00:10\nM1404\n"
    }
    fn exact() -> Tolerances {
        Tolerances {
            coordinates_mm: 0.0,
            feed_mm_min: 0.0,
            dwell_seconds: 0.0,
        }
    }
    #[test]
    fn comments_formatting_and_modal_feed_have_distinct_equality_levels() {
        let formatted = fixture()
            .replace("X5", "X5.000")
            .replace("X10 Y-1", "X10 Y-1 F120");
        let report = compare(
            fixture(),
            &format!("; independent fixture\n{formatted}"),
            exact(),
        )
        .unwrap();
        assert!(!report.byte_identical && !report.commands_exactly_equal);
        assert_eq!(report.verdict, Verdict::EquivalentObservedTrace);
        assert_eq!(report.aligned_known_feed_pairs, 2);
        assert_eq!(report.difference_counts.total(), 0);
        assert!(
            compare(fixture(), fixture(), exact())
                .unwrap()
                .byte_identical
        );
    }
    #[test]
    fn coordinates_feeds_duration_metadata_and_missing_commands_are_reported() {
        let altered = fixture()
            .replace("X5 Y-1 F120", "X5.1 Y-1 F100")
            .replace("G4 S2", "G4 S3")
            .replace("Aluminum", "Steel");
        let report = compare(fixture(), &altered, exact()).unwrap();
        assert_eq!(report.verdict, Verdict::Different);
        assert_eq!(report.difference_counts.coordinates, 1);
        assert_eq!(report.difference_counts.feed, 2); // modal change persists
        assert_eq!(report.difference_counts.duration, 1);
        assert_eq!(report.difference_counts.metadata, 1);
        let loose = Tolerances {
            coordinates_mm: 0.11,
            feed_mm_min: 20.0,
            dwell_seconds: 1.0,
        };
        assert_eq!(
            compare(fixture(), &altered.replace("Steel", "Aluminum"), loose)
                .unwrap()
                .verdict,
            Verdict::EquivalentObservedTrace
        );
        let extra = fixture().replace("M1413", "G4 S1\nM1413");
        let report = compare(fixture(), &extra, exact()).unwrap();
        assert!(
            report.difference_counts.sequence > 0 && report.difference_counts.missing_command > 0
        );
    }
    #[test]
    fn unresolved_initial_feeds_never_claim_equivalent_trace() {
        let unknown = fixture().replace(" F120", "");
        let report = compare(&unknown, &unknown, exact()).unwrap();
        assert!(report.byte_identical && report.commands_exactly_equal);
        assert_eq!(report.verdict, Verdict::UnresolvedInitialFeed);
        assert_eq!(report.reference_summary.unresolved_feed_moves, 2);
        assert_eq!(
            compare(&unknown, fixture(), exact())
                .unwrap()
                .difference_counts
                .feed,
            2
        );
    }
    #[test]
    fn budgets_and_invalid_inputs_fail_and_difference_details_are_capped() {
        assert!(compare(fixture(), "G20", exact()).is_err());
        assert!(compare(
            fixture(),
            fixture(),
            Tolerances {
                coordinates_mm: f64::NAN,
                ..exact()
            }
        )
        .is_err());
        assert!(compare(
            fixture(),
            fixture(),
            Tolerances {
                dwell_seconds: -1.0,
                ..exact()
            }
        )
        .is_err());
        let left = fixture().replace("G1 X5 Y-1 F120", &"G1 X5 Y-1 F120\n".repeat(100));
        let right = left.replace("X5 Y-1", "X6 Y-1");
        let report = compare(&left, &right, exact()).unwrap();
        assert_eq!(report.difference_counts.coordinates, 100);
        assert_eq!(report.differences.len(), MAX_DIFFERENCES);
        assert!(report.differences_truncated);
    }
}
