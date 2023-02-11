// Copyright 2023 The Artifact Executor Authors. All rights reserved.
// Use of this source code is governed by a Apache-style license that can be
// found in the LICENSE file.

// use std::path::PathBuf;
// use crate::format::Inputs as InputsConfig;
// use crate::format::Outputs as OutputsConfig;

// fn parse_files_manifest_description(
//     files_manifest_description: &PathBuf,
// ) -> anyhow::Result<InputsConfig> {
//     let files_manifest_description_contents = std::fs::read_to_string(files_manifest_description)?;
//     Ok(json5::from_str(&files_manifest_description_contents)?)
// }

// fn parse_inputs(
//     inputs_manifest_description: &PathBuf,
// ) -> anyhow::Result<InputsConfig> {
//     parse_files_manifest_description(inputs_manifest_description)
// }

// fn parse_outputs(
//     outputs_manifest_description: &PathBuf,
// ) -> anyhow::Result<InputsConfig> {
//     parse_files_manifest_description(outputs_manifest_description)
// }

// #[cfg(test)]
// mod tests {
//     use super::ExecuteQuery;
//     use crate::args::Execute as ExecuteCommand;
//     use std::collections::HashMap;
//     use std::io::Write as _;
//     use std::os::unix::fs::PermissionsExt as _;
//     use std::path::Path;
//     use std::path::PathBuf;

//     fn write_files(directory: &Path, files: HashMap<&'static str, &'static str>) {
//         for (path, contents) in files {
//             let mut file = std::fs::File::create(directory.join(path)).expect("create test file");
//             file.write_all(contents.as_bytes())
//                 .expect("write all to test file");
//         }
//     }

//     fn mark_as_executable<P: AsRef<Path>>(path: P) {
//         let mut permissions = path
//             .as_ref()
//             .metadata()
//             .expect("metadata for mark-as-executable")
//             .permissions();
//         permissions.set_mode(permissions.mode() | 0o100);
//         std::fs::set_permissions(path, permissions).expect("mark-as-executable");
//     }

//     #[test]
//     fn test_vacuous_command() {
//         let temporary_directory = tempfile::tempdir().expect("temporary directory");
//         println!("{:?}", temporary_directory.path());
//         write_files(
//             temporary_directory.path(),
//             maplit::hashmap! {
//                 "env" => "\n",
//                 "inputs" => "\n",
//                 "outputs" => "\n",
//                 "program" => "#!/usr/bin/env bash\n",
//             },
//         );
//         mark_as_executable(temporary_directory.path().join("program"));
//         let command = ExecuteQuery::from_command(
//             temporary_directory.path().to_path_buf(),
//             ExecuteCommand {
//                 program: temporary_directory.path().join("program"),
//                 environment: temporary_directory.path().join("env"),
//                 inputs: temporary_directory.path().join("inputs"),
//                 outputs: temporary_directory.path().join("outputs"),
//             },
//         )
//         .expect("instantiate execute command");

//         assert_eq!(temporary_directory.path().join("program"), command.command);
//         assert_eq!(maplit::hashmap! {}, command.environment);
//         assert_eq!(vec![] as Vec<PathBuf>, command.inputs);
//         assert_eq!(vec![] as Vec<PathBuf>, command.outputs);
//     }

//     #[test]
//     fn test_interesting_command() {
//         let temporary_directory = tempfile::tempdir().expect("temporary directory");
//         println!("{:?}", temporary_directory.path());
//         write_files(
//             temporary_directory.path(),
//             maplit::hashmap! {
//                 "env" => "a=x\nb=y=z\n\n c\t=  z\t ",
//                 "inputs" => "a/b/c\n\nd/ e\t/f\n./x/y/z/../z\n",
//                 "outputs" => "a\nb\n\n c\nd ",
//                 "program" => "#!/usr/bin/env bash",
//             },
//         );
//         mark_as_executable(temporary_directory.path().join("program"));
//         let command = ExecuteQuery::from_command(
//             temporary_directory.path().to_path_buf(),
//             ExecuteCommand {
//                 program: temporary_directory.path().join("program"),
//                 environment: temporary_directory.path().join("env"),
//                 inputs: temporary_directory.path().join("inputs"),
//                 outputs: temporary_directory.path().join("outputs"),
//             },
//         )
//         .expect("instantiate execute command");

//         assert_eq!(temporary_directory.path().join("program"), command.command);
//         assert_eq!(
//             maplit::hashmap! {
//                 "a".to_string() => "x".to_string(),
//                 "b".to_string() => "y=z".to_string(),
//                 " c\t".to_string() => "  z\t ".to_string(),
//             },
//             command.environment
//         );
//         assert_eq!(
//             vec![
//                 PathBuf::from("a/b/c"),
//                 PathBuf::from("d/ e\t/f"),
//                 PathBuf::from("./x/y/z/../z"),
//             ] as Vec<PathBuf>,
//             command.inputs
//         );
//         assert_eq!(
//             vec![
//                 PathBuf::from("a"),
//                 PathBuf::from("b"),
//                 PathBuf::from(" c"),
//                 PathBuf::from("d "),
//             ] as Vec<PathBuf>,
//             command.outputs
//         );
//     }
// }
