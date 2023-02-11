// Copyright 2023 The Artifact Executor Authors. All rights reserved.
// Use of this source code is governed by a Apache-style license that can be
// found in the LICENSE file.

use crate::identity::Identity as IdentityBound;
use anyhow::Context as _;
use serde::de::DeserializeOwned;
use serde::de::Deserializer;
use serde::de::Visitor;
use serde::ser::Serializer;
use serde::Deserialize;
use serde::Serialize;
use std::convert::TryFrom;
use std::convert::TryInto;
use std::path::PathBuf;
use sysinfo::SystemExt;

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
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub include_files: Vec<PathBuf>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub exclude_files: Vec<PathBuf>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub include_globs: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub exclude_globs: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub inter_file_references: Vec<InterFileReferences>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Outputs {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub include_files: Vec<PathBuf>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub include_match_transforms: Vec<MatchTransform>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub exclude_matches: Vec<Match>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct InterFileReferences {
    /// Default: Use matched files from containing object.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub files_to_match: Option<Inputs>,
    pub match_transforms: Vec<MatchTransform>,
    /// Default: Use working directory according to containing context.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub directories_to_search: Option<Vec<PathBuf>>,
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

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum IdentityScheme {
    ContentSha256,
}

impl Default for IdentityScheme {
    fn default() -> Self {
        Self::ContentSha256
    }
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Sha256([u8; 32]);

impl Sha256 {
    pub fn new(hash: [u8; 32]) -> Self {
        Self(hash)
    }
}

impl TryFrom<&str> for Sha256 {
    type Error = anyhow::Error;

    fn try_from(hex_str: &str) -> Result<Self, Self::Error> {
        let bytes_vec = hex::decode(hex_str)?;
        let bytes_slice = bytes_vec.as_slice();
        let sha256: [u8; 32] = bytes_slice
            .try_into()
            .map_err(anyhow::Error::from)
            .with_context(|| {
                format!(
                    "expected hex string describing 32 bytes, but got {} bytes",
                    bytes_vec.len()
                )
            })?;
        Ok(Sha256(sha256))
    }
}

impl TryFrom<String> for Sha256 {
    type Error = anyhow::Error;

    fn try_from(hex_string: String) -> Result<Self, Self::Error> {
        let hex_str: &str = &hex_string;
        Sha256::try_from(hex_str)
    }
}

impl ToString for Sha256 {
    fn to_string(&self) -> String {
        hex::encode(self.0)
    }
}

impl Serialize for Sha256 {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
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
        Sha256::try_from(hex_str).map_err(|err| E::custom(format!("{:?}", err)))
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
#[serde(bound = "Identity: IdentityBound")]
pub struct Listing<Identity>
where
    Identity: IdentityBound,
{
    pub entries: Vec<Identity>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(bound = "Identity: IdentityBound")]
pub struct TaskSummary<Identity>
where
    Identity: IdentityBound,
{
    pub input: TaskInput<Identity>,
    pub output: TaskOutput<Identity>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(bound = "Identity: IdentityBound")]
pub struct TaskInput<Identity>
where
    Identity: IdentityBound,
{
    #[serde(flatten)]
    pub environment_variables: EnvironmentVariables,
    #[serde(flatten)]
    pub program: Program,
    #[serde(flatten)]
    pub arguments: Arguments,
    pub input_files: FileIdentitiesManifest<Identity>,
    pub output_files: FileIdentitiesManifest<Identity>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(bound = "Identity: IdentityBound")]
pub struct TaskOutput<Identity>
where
    Identity: IdentityBound,
{
    pub input_files_with_program: FileIdentitiesManifest<Identity>,
    pub output_files: FileIdentitiesManifest<Identity>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct FilesManifest {
    pub paths: Vec<PathBuf>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(bound = "Identity: Clone + DeserializeOwned + Serialize")]
pub struct FileIdentitiesManifest<Identity>
where
    Identity: IdentityBound,
{
    pub identity_scheme: IdentityScheme,
    pub identities: Vec<(PathBuf, Option<Identity>)>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Metadata {
    pub timestamp_nanos: i64,
    pub execution_duration_nanos: u128,
    pub system: System,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct System {
    pub name: Option<String>,
    pub long_os_version: Option<String>,
    pub kernel_version: Option<String>,
    pub distribution_id: String,
    pub total_memory: u64,
    pub estimated_num_cpu_cores: usize,
}

impl From<sysinfo::System> for System {
    fn from(system: sysinfo::System) -> Self {
        Self {
            name: system.name(),
            long_os_version: system.long_os_version(),
            kernel_version: system.kernel_version(),
            distribution_id: system.distribution_id(),
            total_memory: system.total_memory(),
            estimated_num_cpu_cores: system
                .physical_core_count()
                .unwrap_or_else(|| system.cpus().len()),
        }
    }
}
