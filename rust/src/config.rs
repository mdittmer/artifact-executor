use serde::Deserialize;
use serde::Serialize;
use std::path::PathBuf;

#[derive(Debug, Deserialize, Serialize)]
pub struct Inputs {
    pub include_files: Vec<PathBuf>,
    pub exclude_files: Vec<PathBuf>,
    pub include_globs: Vec<String>,
    pub exclude_globs: Vec<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Outputs {
    pub include_files: Vec<PathBuf>,
    pub include_match_transforms: Vec<MatchTransform>,
    pub exclude_matches: Vec<Match>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct MatchTransform {
    pub match_regular_expression: String,
    pub match_transform_expressions: Vec<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Match {
    pub match_regular_expression: String,
}
