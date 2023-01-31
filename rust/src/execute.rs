use crate::args::Execute as ExecuteCommand;
use anyhow::Context as _;
use std::collections::HashMap;
use std::os::unix::fs::PermissionsExt as _;
use std::path::PathBuf;

fn check_program(program_executable: &PathBuf) -> anyhow::Result<PathBuf> {
    let mode = program_executable
        .metadata()
        .with_context(|| {
            format!(
                "failed to get metadata for program executable: {:?}",
                program_executable
            )
        })?
        .permissions()
        .mode();
    if mode & 0o111 == 0 {
        anyhow::bail!(
            "program executable file, {:?}, is not executable",
            program_executable
        );
    }

    Ok(program_executable.clone())
}

fn parse_environment(environment_manifest: &PathBuf) -> anyhow::Result<HashMap<String, String>> {
    let manifest_contents = std::fs::read_to_string(environment_manifest)
        .with_context(|| format!("failed to read file, {:?}, as string", environment_manifest))?;
    let meaningful_lines = manifest_contents
        .split('\n')
        .filter(|line| !line.is_empty());
    let mut key_value_map: HashMap<String, String> = HashMap::new();
    for line in meaningful_lines {
        let mut pair = line.splitn(2, "=");
        let key = pair.next().ok_or_else(|| {
            anyhow::anyhow!(
                "no key in key/value pair in file {:?}; line contents: {:?}",
                environment_manifest,
                line
            )
        })?;
        let value = pair.next().ok_or_else(|| {
            anyhow::anyhow!(
                "no value in key/value pair in file {:?}; line contents: {:?}",
                environment_manifest,
                line
            )
        })?;
        if key_value_map.contains_key(key) {
            anyhow::bail!("environment file contains repeated key: {:?}", key);
        }
        key_value_map.insert(String::from(key), String::from(value));
    }
    Ok(key_value_map)
}

fn parse_files_manifest(files_manifest: &PathBuf) -> anyhow::Result<Vec<PathBuf>> {
    let manifest_contents = std::fs::read_to_string(files_manifest)
        .with_context(|| format!("failed to read file, {:?}, as string", files_manifest))?;
    let meaningful_paths = manifest_contents
        .split('\n')
        .filter(|line| !line.is_empty())
        .map(PathBuf::from)
        .map(|path| {
            if path.is_absolute() {
                Err(anyhow::anyhow!(
                    "manifest contains absolute path: {:?}",
                    path
                ))
            } else {
                Ok(path)
            }
        });
    let paths: Result<Vec<PathBuf>, anyhow::Error> = meaningful_paths.collect();
    paths
}

fn parse_inputs(inputs_manifest: &PathBuf) -> anyhow::Result<Vec<PathBuf>> {
    parse_files_manifest(inputs_manifest)
}

fn parse_outputs(outputs_manifest: &PathBuf) -> anyhow::Result<Vec<PathBuf>> {
    parse_files_manifest(outputs_manifest)
}

pub struct Execute {
    pub command: PathBuf,
    pub environment: HashMap<String, String>,
    pub inputs: Vec<PathBuf>,
    pub outputs: Vec<PathBuf>,
}

impl Execute {
    pub fn from_command(command: ExecuteCommand) -> anyhow::Result<Execute> {
        Ok(Execute {
            command: check_program(&command.program)?,
            environment: parse_environment(&command.environment)?,
            inputs: parse_inputs(&command.inputs)?,
            outputs: parse_outputs(&command.outputs)?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::Execute;
    use crate::args::Execute as ExecuteCommand;
    use std::collections::HashMap;
    use std::io::Write as _;
    use std::os::unix::fs::PermissionsExt as _;
    use std::path::Path;
    use std::path::PathBuf;

    fn write_files(directory: &Path, files: HashMap<&'static str, &'static str>) {
        for (path, contents) in files {
            let mut file = std::fs::File::create(directory.join(path)).expect("create test file");
            file.write_all(contents.as_bytes())
                .expect("write all to test file");
        }
    }

    fn mark_as_executable<P: AsRef<Path>>(path: P) {
        let mut permissions = path
            .as_ref()
            .metadata()
            .expect("metadata for mark-as-executable")
            .permissions();
        permissions.set_mode(permissions.mode() | 0o100);
        std::fs::set_permissions(path, permissions).expect("mark-as-executable");
    }

    #[test]
    fn test_vacuous_command() {
        let temporary_directory = tempfile::tempdir().expect("temporary directory");
        println!("{:?}", temporary_directory.path());
        write_files(
            temporary_directory.path(),
            maplit::hashmap! {
                "env" => "\n",
                "inputs" => "\n",
                "outputs" => "\n",
                "program" => "#!/usr/bin/env bash\n",
            },
        );
        mark_as_executable(temporary_directory.path().join("program"));
        let command = Execute::from_command(ExecuteCommand {
            program: temporary_directory.path().join("program"),
            environment: temporary_directory.path().join("env"),
            inputs: temporary_directory.path().join("inputs"),
            outputs: temporary_directory.path().join("outputs"),
        })
        .expect("instantiate execute command");

        assert_eq!(temporary_directory.path().join("program"), command.command);
        assert_eq!(maplit::hashmap! {}, command.environment);
        assert_eq!(vec![] as Vec<PathBuf>, command.inputs);
        assert_eq!(vec![] as Vec<PathBuf>, command.outputs);
    }

    #[test]
    fn test_interesting_command() {
        let temporary_directory = tempfile::tempdir().expect("temporary directory");
        println!("{:?}", temporary_directory.path());
        write_files(
            temporary_directory.path(),
            maplit::hashmap! {
                "env" => "a=x\nb=y=z\n\n c\t=  z\t ",
                "inputs" => "a/b/c\n\nd/ e\t/f\n./x/y/z/../z\n",
                "outputs" => "a\nb\n\n c\nd ",
                "program" => "#!/usr/bin/env bash",
            },
        );
        mark_as_executable(temporary_directory.path().join("program"));
        let command = Execute::from_command(ExecuteCommand {
            program: temporary_directory.path().join("program"),
            environment: temporary_directory.path().join("env"),
            inputs: temporary_directory.path().join("inputs"),
            outputs: temporary_directory.path().join("outputs"),
        })
        .expect("instantiate execute command");

        assert_eq!(temporary_directory.path().join("program"), command.command);
        assert_eq!(
            maplit::hashmap! {
                "a".to_string() => "x".to_string(),
                "b".to_string() => "y=z".to_string(),
                " c\t".to_string() => "  z\t ".to_string(),
            },
            command.environment
        );
        assert_eq!(
            vec![
                PathBuf::from("a/b/c"),
                PathBuf::from("d/ e\t/f"),
                PathBuf::from("./x/y/z/../z"),
            ] as Vec<PathBuf>,
            command.inputs
        );
        assert_eq!(
            vec![
                PathBuf::from("a"),
                PathBuf::from("b"),
                PathBuf::from(" c"),
                PathBuf::from("d "),
            ] as Vec<PathBuf>,
            command.outputs
        );
    }
}
