// Copyright 2023 The Artifact Executor Authors. All rights reserved.
// Use of this source code is governed by a Apache-style license that can be
// found in the LICENSE file.

use crate::context::diff_items_to_string;
use crate::fs::Filesystem as FilesystemApi;
use crate::identity::AsTransport;
use crate::identity::Identity as IdentityBound;
use crate::identity::IdentityScheme as IdentitySchemeApi;
use crate::identity::IntoTransport;
use crate::transport::Arguments as ArgumentsTransport;
use crate::transport::EnvironmentVariables as EnvironmentVariablesTransport;
use crate::transport::FileIdentitiesManifest as FileIdentitiesManifestTransport;
use crate::transport::FilesManifest as FilesManifestTransport;
use crate::transport::IdentityScheme;
use crate::transport::Inputs as InputsTransport;
use crate::transport::Listing as ListingTransport;
use crate::transport::Match;
use crate::transport::MatchTransform as MatchTransformTransport;
use crate::transport::Metadata as MetadataTransport;
use crate::transport::Outputs as OutputsTransport;
use crate::transport::Program as ProgramTransport;
use crate::transport::System as SystemTransport;
use crate::transport::TaskInputs as TaskInputsTransport;
use crate::transport::TaskOutputs as TaskOutputsTransport;
use anyhow::Context as _;
use regex::Regex;
use std::borrow::Borrow;
use std::borrow::Cow;
use std::cmp::Ordering;
use std::collections::HashSet;
use std::hash::Hash;
use std::hash::Hasher;
use std::io::BufRead;
use std::io::BufReader;
use std::path::Path;
use std::path::PathBuf;
use sysinfo::SystemExt;

#[derive(Clone, Debug)]
pub struct RegularExpression {
    regular_expression_string: String,
    regular_expression: Regex,
}

#[derive(Eq, Hash, Ord, PartialEq, PartialOrd)]
struct RegexStr<'a>(&'a str);

impl TryFrom<String> for RegularExpression {
    type Error = regex::Error;

    fn try_from(regular_expression_string: String) -> Result<Self, Self::Error> {
        let regular_expression = regex::Regex::new(&regular_expression_string)?;
        Ok(Self {
            regular_expression_string,
            regular_expression,
        })
    }
}

impl Hash for RegularExpression {
    fn hash<H: Hasher>(&self, state: &mut H) {
        RegexStr(&self.regular_expression_string).hash(state)
    }
}

impl PartialEq for RegularExpression {
    fn eq(&self, other: &Self) -> bool {
        RegexStr(&self.regular_expression_string) == RegexStr(&other.regular_expression_string)
    }
}

impl Eq for RegularExpression {}

impl PartialOrd<Self> for RegularExpression {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        RegexStr(&self.regular_expression_string)
            .partial_cmp(&RegexStr(&other.regular_expression_string))
    }
}

impl Ord for RegularExpression {
    fn cmp(&self, other: &Self) -> Ordering {
        RegexStr(&self.regular_expression_string).cmp(&RegexStr(&other.regular_expression_string))
    }
}

impl TryFrom<Match> for RegularExpression {
    type Error = regex::Error;

    fn try_from(match_transport: Match) -> Result<Self, Self::Error> {
        Self::try_from(match_transport.match_regular_expression)
    }
}

impl IntoTransport for RegularExpression {
    type Transport = Match;

    fn into_transport(self) -> Self::Transport {
        Self::Transport {
            match_regular_expression: self.regular_expression_string,
        }
    }
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct MatchTransform {
    match_regular_expression: RegularExpression,
    match_transform_expressions: Vec<String>,
}

impl MatchTransform {
    pub fn match_regular_expression(&self) -> &Regex {
        &self.match_regular_expression.regular_expression
    }

    pub fn match_transform_expressions(&self) -> impl Iterator<Item = &String> {
        self.match_transform_expressions.iter()
    }
}

#[cfg(test)]
impl MatchTransform {
    pub fn new(
        match_regular_expression: RegularExpression,
        match_transform_expressions: Vec<String>,
    ) -> Self {
        Self {
            match_regular_expression,
            match_transform_expressions,
        }
    }
}

impl TryFrom<MatchTransformTransport> for MatchTransform {
    type Error = regex::Error;

    fn try_from(transport: MatchTransformTransport) -> Result<Self, Self::Error> {
        Ok(Self {
            match_regular_expression: transport.match_regular_expression.try_into()?,
            match_transform_expressions: transport.match_transform_expressions,
        })
    }
}

impl IntoTransport for MatchTransform {
    type Transport = MatchTransformTransport;

