use crate::context::diff_items_to_string;
use crate::format::Arguments as ArgumentsTransport;
use crate::format::EnvironmentVariables as EnvironmentVariablesTransport;
use crate::format::FileIdentitiesManifest as FileIdentitiesManifestTransport;
use crate::format::FilesManifest as FilesManifestTransport;
use crate::format::IdentityScheme;
use crate::format::Inputs as InputsConfig;
use crate::format::Listing as ListingTransport;
use crate::format::Match;
use crate::format::MatchTransform;
use crate::format::Metadata as MetadataTransport;
use crate::format::Outputs as OutputsConfig;
use crate::format::Program as ProgramTransport;
use crate::format::System as SystemTransport;
use crate::fs::Filesystem as FilesystemApi;
use crate::identity::Identity as IdentityBound;
use crate::identity::IntoTransport;
use std::collections::HashSet;
use std::path::PathBuf;
use std::slice::Iter;

#[derive(Clone, Debug)]
pub struct Listing<Identity: IdentityBound> {
    entries: HashSet<Identity>,
}

impl<Identity: IdentityBound> Listing<Identity> {
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
pub struct FilesManifest {
    paths: Vec<PathBuf>,
}

impl FilesManifest {
    pub fn paths(&self) -> impl Iterator<Item = &PathBuf> {
        self.paths.iter()
    }
}

impl FilesManifest {
    pub fn empty() -> Self {
        Self { paths: vec![] }
    }

    #[cfg(test)]
    pub fn from_paths(mut paths: Vec<PathBuf>) -> Self {
        paths.sort();
        Self { paths }
    }

    pub fn iter(&self) -> Iter<'_, PathBuf> {
        self.paths.iter()
    }
}

impl IntoTransport for FilesManifest {
    type Transport = FilesManifestTransport;

    fn into_transport(self) -> Self::Transport {
        Self::Transport { paths: self.paths }
    }
}

impl<FS: FilesystemApi> TryFrom<(&mut FS, InputsConfig)> for FilesManifest {
    type Error = anyhow::Error;

    fn try_from(filesystem_and_description: (&mut FS, InputsConfig)) -> Result<Self, Self::Error> {
        let (filesystem, description) = filesystem_and_description;
        let mut files: HashSet<PathBuf> = description.include_files.into_iter().collect();
        for include_glob in description.include_globs {
            let include_path_results = filesystem.execute_glob(&include_glob)?;
            for include_path_result in include_path_results {
                match include_path_result {
                    Ok(path) => {
                        files.insert(path);
                    }
                    Err(err) => {
                        return Err(anyhow::Error::from(err).context(
                            "error executing include-glob in inputs manifest description",
                        ));
                    }
                }
            }
        }
        for exclude_glob in description.exclude_globs {
            let exclude_path_results = filesystem.execute_glob(&exclude_glob)?;
            for exclude_path_result in exclude_path_results {
                match exclude_path_result {
                    Ok(path) => {
                        if files.contains(&path) {
                            files.remove(&path);
                        }
                    }
                    Err(err) => {
                        return Err(anyhow::Error::from(err).context(
                            "error executing exclude-glob in inputs manifest description",
                        ));
                    }
                }
            }
        }
        for file in description.exclude_files.iter() {
            if files.contains(file) {
                files.remove(file);
            }
        }

        let mut paths: Vec<PathBuf> = files.into_iter().collect();
        paths.sort();

        Ok(FilesManifest { paths })
    }
}

impl<FS: FilesystemApi> TryFrom<(&mut FS, &InputsConfig)> for FilesManifest {
    type Error = anyhow::Error;
    fn try_from(filesystem_and_description: (&mut FS, &InputsConfig)) -> Result<Self, Self::Error> {
        let (filesystem, description) = filesystem_and_description;
        let description: InputsConfig = description.clone();
        FilesManifest::try_from((filesystem, description))
    }
}

impl TryFrom<(&FilesManifest, OutputsConfig)> for FilesManifest {
    type Error = anyhow::Error;

