use crate::config::Inputs as InputsConfig;
use crate::config::Match;
use crate::config::MatchTransform;
use crate::config::Outputs as OutputsConfig;
use crate::fs::Filesystem as FilesystemApi;
use std::collections::HashSet;
use std::marker::PhantomData;
use std::path::PathBuf;
use std::slice::Iter;

#[derive(Clone)]
pub struct FilesManifest<FS: FilesystemApi> {
    paths: Vec<PathBuf>,
    _fs: PhantomData<FS>,
}

#[derive(Debug, PartialEq)]
struct BareFilesManifest<'a> {
    paths: &'a Vec<PathBuf>,
}

impl<FS: FilesystemApi> PartialEq for FilesManifest<FS> {
    fn eq(&self, other: &Self) -> bool {
        BareFilesManifest { paths: &self.paths }
            == BareFilesManifest {
                paths: &other.paths,
            }
    }
}

impl<FS: FilesystemApi> std::fmt::Debug for FilesManifest<FS> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        BareFilesManifest { paths: &self.paths }.fmt(f)
    }
}

impl<FS: FilesystemApi> FilesManifest<FS> {
    pub fn empty() -> Self {
        Self {
            paths: vec![],
            _fs: PhantomData,
        }
    }

    #[cfg(test)]
    pub fn from_paths(mut paths: Vec<PathBuf>) -> Self {
        paths.sort();
        Self {
            paths,
            _fs: PhantomData,
        }
    }

    pub fn iter(&self) -> Iter<'_, PathBuf> {
        self.paths.iter()
    }
}

impl<FS: FilesystemApi> TryFrom<(&mut FS, InputsConfig)> for FilesManifest<FS> {
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

        Ok(FilesManifest {
            paths,
            _fs: PhantomData,
        })
    }
}

impl<FS: FilesystemApi> TryFrom<(&FilesManifest<FS>, OutputsConfig)> for FilesManifest<FS> {
    type Error = anyhow::Error;

    fn try_from(
        inputs_and_outputs_description: (&FilesManifest<FS>, OutputsConfig),
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

        Ok(FilesManifest {
            paths,
            _fs: PhantomData,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::FilesManifest;
    use crate::config::Inputs as InputsConfig;
    use crate::config::Match;
    use crate::config::MatchTransform;
    use crate::config::Outputs as OutputsConfig;
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
        let inputs_manifest: FilesManifest<HostFilesystem> =
            FilesManifest::try_from((&mut host_filesystem, inputs_config))
                .expect("create inputs manifest");
        assert_eq!(
            FilesManifest::<HostFilesystem>::from_paths(vec![
                PathBuf::from("a/n.stu"),
                PathBuf::from("a/b/d/p.vwx"),
            ]),
            inputs_manifest
        );
    }

    #[test]
    fn test_outputs_manifest() {
        let inputs_manifest = FilesManifest::<HostFilesystem>::from_paths(vec![
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

        let outputs_manifest: FilesManifest<HostFilesystem> =
            FilesManifest::try_from((&inputs_manifest, outputs_config))
                .expect("create inputs manifest");
        assert_eq!(
            FilesManifest::<HostFilesystem>::from_paths(vec![
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
