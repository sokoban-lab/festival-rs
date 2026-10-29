//! JSON output for `--format json`.

use std::time::Duration;

use serde::Serialize;

/// The version of the JSON output schema.
pub const SCHEMA_VERSION: u32 = 1;

#[derive(Serialize)]
struct Document<'a, T: Serialize> {
    schema: u32,
    command: &'a str,
    #[serde(flatten)]
    body: T,
}

/// Writes one JSON document to standard output.
pub fn write(command: &str, body: &impl Serialize) -> Result<(), String> {
    let document = Document {
        schema: SCHEMA_VERSION,
        command,
        body,
    };
    let text = serde_json::to_string_pretty(&document).map_err(|err| format!("json: {err}"))?;
    println!("{text}");
    Ok(())
}

/// Converts a duration to milliseconds, rounded to microsecond precision.
pub fn millis(duration: Duration) -> f64 {
    (duration.as_secs_f64() * 1_000_000.0).round() / 1000.0
}

/// The body of the `solve` document.
#[derive(Serialize)]
pub struct SolveReport {
    pub results: Vec<SolveResult>,
    pub summary: SolveSummary,
}

/// One level in the `solve` document.
#[derive(Serialize)]
pub struct SolveResult {
    pub index: usize,
    pub title: String,
    pub status: &'static str,
    pub failure: Option<String>,
    pub moves: usize,
    pub pushes: usize,
    pub expansions: u64,
    pub elapsed_ms: f64,
    pub solutions: Vec<Solution>,
}

/// A produced solution.
#[derive(Serialize)]
pub struct Solution {
    pub lurd: String,
    pub solved: bool,
    pub moves: usize,
    pub pushes: usize,
}

/// The summary of a `solve` run.
#[derive(Default, Serialize)]
pub struct SolveSummary {
    pub levels: usize,
    pub solved: usize,
    pub moves: usize,
    pub pushes: usize,
    pub elapsed_ms: f64,
}

/// The body of the `analyse` document.
#[derive(Serialize)]
pub struct AnalysisReport {
    pub results: Vec<AnalysisResult>,
    pub summary: AnalysisSummary,
}

/// One level in the `analyse` document.
#[derive(Serialize)]
pub struct AnalysisResult {
    pub index: usize,
    pub title: String,
    pub status: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failure: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub analysis: Option<AnalysisDetails>,
}

/// Static analysis figures of one level.
#[derive(Serialize)]
pub struct AnalysisDetails {
    pub width: usize,
    pub height: usize,
    pub boxes: usize,
    pub targets: usize,
    pub inner_cells: usize,
    pub rooms: usize,
    pub hotspots: usize,
    pub aborted_hotspots: bool,
    pub forbidden_push: usize,
    pub forbidden_pull: usize,
    pub hash: String,
    pub elapsed_ms: f64,
}

/// The summary of an `analyse` run.
#[derive(Serialize)]
pub struct AnalysisSummary {
    pub levels: usize,
    pub elapsed_ms: f64,
}