    fn try_from(
        inputs_and_outputs_description: (&FilesManifest, OutputsConfig),
    ) -> Result<Self, Self::Error> {
        let (inputs, description) = inputs_and_outputs_description;
        let mut files: HashSet<PathBuf> = description.include_files.into_iter().collect();

        struct MTRE {
            match_regular_expression: regex::Regex,
            match_transform_expressions: Vec<String>,
        }

        let include_match_transforms = description
            .include_match_transforms
            .into_iter()
            .map(
                |MatchTransform {
                     match_regular_expression,
                     match_transform_expressions,
                 }| {
                    Ok(MTRE {
                        match_regular_expression: regex::Regex::new(&match_regular_expression)
                            .map_err(anyhow::Error::from)
                            .map_err(|err| err.context(format!(
                                "malformed include-regular-expression, {:?}, in outputs manifest description",
                                match_regular_expression,
                            )))?,
                        match_transform_expressions,
                    })
                },
            )
            .collect::<anyhow::Result<Vec<MTRE>>>()?;
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

        for input in inputs.iter() {
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

            for MTRE {
                match_regular_expression,
                match_transform_expressions,
            } in include_match_transforms.iter()
            {
                if match_regular_expression.is_match(input) {
                    for match_transform_expression in match_transform_expressions.iter() {
                        let output_string = match_regular_expression
                            .replace_all(input, match_transform_expression)
                            .to_string();
                        let output = PathBuf::from(output_string);
                        if !files.contains(&output) {
                            files.insert(output);
                        }
                    }
                }
            }
        }

        let mut paths: Vec<PathBuf> = files.into_iter().collect();
        paths.sort();

        Ok(FilesManifest { paths })
    }
}

impl TryFrom<(&FilesManifest, &OutputsConfig)> for FilesManifest {
    type Error = anyhow::Error;

