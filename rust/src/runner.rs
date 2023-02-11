// Copyright 2023 The Artifact Executor Authors. All rights reserved.
// Use of this source code is governed by a Apache-style license that can be
// found in the LICENSE file.

use crate::identity::Identity as IdentityBound;
use crate::task::Inputs;
use crate::task::Outputs;
use anyhow::Context;
use std::process::Command;
use std::process::Stdio;

pub trait Runner<Identity: IdentityBound, Stdout: Into<Stdio>, Stderr: Into<Stdio>> {
    fn run_task(
        inputs: &Inputs<Identity>,
        stdout: Stdout,
        stderr: Stderr,
    ) -> anyhow::Result<Outputs<Identity>>;
}

pub struct SimpleRunner;

impl<Identity: IdentityBound, Stdout: Into<Stdio>, Stderr: Into<Stdio>>
    Runner<Identity, Stdout, Stderr> for SimpleRunner
{
    fn run_task(
        inputs: &Inputs<Identity>,
        stdout: Stdout,
        stderr: Stderr,
    ) -> anyhow::Result<Outputs<Identity>> {
        let mut command = Command::new(inputs.program());
        command
            .env_clear()
            .envs(inputs.environment_variables().map(|v| v.clone()))
            .args(inputs.arguments())
            .stdin(Stdio::null())
            .stdout(stdout)
            .stderr(stderr);
        let mut child = command
            .spawn()
            .map_err(anyhow::Error::from)
            .context("spawning child process")?;
        let status = child
            .wait()
            .map_err(anyhow::Error::from)
            .context("waiting for child proces to complete")?;

        if !status.success() {
            anyhow::bail!("child returned unsuccessful exit status: {}", status);
        }

        // TODO: Generate information about outpus. `inputs.outputs` has the wrong type; no file
        // identitities needed on specifying expected outputs, only on reporting actual outputs.

        Ok(())
    }
}

#[cfg(unix)]
#[cfg(test)]
mod tests {
    use super::Runner;
    use super::SimpleRunner;
    use crate::format::Sha256;
    use crate::format::TaskInput;
    use crate::format::TaskOutput;
    use std::fs::File;
    use std::io::Write;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn test_simple_program() {
        let temporary_directory = tempfile::tempdir().expect("temporary directory");
        let mut program_file =
            File::create(temporary_directory.path().join("bin")).expect("program executable");
        let stdout_file =
            File::create(temporary_directory.path().join("stdout")).expect("stdout file");
        let stderr_file =
            File::create(temporary_directory.path().join("stderr")).expect("stderr file");

        let stdout_str = "Hello, stdout\n";
        let stderr_str = "Hello, stderr\n";

        program_file
            .write_all(
                format!(
                    r#"#!/usr/bin/env bash

printf "{}" >1
printf "{}" >2
"#,
                    stdout_str, stderr_str
                )
                .as_bytes(),
            )
            .expect("write program file");

        let program_metadata = program_file.metadata().expect("program metadata");
        let mut permissions = program_metadata.permissions();
        permissions.set_mode(0o744);

        SimpleRunner::run_task(
            &TaskInput::<Sha256> {
                environment_variables: vec![],
                program: temporary_directory.path().join("bin"),
                arguments: vec![],
                input_files: vec![],
                output_files: vec![],
            }
            .try_into()
            .expect("inputs"),
            stdout_file,
            stderr_file,
        )
        .expect("run program");

        // TODO: Create and write to stdout and stderr files; execute; check file contents.
    }
}