    fn into_transport(self) -> Self::Transport {
        Self::Transport {
            match_regular_expression: self.match_regular_expression.regular_expression_string,
            match_transform_expressions: self.match_transform_expressions,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Listing<Identity: IdentityBound> {
    entries: HashSet<Identity>,
}

impl<Identity: IdentityBound> Listing<Identity> {
    pub fn empty() -> Self {
        Self {
            entries: HashSet::new(),
        }
    }

    pub fn put(&mut self, identity: Identity) -> bool {
        if self.entries.contains(&identity) {
            false
        } else {
            self.entries.insert(identity);
            true
        }
    }

    pub fn remove(&mut self, identity: &Identity) -> bool {
        self.entries.remove(identity)
    }
}

#[cfg(test)]
impl<Identity: IdentityBound> Listing<Identity> {
    pub fn new<I: Iterator<Item = Identity>, II: IntoIterator<Item = Identity, IntoIter = I>>(
        entries: II,
    ) -> Self {
        Self {
            entries: entries.into_iter().collect(),
        }
    }
}

impl<Identity: IdentityBound> IntoTransport for Listing<Identity> {
    type Transport = ListingTransport<Identity>;

    fn into_transport(self) -> Self::Transport {
        let mut entries: Vec<_> = self.entries.into_iter().collect();
        entries.sort();
        Self::Transport { entries }
    }
}

impl<Identity: IdentityBound> Default for Listing<Identity> {
    fn default() -> Self {
        Self {
            entries: HashSet::new(),
        }
    }
}

impl<Identity: IdentityBound> TryFrom<ListingTransport<Identity>> for Listing<Identity> {
    type Error = anyhow::Error;

    fn try_from(mut transport: ListingTransport<Identity>) -> Result<Self, Self::Error> {
        let input = transport.entries.clone();
        transport.entries.sort();
        let sorted = transport.entries;
        if input != sorted {
            return Err(
                anyhow::anyhow!("listing not sorted").context(diff_items_to_string(
                    "input vs. sorted",
                    &input,
                    &sorted,
                )),
            );
        }
        let deduped: HashSet<_> = sorted.clone().into_iter().collect();
        let deduped: Vec<_> = deduped.into_iter().collect();
        if sorted != deduped {
            return Err(anyhow::anyhow!("listing contains duplicates").context(
                diff_items_to_string("sorted vs. sorted+deduped", &sorted, &deduped),
            ));
        }
        let entries: HashSet<_> = sorted.clone().into_iter().collect();
        Ok(Self { entries })
    }
}

impl<Identity: IdentityBound> TryFrom<&ListingTransport<Identity>> for Listing<Identity> {
    type Error = anyhow::Error;

    fn try_from(transport: &ListingTransport<Identity>) -> Result<Self, Self::Error> {
        let transport: ListingTransport<Identity> = transport.clone();
        Listing::try_from(transport)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Outputs {
    include_files: HashSet<PathBuf>,
    include_match_transforms: HashSet<Vec<MatchTransform>>,
    exclude_matches: HashSet<RegularExpression>,
}

impl Outputs {
    pub fn empty() -> Self {
        Self {
            include_files: HashSet::new(),
            include_match_transforms: HashSet::new(),
            exclude_matches: HashSet::new(),
        }
    }
}

#[cfg(test)]
impl Outputs {
    pub fn new<
        P: AsRef<Path>,
        I1: Iterator<Item = P>,
        II1: IntoIterator<Item = P, IntoIter = I1>,
        I22: Iterator<Item = MatchTransform>,
        II22: IntoIterator<Item = MatchTransform, IntoIter = I22>,
        I21: Iterator<Item = II22>,
        II21: IntoIterator<Item = II22, IntoIter = I21>,
        I3: Iterator<Item = RegularExpression>,
        II3: IntoIterator<Item = RegularExpression, IntoIter = I3>,
    >(
        include_files: II1,
        include_match_transforms: II21,
        exclude_matches: II3,
    ) -> Self {
        Self {
            include_files: include_files
                .into_iter()
                .map(|path| path.as_ref().to_path_buf())
                .collect(),
            include_match_transforms: include_match_transforms
                .into_iter()
                .map(|into_iter| into_iter.into_iter().collect())
                .collect(),
            exclude_matches: exclude_matches.into_iter().collect(),
        }
    }

    /// Helper function to avoid the type system requiring that all type parameters for `Self::new`
    /// be manually specified.
    pub fn empty_include_match_transforms() -> HashSet<Vec<MatchTransform>> {
        let empty: HashSet<Vec<MatchTransform>> = HashSet::new();
        empty
    }
}

impl TryFrom<OutputsTransport> for Outputs {
    type Error = anyhow::Error;

    fn try_from(transport: OutputsTransport) -> anyhow::Result<Self> {
        let mut include_files = HashSet::new();

        for include_file in transport.include_files.into_iter() {
            if include_files.contains(&include_file) {
                anyhow::bail!(
                    "include path, {:?}, appears twice in output files description",
                    include_file
                );
            }

            include_files.insert(include_file);
        }

        let mut include_match_transforms = HashSet::new();
        for include_match_transform_series in transport.include_match_transforms.into_iter() {
            let match_transform_series = include_match_transform_series
                .into_iter()
                .map(MatchTransform::try_from)
                .collect::<Result<_, _>>()?;
            if include_match_transforms.contains(&match_transform_series) {
                anyhow::bail!(
                    "include regular expression + transform sequence, {:?}, appears twice in output files description",
                    match_transform_series
                );
            }

            include_match_transforms.insert(match_transform_series);
        }

        let mut exclude_matches = HashSet::new();
        for exclude_match in transport.exclude_matches.into_iter() {
            let exclude_match: RegularExpression = exclude_match.try_into()?;
            if exclude_matches.contains(&exclude_match) {
                anyhow::bail!(
                    "exclude regular expression, {:?}, appears twice in output files description",
                    exclude_match
                );
            }

            exclude_matches.insert(exclude_match);
        }

        Ok(Self {
            include_files,
            include_match_transforms,
            exclude_matches,
        })
    }
}

impl IntoTransport for Outputs {
    type Transport = OutputsTransport;

    fn into_transport(self) -> Self::Transport {
        let mut include_files: Vec<_> = self.include_files.into_iter().collect();
        include_files.sort();
        let mut include_match_transforms: Vec<_> = self
            .include_match_transforms
            .into_iter()
            .map(|match_transform_series| {
                match_transform_series
                    .into_iter()
                    .map(MatchTransform::into_transport)
                    .collect()
            })
            .collect();
        include_match_transforms.sort();
        let mut exclude_matches: Vec<_> = self
            .exclude_matches
            .into_iter()
            .map(RegularExpression::into_transport)
            .collect();
        exclude_matches.sort();
        Self::Transport {
            include_files,
            include_match_transforms,
            exclude_matches,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct FilesManifest {
    paths: Vec<PathBuf>,
}

impl FilesManifest {
    pub fn paths(&self) -> impl Iterator<Item = &PathBuf> {
        self.paths.iter()
    }

    pub fn into_identified<IS: IdentitySchemeApi, FS: FilesystemApi>(
        self,
        filesystem: &mut FS,
    ) -> FileIdentitiesManifest<IS> {
        FileIdentitiesManifest {
            identity_scheme: IS::IDENTITY_SCHEME,
            identities: self
                .paths
                .into_iter()
                .map(|path| match IS::identify_file(filesystem, &path) {
                    Ok(identity) => (path, Some(identity)),
                    Err(_) => (path, None),
                })
                .collect(),
        }
    }

    pub fn try_into_identified<IS: IdentitySchemeApi, FS: FilesystemApi>(
        self,
        filesystem: &mut FS,
    ) -> anyhow::Result<FileIdentitiesManifest<IS>> {
        let identities = self
            .paths
            .into_iter()
            .map(|path| match IS::identify_file(filesystem, &path) {
                Ok(identity) => Ok((path, Some(identity))),
                Err(error) => Err(error),
            })
            .collect::<Result<_, _>>()?;
        Ok(FileIdentitiesManifest {
            identity_scheme: IS::IDENTITY_SCHEME,
            identities,
        })
    }
}

#[cfg(test)]
impl FilesManifest {
    pub fn new<P: AsRef<Path>, I: Iterator<Item = P>, II: IntoIterator<Item = P, IntoIter = I>>(
        paths: II,
    ) -> Self {
        let mut paths: Vec<_> = paths
            .into_iter()
            .map(|path| path.as_ref().to_path_buf())
            .collect();
        paths.sort();
        Self { paths }
    }
}

impl IntoTransport for FilesManifest {
    type Transport = FilesManifestTransport;

    fn into_transport(self) -> Self::Transport {
        Self::Transport { paths: self.paths }
    }
}

impl<FS: FilesystemApi> TryFrom<(&mut FS, InputsTransport)> for FilesManifest {
    type Error = anyhow::Error;

    fn try_from(
        filesystem_and_description: (&mut FS, InputsTransport),
    ) -> Result<Self, Self::Error> {
        let (filesystem, description) = filesystem_and_description;
        if surely_includes_none(&description) {
            anyhow::bail!(
                "attempted to load input files configuration that always includes no files"
            );
        }

        let files = get_matching_input_files(filesystem, &description)?;

        let mut paths: Vec<PathBuf> = files.into_iter().collect();
        paths.sort();

        Ok(FilesManifest { paths })
    }
}

fn surely_includes_none(inputs_config: &InputsTransport) -> bool {
    if inputs_config.include_files.len() > 0 || inputs_config.include_globs.len() > 0 {
        return false;
    }

    for inter_file_references_config in inputs_config.inter_file_references.iter() {
        if let Some(files_to_match) = &inter_file_references_config.files_to_match {
            if !surely_includes_none(files_to_match) {
                return false;
            }
        }
    }

    true
}

/// Gets the set of files that match include/exclude pattern matching in `inputs_config`.
fn get_matching_input_files<FS: FilesystemApi>(
    filesystem: &mut FS,
    inputs_config: &InputsTransport,
) -> anyhow::Result<HashSet<PathBuf>> {
    let mut files: HashSet<PathBuf> = inputs_config
        .include_files
        .iter()
        .map(PathBuf::clone)
        .collect();
    for include_glob in inputs_config.include_globs.iter() {
        let include_path_results = filesystem.execute_glob(&include_glob)?;
        for include_path_result in include_path_results {
            match include_path_result {
                Ok(path) => {
                    files.insert(path);
                }
                Err(err) => {
                    return Err(anyhow::Error::from(err)
                        .context("error executing include-glob in inputs manifest inputs_config"));
                }
            }
        }
    }
    for exclude_glob in inputs_config.exclude_globs.iter() {
        let exclude_path_results = filesystem.execute_glob(&exclude_glob)?;
        for exclude_path_result in exclude_path_results {
            match exclude_path_result {
                Ok(path) => {
                    if files.contains(&path) {
                        files.remove(&path);
                    }
                }
                Err(err) => {
                    return Err(anyhow::Error::from(err)
                        .context("error executing exclude-glob in inputs manifest inputs_config"));
                }
            }
        }
    }
    for file in inputs_config.exclude_files.iter() {
        if files.contains(file) {
            files.remove(file);
        }
    }

    // Keep matching files until no additional files are found.
    let mut prev_num_files = files.len();
    let mut num_files = prev_num_files + 1;
    while prev_num_files < num_files {
        for inter_file_references_config in inputs_config.inter_file_references.iter() {
            // Match against either declared set of files or else initial set of files(before inter-file
            // processing.
            let matching_files = match &inter_file_references_config.files_to_match {
                Some(declared_matching_files) => Cow::Owned(get_matching_input_files(
                    filesystem,
                    declared_matching_files,
                )?),
                None => Cow::Borrowed(&files),
            };

            // Prepare regular expressions and their sets of transforms.
            let match_transforms = inter_file_references_config
                .match_transforms
                .clone()
                .into_iter()
                .map(MatchTransform::try_from)
                .collect::<Result<Vec<_>, _>>()?;

            // For all inputs whose contents should be matched to find new inputs...
            let mut matched_files = HashSet::new();
            for matching_file in matching_files.iter() {
                // Read each line.
                let reader = BufReader::new(filesystem.open_file_for_read(matching_file)?);
                for line_result in reader.lines() {
                    // Give up if reading fails at any point.
                    let line = line_result?;

                    // Attempt to find-replace each bound regex/transformer pair.
                    for MatchTransform {
                        match_regular_expression,
                        match_transform_expressions,
                    } in match_transforms.iter()
                    {
                        let regular_expression = &match_regular_expression.regular_expression;
                        for matched_text in regular_expression.find_iter(&line) {
                            // Matched regex; store each transform bound to this regex.
                            for transform in match_transform_expressions.iter() {
                                let matched_file =
                                    regular_expression.replace(matched_text.as_str(), transform);
                                let matched_path = PathBuf::from(matched_file.into_owned());
                                // Find actual file path that exists for pattern.
                                match &inter_file_references_config.directories_to_search {
                                    Some(directories) => {
                                        for directory in directories.iter() {
                                            let full_matched_path = directory.join(&matched_path);
                                            if filesystem.file_exists(&full_matched_path)
                                                && !is_shallowly_excluded(
                                                    filesystem,
                                                    inputs_config,
                                                    &full_matched_path,
                                                )?
                                            {
                                                matched_files.insert(full_matched_path);
                                                break;
                                            }
                                        }
                                    }
                                    None => {
                                        // Use matched path directly when no "directories to search"
                                        // are provided.
                                        if filesystem.file_exists(&matched_path)
                                            && !is_shallowly_excluded(
                                                filesystem,
                                                inputs_config,
                                                &matched_path,
                                            )?
                                        {
                                            matched_files.insert(matched_path);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            files.extend(matched_files.into_iter());
        }

        prev_num_files = num_files;
        num_files = files.len();
    }

    Ok(files)
}

/// Performs all non-recursive pattern matching from `inputs_config` against `path`. This function
/// is used to ensure that files added by inspecting file contents are skipped when they should be
/// categorically excluded.
fn is_shallowly_excluded<FS: FilesystemApi, P: AsRef<Path>>(
    filesystem: &mut FS,
    inputs_config: &InputsTransport,
    path: P,
) -> anyhow::Result<bool> {
    if inputs_config
        .exclude_files
        .contains(&path.as_ref().to_path_buf())
    {
        return Ok(true);
    }
    for exclude_glob in inputs_config.exclude_globs.iter() {
        if filesystem.glob_matches(exclude_glob, path.as_ref())? {
            return Ok(true);
        }
    }
    return Ok(false);
}

impl<FS: FilesystemApi> TryFrom<(&mut FS, &InputsTransport)> for FilesManifest {
    type Error = anyhow::Error;
    fn try_from(
        filesystem_and_description: (&mut FS, &InputsTransport),
    ) -> Result<Self, Self::Error> {
        let (filesystem, description) = filesystem_and_description;
        let description: InputsTransport = description.clone();
        FilesManifest::try_from((filesystem, description))
    }
}

impl TryFrom<(&FilesManifest, OutputsTransport)> for FilesManifest {
    type Error = anyhow::Error;

    fn try_from(
        inputs_and_outputs_description: (&FilesManifest, OutputsTransport),
    ) -> Result<Self, Self::Error> {
        let (inputs, description) = inputs_and_outputs_description;
        let mut files: HashSet<PathBuf> = description.include_files.into_iter().collect();

        let exclude_matches = description
            .exclude_matches
            .into_iter()
            .map(
                |Match {
                     match_regular_expression,
                 }| regex::Regex::new(&match_regular_expression)
                    .map_err(anyhow::Error::from)
                    .map_err(|err| err.context(format!(
                        "malformed exclude-regular-expression, {:?}, in outputs manifest description",
                        match_regular_expression,
                    ))),
            )
            .collect::<anyhow::Result<Vec<regex::Regex>>>()?;

        let include_match_transforms = description
            .include_match_transforms
            .into_iter()
            .map(|match_transform_series| {
                match_transform_series
                    .into_iter()
                    .map(MatchTransform::try_from)
                    .collect::<Result<Vec<_>, _>>()
            })
            .collect::<Result<Vec<_>, _>>()?;

        for input in inputs.paths() {
            let input = input.to_str().ok_or_else(|| anyhow::anyhow!("input path, {:?}, cannot be formatted as string for preparing associated output paths", input))?;

            let mut exclude_input = false;
            for exclude_match in exclude_matches.iter() {
                if exclude_match.is_match(input) {
                    exclude_input = true;
                    break;
                }
            }
            if exclude_input {
                continue;
            }

            for match_transform_series in include_match_transforms.iter() {
                let mut input_path_strings;
                let mut output_path_strings = HashSet::new();
                output_path_strings.insert(input.to_string());
                for MatchTransform {
                    match_regular_expression:
                        RegularExpression {
                            regular_expression, ..
                        },
                    match_transform_expressions,
                } in match_transform_series.iter()
                {
                    input_path_strings = output_path_strings;
                    output_path_strings = HashSet::new();
                    for input_path_string in input_path_strings.iter() {
                        if regular_expression.is_match(input_path_string) {
                            for match_transform_expression in match_transform_expressions.iter() {
                                let output_path = regular_expression
                                    .replace_all(input_path_string, match_transform_expression)
                                    .to_string();
                                output_path_strings.insert(output_path);
                            }
                        }
                    }
                }

                for output_path_string in output_path_strings.into_iter() {
                    let output_path = PathBuf::from(output_path_string);
                    if !files.contains(&output_path) {
                        files.insert(output_path);
                    }
                }
            }
        }

        let mut paths: Vec<PathBuf> = files.into_iter().collect();
        paths.sort();

        Ok(FilesManifest { paths })
    }
}

impl TryFrom<(&FilesManifest, &OutputsTransport)> for FilesManifest {
    type Error = anyhow::Error;

    fn try_from(
        inputs_and_outputs_description: (&FilesManifest, &OutputsTransport),
    ) -> Result<Self, Self::Error> {
        let (filesystem, description) = inputs_and_outputs_description;
        let description: OutputsTransport = description.clone();
        FilesManifest::try_from((filesystem, description))
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct FileIdentitiesManifest<IS: IdentitySchemeApi> {
    identity_scheme: IdentityScheme,
    identities: Vec<(PathBuf, Option<IS::Identity>)>,
}

impl<IS: IdentitySchemeApi> FileIdentitiesManifest<IS> {
    pub fn empty() -> Self {
        Self {
            identity_scheme: IS::IDENTITY_SCHEME,
            identities: vec![],
        }
    }

    pub fn identities(&self) -> impl Iterator<Item = &(PathBuf, Option<IS::Identity>)> {
        self.identities.iter()
    }
}

impl<IS: IdentitySchemeApi> IntoTransport for FileIdentitiesManifest<IS> {
    type Transport = FileIdentitiesManifestTransport<IS>;

    fn into_transport(self) -> Self::Transport {
        Self::Transport {
            identity_scheme: self.identity_scheme,
            identities: self.identities,
        }
    }
}

#[cfg(test)]
impl<IS: IdentitySchemeApi> FileIdentitiesManifest<IS> {
    pub fn new<
        P: AsRef<Path>,
        I: Iterator<Item = (P, Option<IS::Identity>)>,
        II: IntoIterator<Item = (P, Option<IS::Identity>), IntoIter = I>,
    >(
        identities: II,
    ) -> Self {
        let mut identities: Vec<_> = identities
            .into_iter()
            .map(|(path, identity)| (path.as_ref().to_path_buf(), identity))
            .collect();
        identities.sort_by(|(path1, _), (path2, _)| path1.cmp(path2));
        Self {
            identity_scheme: IS::IDENTITY_SCHEME,
            identities,
        }
    }
}

impl<IS: IdentitySchemeApi> TryFrom<FileIdentitiesManifestTransport<IS>>
    for FileIdentitiesManifest<IS>
{
    type Error = anyhow::Error;

    fn try_from(transport: FileIdentitiesManifestTransport<IS>) -> Result<Self, anyhow::Error> {
        let stated_paths: Vec<_> = transport.identities.iter().map(|(path, _)| path).collect();
        let mut sorted_paths: Vec<_> = transport.identities.iter().map(|(path, _)| path).collect();
        sorted_paths.sort();
        let sorted_paths = sorted_paths;
        if stated_paths != sorted_paths {
            return Err(
                anyhow::anyhow!("attempted to load unsorted file identities manifest").context(
                    diff_items_to_string(
                        "stated paths vs. sorted paths",
                        &stated_paths,
                        &sorted_paths,
                    ),
                ),
            );
        }

        Ok(FileIdentitiesManifest {
            identity_scheme: transport.identity_scheme,
            identities: transport.identities,
        })
    }
}

impl<IS: IdentitySchemeApi> TryFrom<&FileIdentitiesManifestTransport<IS>>
    for FileIdentitiesManifest<IS>
{
    type Error = anyhow::Error;

    fn try_from(transport: &FileIdentitiesManifestTransport<IS>) -> Result<Self, anyhow::Error> {
        let transport: FileIdentitiesManifestTransport<IS> = transport.clone();
        Self::try_from(transport)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct EnvironmentVariables {
    pub environment_variables: Vec<(String, String)>,
}

impl EnvironmentVariables {
    pub fn environment_variables(&self) -> impl Iterator<Item = &(String, String)> {
        self.environment_variables.iter()
    }

    pub fn empty() -> Self {
        Self {
            environment_variables: vec![],
        }
    }

    /// Load environment variables from a user-specified configuration. Such configurations may be
    /// out of order, but must contain no duplicates.
    pub fn try_from_config(
        mut environment_variables: EnvironmentVariablesTransport,
    ) -> Result<Self, anyhow::Error> {
        environment_variables
            .environment_variables
            .sort_by(|(key1, _), (key2, _)| key1.cmp(key2));
        let environment_variables = environment_variables.environment_variables;
        let deduped_environment_variables: HashSet<_> =
            environment_variables.clone().into_iter().collect();
        let deduped_environment_variables: Vec<_> =
            deduped_environment_variables.into_iter().collect();
        if environment_variables != deduped_environment_variables {
            return Err(
                anyhow::anyhow!("environment variables configuration contains duplicates").context(
                    diff_items_to_string(
                        "sorted vs. sorted+deduped",
                        &environment_variables,
                        &deduped_environment_variables,
                    ),
                ),
            );
        }
        Ok(Self {
            environment_variables,
        })
    }

    pub fn try_from_borrowed_config(
        environment_variables: &EnvironmentVariablesTransport,
    ) -> Result<Self, anyhow::Error> {
        let environment_variables: EnvironmentVariablesTransport = environment_variables.clone();
        Self::try_from_config(environment_variables)
    }

    /// Load environment variables from a tool-generated manifest. Such manifests must be sorted and
    /// deduplicated.
    pub fn try_from_manifest(
        mut environment_variables: EnvironmentVariablesTransport,
    ) -> Result<Self, anyhow::Error> {
        let input_environment_variables = environment_variables.environment_variables.clone();
        environment_variables
            .environment_variables
            .sort_by(|(key1, _), (key2, _)| key1.cmp(key2));
        let sorted_environment_variables = environment_variables.environment_variables;
        if input_environment_variables != sorted_environment_variables {
            return Err(
                anyhow::anyhow!("environment variables manifest is not sorted").context(
                    diff_items_to_string(
                        "input vs. sorted",
                        &input_environment_variables,
                        &sorted_environment_variables,
                    ),
                ),
            );
        }
        let deduped_environment_variables: HashSet<_> =
            sorted_environment_variables.clone().into_iter().collect();
        let deduped_environment_variables: Vec<_> =
            deduped_environment_variables.into_iter().collect();
        if sorted_environment_variables != deduped_environment_variables {
            return Err(
                anyhow::anyhow!("environment variables manifest contains duplicates").context(
                    diff_items_to_string(
                        "sorted vs. sorted+deduped",
                        &sorted_environment_variables,
                        &deduped_environment_variables,
                    ),
                ),
            );
        }
        Ok(Self {
            environment_variables: sorted_environment_variables,
        })
    }

    pub fn try_from_borrowed_manifest(
        environment_variables: &EnvironmentVariablesTransport,
    ) -> Result<Self, anyhow::Error> {
        let environment_variables: EnvironmentVariablesTransport = environment_variables.clone();
        Self::try_from_manifest(environment_variables)
    }

    pub fn into_manifest(self) -> EnvironmentVariablesTransport {
        EnvironmentVariablesTransport {
            environment_variables: self.environment_variables,
        }
    }

    pub fn as_manifest(&self) -> EnvironmentVariablesTransport {
        let self_clone: Self = self.clone();
        self_clone.into_manifest()
    }
}

#[cfg(test)]
impl EnvironmentVariables {
    pub fn new<
        KeyString,
        ValueString,
        I: Iterator<Item = (KeyString, ValueString)>,
        II: IntoIterator<Item = (KeyString, ValueString), IntoIter = I>,
    >(
        environment_variables: II,
    ) -> Self
    where
        String: From<KeyString> + From<ValueString>,
    {
        Self {
            environment_variables: environment_variables
                .into_iter()
                .map(|(key, value)| (String::from(key), String::from(value)))
                .collect(),
        }
    }
}

impl IntoTransport for EnvironmentVariables {
    type Transport = EnvironmentVariablesTransport;

    fn into_transport(self) -> Self::Transport {
        Self::Transport {
            environment_variables: self.environment_variables,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Program {
    program: PathBuf,
}

impl Program {
    pub fn program(&self) -> &PathBuf {
        &self.program
    }
}

#[cfg(test)]
impl Program {
    pub fn new<P: AsRef<Path>>(program: P) -> Self {
        Self {
            program: program.as_ref().to_path_buf(),
        }
    }
}

impl From<ProgramTransport> for Program {
    fn from(transport: ProgramTransport) -> Self {
        Self {
            program: transport.program,
        }
    }
}

impl From<&ProgramTransport> for Program {
    fn from(transport: &ProgramTransport) -> Self {
        let transport: ProgramTransport = transport.clone();
        Self::from(transport)
    }
}

impl<'a> From<&Program> for ProgramTransport {
    fn from(program: &Program) -> Self {
        let program: Program = program.clone();
        Self {
            program: program.program,
        }
    }
}

impl IntoTransport for Program {
    type Transport = ProgramTransport;

    fn into_transport(self) -> Self::Transport {
        Self::Transport {
            program: self.program,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Arguments {
    arguments: Vec<String>,
}

impl Arguments {
    pub fn empty() -> Self {
        Self { arguments: vec![] }
    }

    pub fn arguments(&self) -> impl Iterator<Item = &String> {
        self.arguments.iter()
    }
}

#[cfg(test)]
impl Arguments {
    pub fn new<S, I: Iterator<Item = S>, II: IntoIterator<Item = S, IntoIter = I>>(
        arguments: II,
    ) -> Self
    where
        String: From<S>,
    {
        Self {
            arguments: arguments.into_iter().map(String::from).collect(),
        }
    }
}

impl From<ArgumentsTransport> for Arguments {
    fn from(transport: ArgumentsTransport) -> Self {
        Self {
            arguments: transport.arguments,
        }
    }
}

impl From<&ArgumentsTransport> for Arguments {
    fn from(transport: &ArgumentsTransport) -> Self {
        let transport: ArgumentsTransport = transport.clone();
        Self::from(transport)
    }
}

impl From<&Arguments> for ArgumentsTransport {
    fn from(arguments: &Arguments) -> Self {
        let arguments: Arguments = arguments.clone();
        Self {
            arguments: arguments.arguments,
        }
    }
}

impl IntoTransport for Arguments {
    type Transport = ArgumentsTransport;

    fn into_transport(self) -> Self::Transport {
        Self::Transport {
            arguments: self.arguments,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Metadata {
    timestamp_nanos: i64,
    execution_duration_nanos: u128,
    system: System,
}

impl Metadata {
    pub fn new(timestamp_nanos: i64, execution_duration_nanos: u128, system: System) -> Self {
        Self {
            timestamp_nanos,
            execution_duration_nanos,
            system,
        }
    }
}

impl From<MetadataTransport> for Metadata {
    fn from(transport: MetadataTransport) -> Self {
        Self {
            timestamp_nanos: transport.timestamp_nanos,
            execution_duration_nanos: transport.execution_duration_nanos,
            system: transport.system.into(),
        }
    }
}

impl IntoTransport for Metadata {
    type Transport = MetadataTransport;

    fn into_transport(self) -> Self::Transport {
        Self::Transport {
            timestamp_nanos: self.timestamp_nanos,
            execution_duration_nanos: self.execution_duration_nanos,
            system: self.system.into_transport(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct System {
    name: Option<String>,
    long_os_version: Option<String>,
    kernel_version: Option<String>,
    distribution_id: String,
    total_memory: u64,
    estimated_num_cpu_cores: usize,
}

#[cfg(test)]
impl System {
    pub fn new<NameString, LongOsVerionSring, KernelVersionString, DistributionIdString>(
        name: Option<NameString>,
        long_os_version: Option<LongOsVerionSring>,
        kernel_version: Option<KernelVersionString>,
        distribution_id: DistributionIdString,
        total_memory: u64,
        estimated_num_cpu_cores: usize,
    ) -> Self
    where
        String: From<NameString>
            + From<LongOsVerionSring>
            + From<KernelVersionString>
            + From<DistributionIdString>,
    {
        Self {
            name: name.map(String::from),
            long_os_version: long_os_version.map(String::from),
            kernel_version: kernel_version.map(String::from),
            distribution_id: String::from(distribution_id),
            total_memory,
            estimated_num_cpu_cores,
        }
    }
}

impl From<&sysinfo::System> for System {
    fn from(system: &sysinfo::System) -> Self {
        System {
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

impl From<SystemTransport> for System {
    fn from(transport: SystemTransport) -> Self {
        System {
            name: transport.name,
            long_os_version: transport.long_os_version,
            kernel_version: transport.kernel_version,
            distribution_id: transport.distribution_id,
            total_memory: transport.total_memory,
            estimated_num_cpu_cores: transport.estimated_num_cpu_cores,
        }
    }
}

impl IntoTransport for System {
    type Transport = SystemTransport;

    fn into_transport(self) -> Self::Transport {
        Self::Transport {
            name: self.name,
            long_os_version: self.long_os_version,
            kernel_version: self.kernel_version,
            distribution_id: self.distribution_id,
            total_memory: self.total_memory,
            estimated_num_cpu_cores: self.estimated_num_cpu_cores,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct TaskInputs<IS: IdentitySchemeApi> {
    environment_variables: EnvironmentVariables,
    program: Program,
    arguments: Arguments,
    input_files: FileIdentitiesManifest<IS>,
    outputs_description: Outputs,
}

impl<IS: IdentitySchemeApi> TaskInputs<IS> {
    pub fn environment_variables(&self) -> impl Iterator<Item = &(String, String)> {
        self.environment_variables.environment_variables()
    }

    pub fn program(&self) -> &PathBuf {
        self.program.program()
    }

    pub fn arguments(&self) -> impl Iterator<Item = &String> {
        self.arguments.arguments()
    }

    pub fn input_files(&self) -> impl Iterator<Item = &(PathBuf, Option<IS::Identity>)> {
        self.input_files.identities()
    }

    pub fn outputs_description(&self) -> &Outputs {
        &self.outputs_description
    }

    pub fn wrap_program<FS: FilesystemApi, P: AsRef<Path>>(
        self,
        filesystem: &mut FS,
        new_program: P,
    ) -> anyhow::Result<Self> {
        let old_program_str = self.program().to_str().ok_or_else(|| {
            anyhow::anyhow!(
                "wrapping task program: previous program path, {:?} cannot be converted to string",
                new_program.as_ref()
            )
        })?;
        let mut arguments = vec![String::from(old_program_str)];
        arguments.extend(self.arguments().map(String::clone));
        let mut input_files = self.input_files().map(Clone::clone).collect::<Vec<_>>();
        let old_program_identity = IS::identify_file(filesystem, old_program_str)?;
        input_files.push((self.program().clone(), Some(old_program_identity)));
        input_files.sort();
        Ok(Self {
            environment_variables: self.environment_variables,
            program: Program {
                program: new_program.as_ref().to_path_buf(),
            },
            arguments: Arguments { arguments },
            input_files: FileIdentitiesManifest {
                identity_scheme: IS::IDENTITY_SCHEME,
                identities: input_files,
            },
            outputs_description: self.outputs_description,
        })
    }

    pub fn prepend_arguments(self, arguments: impl Iterator<Item = String>) -> Self {
        let mut arguments = arguments.collect::<Vec<_>>();
        arguments.extend(self.arguments().map(String::clone));
        Self {
            environment_variables: self.environment_variables,
            program: self.program,
            arguments: Arguments { arguments },
            input_files: self.input_files,
            outputs_description: self.outputs_description,
        }
    }
}

#[cfg(test)]
impl<IS: IdentitySchemeApi> TaskInputs<IS> {
    pub fn new(
        environment_variables: EnvironmentVariables,
        program: Program,
        arguments: Arguments,
        input_files: FileIdentitiesManifest<IS>,
        outputs_description: Outputs,
    ) -> Self {
        Self {
            environment_variables,
            program,
            arguments,
            input_files,
            outputs_description,
        }
    }
}

impl<IS: IdentitySchemeApi> TryFrom<TaskInputsTransport<IS>> for TaskInputs<IS> {
    type Error = anyhow::Error;

    fn try_from(transport: TaskInputsTransport<IS>) -> anyhow::Result<Self> {
        Ok(Self {
            environment_variables: EnvironmentVariables::try_from_manifest(
                transport.environment_variables,
            )?,
            program: transport.program.into(),
            arguments: transport.arguments.into(),
            input_files: transport.input_files.try_into()?,
            outputs_description: transport.outputs_description.try_into()?,
        })
    }
}

impl<IS: IdentitySchemeApi> IntoTransport for TaskInputs<IS> {
    type Transport = TaskInputsTransport<IS>;

    fn into_transport(self) -> Self::Transport {
        Self::Transport {
            environment_variables: self.environment_variables.as_manifest(),
            program: self.program.as_transport(),
            arguments: self.arguments.as_transport(),
            input_files: self.input_files.as_transport(),
            outputs_description: self.outputs_description.as_transport(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct TaskOutputs<IS: IdentitySchemeApi> {
    input_files_with_program: FileIdentitiesManifest<IS>,
    output_files: FileIdentitiesManifest<IS>,
}

#[cfg(test)]
impl<IS: IdentitySchemeApi> TaskOutputs<IS> {
    pub fn new(
        input_files_with_program: FileIdentitiesManifest<IS>,
        output_files: FileIdentitiesManifest<IS>,
    ) -> Self {
        Self {
            input_files_with_program,
            output_files,
        }
    }
}

impl<IS: IdentitySchemeApi> TryFrom<TaskOutputsTransport<IS>> for TaskOutputs<IS> {
    type Error = anyhow::Error;

    fn try_from(transport: TaskOutputsTransport<IS>) -> anyhow::Result<Self> {
        Ok(Self {
            input_files_with_program: transport.input_files_with_program.try_into()?,
            output_files: transport.output_files.try_into()?,
        })
    }
}

impl<IS: IdentitySchemeApi> IntoTransport for TaskOutputs<IS> {
    type Transport = TaskOutputsTransport<IS>;

    fn into_transport(self) -> Self::Transport {
        Self::Transport {
            input_files_with_program: self.input_files_with_program.as_transport(),
            output_files: self.output_files.as_transport(),
        }
    }
}

impl<FS: FilesystemApi, IS: IdentitySchemeApi> TryFrom<(&mut FS, &TaskInputs<IS>)>
    for TaskOutputs<IS>
{
    type Error = anyhow::Error;

    fn try_from(filesystem_and_inputs: (&mut FS, &TaskInputs<IS>)) -> Result<Self, Self::Error> {
        let (filesystem, inputs) = filesystem_and_inputs;

        let program_path = inputs.program().clone();
        let program_identity =
            IS::identify_file(filesystem, &program_path).context("identifying program")?;
        let mut input_files_with_program = vec![(program_path, Some(program_identity))];
        input_files_with_program.extend(
            inputs
                .input_files()
                .map(|path_and_identity| path_and_identity.clone()),
        );
        input_files_with_program.sort_by(|(path1, _), (path2, _)| path1.cmp(path2));
        let input_files_with_program = FileIdentitiesManifest {
            identity_scheme: IS::IDENTITY_SCHEME,
            identities: input_files_with_program,
        };

        let mut output_files: Vec<_> = get_matching_output_files(inputs)?.into_iter().collect();
        output_files.sort();
        let output_files = output_files
            .into_iter()
            .map(|path| {
                let identity = IS::identify_file(filesystem, &path)?;
                Ok((path, Some(identity)))
            })
            .collect::<Result<Vec<_>, anyhow::Error>>()
            .context("identifying matched output files")?;

        Ok(Self {
            input_files_with_program,
            output_files: FileIdentitiesManifest {
                identity_scheme: IS::IDENTITY_SCHEME,
                identities: output_files,
            },
        })
    }
}

/// Gets the set of files that matches transformations from  `inputs.input_files()` through
/// `inputs.outputs_description()` transformations.
fn get_matching_output_files<IS: IdentitySchemeApi>(
    inputs: &TaskInputs<IS>,
) -> anyhow::Result<HashSet<PathBuf>> {
    let outputs = inputs.outputs_description();
    let mut files: HashSet<PathBuf> = outputs.include_files.iter().map(PathBuf::clone).collect();

    for match_transforms in outputs.include_match_transforms.iter() {
        for (input_path, _) in inputs.input_files() {
            let mut output_path = input_path
                .to_str()
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "input file has path, {:?}, that cannot be encoded as a string",
                        input_path
                    )
                })?
                .to_string();
            for match_transform in match_transforms {
                let regular_expression = match_transform.match_regular_expression();
                if !regular_expression.is_match(&output_path) {
                    break;
                }

                for transform in match_transform.match_transform_expressions() {
                    output_path = match_transform
                        .match_regular_expression
                        .regular_expression
                        .replace_all(&output_path, transform)
                        .to_string();
                }

                let mut exclude = false;
                for exclude_match in outputs.exclude_matches.iter() {
                    if exclude_match
                        .regular_expression
                        .is_match(output_path.borrow())
                    {
                        exclude = true;
                        break;
                    }
                }

                if !exclude {
                    files.insert(PathBuf::from(&output_path));
                }
            }
        }
    }

    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::FilesManifest;
    use crate::fs::HostFilesystem;
    use crate::transport::Inputs as InputsTransport;
    use crate::transport::InterFileReferences;
    use crate::transport::Match;
    use crate::transport::MatchTransform;
    use crate::transport::Outputs as OutputsTransport;
    use std::convert::TryFrom;
    use std::fs::File;
    use std::io::Write;
    use std::path::PathBuf;

    #[test]
    fn test_inputs_manifest() {
        let temporary_directory = tempfile::tempdir().expect("temporary directory");
        std::fs::create_dir_all(temporary_directory.path().join("a/b/c"))
            .expect("manually create directories");
        std::fs::create_dir_all(temporary_directory.path().join("a/b/d"))
            .expect("manually create directories");
        File::create(temporary_directory.path().join("m.stu")).expect("manually create file");
        File::create(temporary_directory.path().join("a/n.stu")).expect("manually create file");
        File::create(temporary_directory.path().join("a/b/o.stu")).expect("manually create file");
        File::create(temporary_directory.path().join("a/b/p.vwx")).expect("manually create file");
        File::create(temporary_directory.path().join("a/b/c/p.vwx")).expect("manually create file");
        {
            let mut pointer_file = File::create(temporary_directory.path().join("a/b/d/p.vwx"))
                .expect("manually create file");
            // Refer to "referenced", which will resolve to "__/referenced".
            pointer_file
                .write_all("\n\nINCLUDE_FILE(referenced)\n\n".as_bytes())
                .expect("write to pointer file");
        }
        // Store "__/referenced" to be found via `INCLUDE_FILE(...)` matching.
        std::fs::create_dir_all(temporary_directory.path().join("__"))
            .expect("manually create directories");
        {
            let mut pointer_file = File::create(temporary_directory.path().join("__/referenced"))
                .expect("manually create file");
            // Refer to "b/c/p.vwx" and "referenced2" inside directory "a". The first is
            // categorically excluded. The second should match.
            pointer_file
                .write_all(
                    "\n\nINCLUDE_FILE(b/c/p.vwx)\nINCLUDE_FILE_INTERNAL(referenced2)\n".as_bytes(),
                )
                .expect("write to pointer file");
        }
        File::create(temporary_directory.path().join("a/referenced2"))
            .expect("manually create file");

        let mut host_filesystem = HostFilesystem::try_new(temporary_directory.path().to_path_buf())
            .expect("host filesystem");
        let inputs_config = InputsTransport {
            include_files: vec![PathBuf::from("a/n.stu")],
            exclude_files: vec![PathBuf::from("a/b/p.vwx")],
            include_globs: vec![String::from("a/b/**/*.vwx")],
            exclude_globs: vec![String::from("**/c/*.vwx")],
            inter_file_references: vec![
                InterFileReferences {
                    files_to_match: None,
                    // Match lines of the form `INCLUDE_FILE(file)`, resolving to path `file`.
                    match_transforms: vec![MatchTransform {
                        match_regular_expression: String::from(r#"^INCLUDE_FILE\(([^)]+)\)$"#),
                        match_transform_expressions: vec![String::from(r#"$1"#)],
                    }],
                    // Search for resolved files in `__` directory.
                    directories_to_search: Some(vec![PathBuf::from("__")]),
                },
                InterFileReferences {
                    files_to_match: Some(InputsTransport {
                        include_files: vec![],
                        exclude_files: vec![],
                        include_globs: vec![String::from("__/*")],
                        exclude_globs: vec![],
                        inter_file_references: vec![],
                    }),
                    // Match lines of the form `INCLUDE_FILE(file)`, resolving to path `file`.
                    match_transforms: vec![MatchTransform {
                        match_regular_expression: String::from(
                            r#"^INCLUDE_FILE_INTERNAL\(([^)]+)\)$"#,
                        ),
                        match_transform_expressions: vec![String::from(r#"$1"#)],
                    }],
                    // Search for resolved files in `__` directory.
                    directories_to_search: Some(vec![PathBuf::from("a")]),
                },
            ],
        };
        let inputs_manifest: FilesManifest =
            FilesManifest::try_from((&mut host_filesystem, inputs_config))
                .expect("create inputs manifest");
        assert_eq!(
            FilesManifest::new([
                // Resolved via `INCLUDE_FILE(...)` inside `a/b/d/p.vwx` file.
                "__/referenced",
                "a/n.stu",
                "a/b/d/p.vwx",
                "a/referenced2",
            ]),
            inputs_manifest
        );
    }

    #[test]
    fn test_outputs_manifest() {
        let inputs_manifest = FilesManifest::new([
            "m.stu",
            "a/n.stu",
            "a/b/o.stu",
            "a/b/p.vwx",
            "a/b/c/p.vwx",
            "a/b/d/p.vwx",
        ]);
        let outputs_config = OutputsTransport {
            include_files: vec![PathBuf::from("out/log")],
            include_match_transforms: vec![
                vec![
                    // TODO: Test multiple transforms over single path.
                    MatchTransform {
                        match_regular_expression: String::from("^(.*)[.](stu|vwx)$"),
                        match_transform_expressions: vec![
                            String::from("out/$1.out.1"),
                            String::from("out/$1.out.2"),
                        ],
                    },
                ],
                vec![MatchTransform {
                    match_regular_expression: String::from("^(.*)[.]stu$"),
                    match_transform_expressions: vec![String::from("out/$1.out.stu")],
                }],
            ],
            exclude_matches: vec![
                Match {
                    match_regular_expression: String::from("^.*/c/.*$"),
                },
                Match {
                    match_regular_expression: String::from("^.*/o[.]stu$"),
                },
            ],
        };

        let outputs_manifest: FilesManifest =
            FilesManifest::try_from((&inputs_manifest, outputs_config))
                .expect("create inputs manifest");
        assert_eq!(
            FilesManifest::new([
                "out/a/b/d/p.out.1",
                "out/a/b/d/p.out.2",
                "out/a/b/p.out.1",
                "out/a/b/p.out.2",
                "out/a/n.out.1",
                "out/a/n.out.2",
                "out/a/n.out.stu",
                "out/log",
                "out/m.out.1",
                "out/m.out.2",
                "out/m.out.stu",
            ]),
            outputs_manifest
        );
    }
}
