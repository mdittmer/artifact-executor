// Copyright 2023 The Artifact Executor Authors. All rights reserved.
// Use of this source code is governed by a Apache-style license that can be
// found in the LICENSE file.

use crate::fs::Filesystem as FilesystemApi;
use crate::identity::IdentityScheme as IdentitySchemeApi;
use crate::task::Inputs;
use crate::task::Outputs;
use anyhow::Context;
use std::process::Command;
use std::process::Stdio;

pub trait Runner {
    fn run_task<
        Filesystem: FilesystemApi,
        IdentityScheme: IdentitySchemeApi,
        Stdout: Into<Stdio>,
        Stderr: Into<Stdio>,
    >(
        filesystem: &mut Filesystem,
        inputs: &Inputs<IdentityScheme>,
        stdout: Stdout,
        stderr: Stderr,
    ) -> anyhow::Result<()>;
}

pub struct SimpleRunner;

impl Runner for SimpleRunner {
    fn run_task<
        Filesystem: FilesystemApi,
        IdentityScheme: IdentitySchemeApi,
        Stdout: Into<Stdio>,
        Stderr: Into<Stdio>,
    >(
        filesystem: &mut Filesystem,
        inputs: &Inputs<IdentityScheme>,
        stdout: Stdout,
        stderr: Stderr,
    ) -> anyhow::Result<()> {
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

        Ok(())
    }
}

#[cfg(unix)]
#[cfg(test)]
mod tests {
    use super::Runner;
    use super::SimpleRunner;
    use crate::fs::HostFilesystem;
    use crate::transport::Arguments;
    use crate::transport::ContentSha256;
    use crate::transport::EnvironmentVariables;
    use crate::transport::FileIdentitiesManifest;
    use crate::transport::Outputs;
    use crate::transport::TaskInput;
    use std::fs::File;
    use std::io::Write;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn test_simple_program() {
        let temporary_directory = tempfile::tempdir().expect("temporary directory");

        let stdout_str = "Hello, stdout\n";
        let stderr_str = "Hello, stderr\n";

        let program_permissions = {
            let mut program_file =
                File::create(temporary_directory.path().join("bin")).expect("program executable");

            program_file
                .write_all(
                    format!(
                        r#"#!/usr/bin/env bash

printf "{}" >&1
printf "{}" >&2
"#,
                        stdout_str, stderr_str
                    )
                    .as_bytes(),
                )
                .expect("write program file");

            let program_metadata = program_file.metadata().expect("program metadata");
            let mut program_permissions = program_metadata.permissions();
            program_permissions.set_mode(0o744);
            program_permissions
        };

        std::fs::set_permissions(temporary_directory.path().join("bin"), program_permissions)
            .expect("set program permissions");

        {
            let stdout_file =
                File::create(temporary_directory.path().join("stdout")).expect("stdout file");
            let stderr_file =
                File::create(temporary_directory.path().join("stderr")).expect("stderr file");

            let mut filesystem = HostFilesystem::try_new(temporary_directory.path().to_path_buf())
                .expect("filesystem for temporary directory");

            SimpleRunner::run_task::<HostFilesystem, ContentSha256, File, File>(
                &mut filesystem,
                &TaskInput::<ContentSha256> {
                    environment_variables: EnvironmentVariables::empty(),
                    program: temporary_directory.path().join("bin").into(),
                    arguments: Arguments::empty().into(),
                    input_files: FileIdentitiesManifest::empty(),
                    outputs_description: Outputs::empty(),
                }
                .try_into()
                .expect("inputs"),
                stdout_file,
                stderr_file,
            )
            .expect("run program");
        }

        let actual_stdout = std::fs::read_to_string(temporary_directory.path().join("stdout"))
            .expect("read stdout");
        let actual_stderr = std::fs::read_to_string(temporary_directory.path().join("stderr"))
            .expect("read stderr");

        assert_eq!(stdout_str, &actual_stdout);
        assert_eq!(stderr_str, &actual_stderr);
    }
}
