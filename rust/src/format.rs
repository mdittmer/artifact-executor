use serde::de::Deserializer;
use serde::de::Visitor;
use serde::ser::Serializer;
use serde::Deserialize;
use serde::Serialize;
use std::collections::HashMap;
use std::convert::TryInto;
use std::path::PathBuf;

//
// Input formats
//

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Task {
    #[serde(default)]
    pub execution_strategy: ExecutionStrategy,
    #[serde(flatten)]
    pub environment_variables: EnvironmentVariables,
    #[serde(flatten)]
    pub program: Program,
    #[serde(flatten)]
    pub arguments: Arguments,
    pub inputs: Inputs,
    pub outputs: Outputs,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionStrategy {
    Simple,
    ForEachInput { inputs_filter: Inputs },
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum InputsFilter {
    All,
    Matches(Inputs),
}

impl Default for ExecutionStrategy {
    fn default() -> Self {
        Self::Simple
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Inputs {
    pub include_files: Vec<PathBuf>,
    pub exclude_files: Vec<PathBuf>,
    pub include_globs: Vec<String>,
    pub exclude_globs: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Outputs {
    pub include_files: Vec<PathBuf>,
    pub include_match_transforms: Vec<MatchTransform>,
    pub exclude_matches: Vec<Match>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct MatchTransform {
    pub match_regular_expression: String,
    pub match_transform_expressions: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Match {
    pub match_regular_expression: String,
}

//
// Shared input/output formats
//

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Arguments {
    pub arguments: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct EnvironmentVariables {
    pub environment_variables: Vec<(String, String)>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Program {
    pub program: PathBuf,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum IdentityScheme {
    ContentSha256,
}

impl Default for IdentityScheme {
    fn default() -> Self {
        Self::ContentSha256
    }
}

#[derive(Clone, Debug)]
pub struct Sha256([u8; 32]);

impl Sha256 {
    pub fn new(hash: [u8; 32]) -> Self {
        Self(hash)
    }
}

impl Serialize for Sha256 {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let hex_string = hex::encode(self.0);
        serializer.serialize_str(&hex_string)
    }
}

struct Sha256Visitor;

impl<'de> Visitor<'de> for Sha256Visitor {
    type Value = Sha256;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("a hex string containing a sha-256 hash")
    }

    fn visit_str<E>(self, hex_str: &str) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        let bytes_vec = hex::decode(hex_str).map_err(|_| {
            E::custom(format!(
                "expected hex string to describing 32 bytes, but got {:?}",
                hex_str
            ))
        })?;
        let bytes_slice = bytes_vec.as_slice();
        let sha256: [u8; 32] = bytes_slice.try_into().map_err(|_| {
            E::custom(format!(
                "expected hex string describing 32 bytes, but got {} bytes",
                bytes_vec.len()
            ))
        })?;
        Ok(Sha256(sha256))
    }
}

impl<'de> Deserialize<'de> for Sha256 {
    fn deserialize<D>(deserializer: D) -> Result<Sha256, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_str(Sha256Visitor)
    }
}

//
// Output formats
//

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct TaskSummary {
    pub input: TaskInput,
    pub output: TaskOutput,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct TaskIndex {
    pub entries: Vec<TaskIdentityIndex>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct TaskIdentityIndex {
    pub identity_scheme: IdentityScheme,
    pub task_input_identity_to_output: HashMap<String, TaskOutput>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct TaskInput {
    #[serde(flatten)]
    pub environment_variables: EnvironmentVariables,
    #[serde(flatten)]
    pub program: Program,
    #[serde(flatten)]
    pub arguments: Arguments,
    pub inputs: FileIdentitiesManifest,
    pub outputs: FileIdentitiesManifest,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct TaskOutput {
    pub inputs: FileIdentitiesManifest,
    pub outputs: FileIdentitiesManifest,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct FilesManifest {
    pub paths: Vec<PathBuf>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct FileIdentitiesManifest {
    pub identity_scheme: IdentityScheme,
    pub paths: Vec<(PathBuf, String)>,
}