    fn try_from(
        inputs_and_outputs_description: (&FilesManifest, &OutputsConfig),
    ) -> Result<Self, Self::Error> {
        let (filesystem, description) = inputs_and_outputs_description;
        let description: OutputsConfig = description.clone();
        FilesManifest::try_from((filesystem, description))
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct FileIdentitiesManifest<Identity> {
    identity_scheme: IdentityScheme,
    identities: Vec<(PathBuf, Option<Identity>)>,
}

impl<Identity> IntoTransport for FileIdentitiesManifest<Identity>
where
    Identity: IdentityBound,
{
    type Transport = FileIdentitiesManifestTransport<Identity>;

    fn into_transport(self) -> Self::Transport {
        Self::Transport {
            identity_scheme: self.identity_scheme,
            identities: self.identities,
        }
    }
}

#[cfg(test)]
impl<Identity> FileIdentitiesManifest<Identity>
where
    Identity: IdentityBound,
{
    pub fn from_transport(mut transport: FileIdentitiesManifestTransport<Identity>) -> Self {
        transport
            .identities
            .sort_by(|(path1, _), (path2, _)| path1.cmp(path2));
        Self {
            identity_scheme: transport.identity_scheme,
            identities: transport.identities,
        }
    }

    pub fn from_borrowed_transport(transport: &FileIdentitiesManifestTransport<Identity>) -> Self {
        let transport: FileIdentitiesManifestTransport<Identity> = transport.clone();
        FileIdentitiesManifest::from_transport(transport)
    }
}

impl<Identity> TryFrom<FileIdentitiesManifestTransport<Identity>>
    for FileIdentitiesManifest<Identity>
where
    Identity: IdentityBound,
{
    type Error = anyhow::Error;

    fn try_from(
        transport: FileIdentitiesManifestTransport<Identity>,
    ) -> Result<Self, anyhow::Error> {
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

impl<Identity> TryFrom<&FileIdentitiesManifestTransport<Identity>>
    for FileIdentitiesManifest<Identity>
where
    Identity: IdentityBound,
{
    type Error = anyhow::Error;

    fn try_from(
        transport: &FileIdentitiesManifestTransport<Identity>,
    ) -> Result<Self, anyhow::Error> {
        let transport: FileIdentitiesManifestTransport<Identity> = transport.clone();
        Self::try_from(transport)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct EnvironmentVariables {
    pub environment_variables: Vec<(String, String)>,
}

impl EnvironmentVariables {
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

#[derive(Clone, Debug, PartialEq)]
pub struct Program {
    program: PathBuf,
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

#[derive(Clone, Debug, PartialEq)]
pub struct Arguments {
    arguments: Vec<String>,
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

#[derive(Clone, Debug, PartialEq)]
pub struct Metadata<Identity: IdentityBound> {
    inputs_identity: Identity,
    outputs_identity: Identity,
    timestamp_nanos: i64,
    execution_duration_nanos: u128,
    system: System,
}

impl<Identity: IdentityBound> IntoTransport for Metadata<Identity> {
    type Transport = MetadataTransport<Identity>;

    fn into_transport(self) -> Self::Transport {
        Self::Transport {
            inputs_identity: self.inputs_identity,
            outputs_identity: self.outputs_identity,
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

#[cfg(test)]
mod tests {
    use super::FilesManifest;
    use crate::format::Inputs as InputsConfig;
    use crate::format::Match;
    use crate::format::MatchTransform;
    use crate::format::Outputs as OutputsConfig;
    use crate::fs::HostFilesystem;
    use std::convert::TryFrom;
    use std::fs::File;
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
        File::create(temporary_directory.path().join("a/b/d/p.vwx")).expect("manually create file");

        let mut host_filesystem = HostFilesystem::try_new(temporary_directory.path().to_path_buf())
            .expect("host filesystem");
        let inputs_config = InputsConfig {
            include_files: vec![PathBuf::from("a/n.stu")],
            exclude_files: vec![PathBuf::from("a/b/p.vwx")],
            include_globs: vec![String::from("a/b/**/*.vwx")],
            exclude_globs: vec![String::from("**/c/*.vwx")],
        };
        let inputs_manifest: FilesManifest =
            FilesManifest::try_from((&mut host_filesystem, inputs_config))
                .expect("create inputs manifest");
        assert_eq!(
            FilesManifest::from_paths(
                vec![PathBuf::from("a/n.stu"), PathBuf::from("a/b/d/p.vwx"),]
            ),
            inputs_manifest
        );
    }

    #[test]
    fn test_outputs_manifest() {
        let inputs_manifest = FilesManifest::from_paths(vec![
            PathBuf::from("m.stu"),
            PathBuf::from("a/n.stu"),
            PathBuf::from("a/b/o.stu"),
            PathBuf::from("a/b/p.vwx"),
            PathBuf::from("a/b/c/p.vwx"),
            PathBuf::from("a/b/d/p.vwx"),
        ]);
        let outputs_config = OutputsConfig {
            include_files: vec![PathBuf::from("out/log")],
            include_match_transforms: vec![
                MatchTransform {
                    match_regular_expression: String::from("^(.*)[.](stu|vwx)$"),
                    match_transform_expressions: vec![
                        String::from("out/$1.out.1"),
                        String::from("out/$1.out.2"),
                    ],
                },
                MatchTransform {
                    match_regular_expression: String::from("^(.*)[.]stu$"),
                    match_transform_expressions: vec![String::from("out/$1.out.stu")],
                },
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
            FilesManifest::from_paths(vec![
                PathBuf::from("out/a/b/d/p.out.1"),
                PathBuf::from("out/a/b/d/p.out.2"),
                PathBuf::from("out/a/b/p.out.1"),
                PathBuf::from("out/a/b/p.out.2"),
                PathBuf::from("out/a/n.out.1"),
                PathBuf::from("out/a/n.out.2"),
                PathBuf::from("out/a/n.out.stu"),
                PathBuf::from("out/log"),
                PathBuf::from("out/m.out.1"),
                PathBuf::from("out/m.out.2"),
                PathBuf::from("out/m.out.stu"),
            ]),
            outputs_manifest
        );
    }
}
