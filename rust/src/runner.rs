// Copyright 2023 The Artifact Executor Authors. All rights reserved.
// Use of this source code is governed by a Apache-style license that can be
// found in the LICENSE file.

use crate::canonical::TaskInputs;
use crate::fs::Filesystem as FilesystemApi;
use crate::identity::IdentityScheme as IdentitySchemeApi;
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
        &mut self,
        filesystem: &mut Filesystem,
        inputs: &TaskInputs<IdentityScheme>,
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
        &mut self,
        filesystem: &mut Filesystem,
        inputs: &TaskInputs<IdentityScheme>,
        stdout: Stdout,
        stderr: Stderr,
    ) -> anyhow::Result<()> {
        let working_directory = filesystem.working_directory();
        if working_directory.is_none() && inputs.program().is_relative() {
            anyhow::bail!("attempted to run task filesystem that has no working directory, but relative program with relative path, {:?}", inputs.program());
        }
        let working_directory = working_directory.unwrap();

        let program = if inputs.program().is_absolute() {
            std::borrow::Cow::Borrowed(inputs.program())
        } else {
            std::borrow::Cow::Owned(working_directory.join(inputs.program()))
        };

        let mut command = Command::new(program.as_path());
        command
            .current_dir(working_directory)
            .env_clear()
            .envs(inputs.environment_variables().map(|v| v.clone()))
            .args(inputs.arguments())
            .stdin(Stdio::null())
            .stdout(stdout)
            .stderr(stderr);
        let mut child = command
            .spawn()
            .map_err(anyhow::Error::from)
            .with_context(|| {
                format!("spawning child process for binary, {:?}", program.as_path())
            })?;
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
mod unix {
    use super::Runner;
    use crate::blob::JSON;
    use crate::canonical::TaskInputs;
    use crate::fs::Filesystem as FilesystemApi;
    use crate::identity::IdentityScheme as IdentitySchemeApi;
    use std::path::{Path, PathBuf};
    use std::process::Stdio;

    pub type TimedRunDeserializer = JSON;

    pub const DEFAULT_TIME_UTILITY_PATH: &str = "/usr/bin/time";
    pub const TIME_FORMAT_SPECIFIER: &str =
        r#"{"wall_clock_seconds":%e,"user_mode_seconds":%U,"kernel_mode_seconds":%S}"#;

    pub struct TimedRunner<R: Runner> {
        time_program_path: PathBuf,
        time_output_path: PathBuf,
        delegate: R,
    }

    impl<R: Runner> TimedRunner<R> {
        pub fn try_new<P: AsRef<Path>>(time_output_path: P, delegate: R) -> anyhow::Result<Self> {
            time_output_path.as_ref().to_str().ok_or_else(|| {
                anyhow::anyhow!(
                    "time utility output path, {:?}, cannot be formatted as string",
                    time_output_path.as_ref()
                )
            })?;

            Ok(Self {
                time_program_path: PathBuf::from(DEFAULT_TIME_UTILITY_PATH),
                time_output_path: time_output_path.as_ref().to_path_buf(),
                delegate,
            })
        }
    }

    impl<R: Runner> Runner for TimedRunner<R> {
        fn run_task<
            Filesystem: FilesystemApi,
            IdentityScheme: IdentitySchemeApi,
            Stdout: Into<Stdio>,
            Stderr: Into<Stdio>,
        >(
            &mut self,
            filesystem: &mut Filesystem,
            inputs: &TaskInputs<IdentityScheme>,
            stdout: Stdout,
            stderr: Stderr,
        ) -> anyhow::Result<()> {
            let inputs = inputs
                .clone()
                .wrap_program(filesystem, &self.time_program_path)?
                .prepend_arguments(
                    [
                        String::from("-o"),
                        String::from(
                            self.time_output_path
                                .to_str()
                                .expect("time utility output path can be formatted as string"),
                        ),
                        String::from("-f"),
                        String::from(TIME_FORMAT_SPECIFIER),
                    ]
                    .into_iter(),
                );

            self.delegate.run_task(filesystem, &inputs, stdout, stderr)
        }
    }
}

#[cfg(unix)]
pub const DEFAULT_TIME_UTILITY_PATH: &str = unix::DEFAULT_TIME_UTILITY_PATH;

#[cfg(unix)]
pub type TimedRunDeserializer = unix::TimedRunDeserializer;

#[cfg(unix)]
pub type TimedRunner<R> = unix::TimedRunner<R>;

#[cfg(target_os = "linux")]
mod linux {
    use super::Runner;
    use crate::canonical::TaskInputs;
    use crate::fs::Filesystem as FilesystemApi;
    use crate::identity::IdentityScheme as IdentitySchemeApi;
    use std::path::{Path, PathBuf};
    use std::process::Stdio;

    pub struct TracedRunner<R: Runner> {
        fsatrace_path: PathBuf,
        fsatrace_output_path: PathBuf,
        delegate: R,
    }

    impl<R: Runner> TracedRunner<R> {
        pub fn try_new<ToolPath: AsRef<Path>, OutputPath: AsRef<Path>>(
            fsatrace_path: ToolPath,
            fsatrace_output_path: OutputPath,
            delegate: R,
        ) -> anyhow::Result<Self> {
            fsatrace_path.as_ref().to_str().ok_or_else(|| {
                anyhow::anyhow!(
                    "fsatrace utility path, {:?}, cannot be formatted as string",
                    fsatrace_path.as_ref()
                )
            })?;
            fsatrace_output_path.as_ref().to_str().ok_or_else(|| {
                anyhow::anyhow!(
                    "fsatrace utility output path, {:?}, cannot be formatted as string",
                    fsatrace_output_path.as_ref()
                )
            })?;

            Ok(Self {
                fsatrace_path: fsatrace_path.as_ref().to_path_buf(),
                fsatrace_output_path: fsatrace_output_path.as_ref().to_path_buf(),
                delegate,
            })
        }
    }

    impl<R: Runner> Runner for TracedRunner<R> {
        fn run_task<
            Filesystem: FilesystemApi,
            IdentityScheme: IdentitySchemeApi,
            Stdout: Into<Stdio>,
            Stderr: Into<Stdio>,
        >(
            &mut self,
            filesystem: &mut Filesystem,
            inputs: &TaskInputs<IdentityScheme>,
            stdout: Stdout,
            stderr: Stderr,
        ) -> anyhow::Result<()> {
            let inputs = inputs
                .clone()
                .wrap_program(filesystem, &self.fsatrace_path)?
                .prepend_arguments(
                    [
                        String::from("rwmd"),
                        String::from(
                            self.fsatrace_output_path
                                .to_str()
                                .expect("fsatrace utility output path can be formatted as string"),
                        ),
                        String::from("--"),
                    ]
                    .into_iter(),
                );

            self.delegate.run_task(filesystem, &inputs, stdout, stderr)
        }
    }
}

#[cfg(target_os = "linux")]
pub type TracedRunner<R> = linux::TracedRunner<R>;

#[cfg(unix)]
#[cfg(test)]
mod tests {
    use super::Runner;
    use super::SimpleRunner;
    use super::TimedRunDeserializer;
    use super::TimedRunner;
    use crate::blob::ReadDeserializer;
    use crate::canonical::Arguments;
    use crate::canonical::EnvironmentVariables;
    use crate::canonical::FileIdentitiesManifest;
    use crate::canonical::Outputs;
    use crate::canonical::Program;
    use crate::canonical::TaskInputs;
    use crate::fs::Filesystem as FilesystemApi;
    use crate::fs::HostFilesystem;
    use crate::identity::IdentityScheme as IdentitySchemeApi;
    use crate::transport::ContentSha256;
    use crate::transport::TaskRunTime;
    use std::fs::File;
    use std::io::Write;
    use std::os::unix::fs::PermissionsExt;
    use std::path::Path;
    use std::path::PathBuf;
    use std::process::Stdio;

    struct AssertInputFileRunner {
        input_file_path: PathBuf,
    }

    impl AssertInputFileRunner {
        fn new(input_file_path: PathBuf) -> Self {
            Self { input_file_path }
        }
    }

    impl Runner for AssertInputFileRunner {
        fn run_task<
            Filesystem: FilesystemApi,
            IdentityScheme: IdentitySchemeApi,
            Stdout: Into<Stdio>,
            Stderr: Into<Stdio>,
        >(
            &mut self,
            _filesystem: &mut Filesystem,
            inputs: &crate::canonical::TaskInputs<IdentityScheme>,
            _stdout: Stdout,
            _stderr: Stderr,
        ) -> anyhow::Result<()> {
            for (input_file_path, _) in inputs.input_files() {
                if input_file_path == &self.input_file_path {
                    return Ok(());
                }
            }
            anyhow::bail!("missing expected input: {:?}", self.input_file_path);
        }
    }

    fn create_and_set_permissions<P: AsRef<Path>>(
        description: &str,
        path: P,
        contents: &[u8],
        mode: u32,
    ) {
        let mut file = File::create(&path).expect(&format!("create: {}", description));

        file.write_all(contents)
            .expect(&format!("write file: {}", description));

        let metadata = file
            .metadata()
            .expect(&format!("get metadata: {}", description));
        let mut permissions = metadata.permissions();
        permissions.set_mode(mode);

        std::fs::set_permissions(&path, permissions)
            .expect(&format!("set permissions: {}", description));
    }

    #[test]
    fn test_simple_program() {
        let temporary_directory = tempfile::tempdir().expect("temporary directory");
        let dir_path = temporary_directory.path();
        let bin_path = dir_path.join("bin");
        let stdout_path = dir_path.join("stdout");
        let stderr_path = dir_path.join("stderr");

        let stdout_str = "Hello, stdout\n";
        let stderr_str = "Hello, stderr\n";

        create_and_set_permissions(
            "program",
            &bin_path,
            format!(
                r#"#!/usr/bin/env bash

printf "{}" >&1
printf "{}" >&2
"#,
                stdout_str, stderr_str,
            )
            .as_bytes(),
            0o744,
        );

        {
            let stdout_file = File::create(&stdout_path).expect("stdout file");
            let stderr_file = File::create(&stderr_path).expect("stderr file");

            let mut filesystem = HostFilesystem::try_new(dir_path.to_path_buf())
                .expect("filesystem for temporary directory");

            let mut runner = SimpleRunner;
            runner
                .run_task::<HostFilesystem, ContentSha256, File, File>(
                    &mut filesystem,
                    &TaskInputs::<ContentSha256>::new(
                        EnvironmentVariables::empty(),
                        Program::new(bin_path),
                        Arguments::empty(),
                        FileIdentitiesManifest::<ContentSha256>::empty(),
                        Outputs::empty(),
                    ),
                    stdout_file,
                    stderr_file,
                )
                .expect("run program");
        }

        let actual_stdout = std::fs::read_to_string(&stdout_path).expect("read stdout");
        let actual_stderr = std::fs::read_to_string(&stderr_path).expect("read stderr");

        assert_eq!(stdout_str, &actual_stdout);
        assert_eq!(stderr_str, &actual_stderr);
    }

    #[test]
    fn test_missing_executable_permission() {
        let temporary_directory = tempfile::tempdir().expect("temporary directory");
        let dir_path = temporary_directory.path();
        let bin_path = dir_path.join("bin");
        let stdout_path = dir_path.join("stdout");
        let stderr_path = dir_path.join("stderr");

        create_and_set_permissions(
            "program",
            &bin_path,
            r#"#!/usr/bin/env bash

printf "Hello, World!" >&1
"#
            .as_bytes(),
            // Note: No executable permissions.
            0o644,
        );

        {
            let stdout_file = File::create(&stdout_path).expect("stdout file");
            let stderr_file = File::create(&stderr_path).expect("stderr file");

            let mut filesystem = HostFilesystem::try_new(dir_path.to_path_buf())
                .expect("filesystem for temporary directory");

            let mut runner = SimpleRunner;
            runner
                .run_task::<HostFilesystem, ContentSha256, File, File>(
                    &mut filesystem,
                    &TaskInputs::<ContentSha256>::new(
                        EnvironmentVariables::empty(),
                        Program::new(bin_path),
                        Arguments::empty(),
                        FileIdentitiesManifest::<ContentSha256>::empty(),
                        Outputs::empty(),
                    ),
                    stdout_file,
                    stderr_file,
                )
                .expect_err("run program");
        }
    }

    #[test]
    fn test_bad_exit_code() {
        let temporary_directory = tempfile::tempdir().expect("temporary directory");
        let dir_path = temporary_directory.path();
        let bin_path = dir_path.join("bin");
        let stdout_path = dir_path.join("stdout");
        let stderr_path = dir_path.join("stderr");

        create_and_set_permissions(
            "program",
            &bin_path,
            // Note: `exit 1` causes non-OK exit code.
            r#"#!/usr/bin/env bash

printf "ERROR!" >&2
exit 1
"#
            .as_bytes(),
            0o744,
        );

        {
            let stdout_file = File::create(&stdout_path).expect("stdout file");
            let stderr_file = File::create(&stderr_path).expect("stderr file");

            let mut filesystem = HostFilesystem::try_new(dir_path.to_path_buf())
                .expect("filesystem for temporary directory");

            let mut runner = SimpleRunner;
            runner
                .run_task::<HostFilesystem, ContentSha256, File, File>(
                    &mut filesystem,
                    &TaskInputs::<ContentSha256>::new(
                        EnvironmentVariables::empty(),
                        Program::new(bin_path),
                        Arguments::empty(),
                        FileIdentitiesManifest::<ContentSha256>::empty(),
                        Outputs::empty(),
                    ),
                    stdout_file,
                    stderr_file,
                )
                .expect_err("run program");
        }
    }

    #[test]
    fn test_time_forwards_input() {
        let temporary_directory = tempfile::tempdir().expect("temporary directory");
        let dir_path = temporary_directory.path();
        let bin_path = dir_path.join("bin");
        let stdout_path = dir_path.join("stdout");
        let stderr_path = dir_path.join("stderr");
        let time_path = dir_path.join("time");

        create_and_set_permissions(
            "program",
            &bin_path,
            format!(
                r#"#!/usr/bin/env bash
"#
            )
            .as_bytes(),
            0o744,
        );

        {
            let stdout_file = File::create(&stdout_path).expect("stdout file");
            let stderr_file = File::create(&stderr_path).expect("stderr file");

            let mut filesystem = HostFilesystem::try_new(dir_path.to_path_buf())
                .expect("filesystem for temporary directory");

            let mut runner =
                TimedRunner::try_new(time_path, AssertInputFileRunner::new(bin_path.clone()))
                    .expect("timed runner");
            runner
                .run_task::<HostFilesystem, ContentSha256, File, File>(
                    &mut filesystem,
                    &TaskInputs::<ContentSha256>::new(
                        EnvironmentVariables::empty(),
                        Program::new(bin_path),
                        Arguments::empty(),
                        FileIdentitiesManifest::<ContentSha256>::empty(),
                        Outputs::empty(),
                    ),
                    stdout_file,
                    stderr_file,
                )
                .expect("run program");
        }
    }

    #[test]
    fn test_time_output() {
        let temporary_directory = tempfile::tempdir().expect("temporary directory");
        let dir_path = temporary_directory.path();
        let bin_path = dir_path.join("bin");
        let stdout_path = dir_path.join("stdout");
        let stderr_path = dir_path.join("stderr");
        let time_path = dir_path.join("time");

        create_and_set_permissions(
            "program",
            &bin_path,
            format!(
                r#"#!/usr/bin/env bash
"#
            )
            .as_bytes(),
            0o744,
        );

        {
            let stdout_file = File::create(&stdout_path).expect("stdout file");
            let stderr_file = File::create(&stderr_path).expect("stderr file");

            let mut filesystem = HostFilesystem::try_new(dir_path.to_path_buf())
                .expect("filesystem for temporary directory");

            let mut runner = TimedRunner::try_new(&time_path, SimpleRunner).expect("traced runner");
            runner
                .run_task::<HostFilesystem, ContentSha256, File, File>(
                    &mut filesystem,
                    &TaskInputs::<ContentSha256>::new(
                        EnvironmentVariables::empty(),
                        Program::new(bin_path),
                        Arguments::empty(),
                        FileIdentitiesManifest::<ContentSha256>::empty(),
                        Outputs::empty(),
                    ),
                    stdout_file,
                    stderr_file,
                )
                .expect("run program");
        }

        let time_file = File::open(&time_path).expect("time file");
        let _time: TaskRunTime =
            TimedRunDeserializer::from_reader(time_file).expect("deserialize task run time");
    }

    #[cfg(target_os = "linux")]
    #[cfg(test)]
    mod linux {
        use super::create_and_set_permissions;
        use crate::canonical::Arguments;
        use crate::canonical::EnvironmentVariables;
        use crate::canonical::FilesManifest;
        use crate::canonical::Outputs;
        use crate::canonical::Program;
        use crate::canonical::TaskInputs;
        use crate::fs::HostFilesystem;
        use crate::runner::Runner;
        use crate::runner::SimpleRunner;
        use crate::runner::TracedRunner;
        use crate::transport::ContentSha256;
        use std::collections::HashSet;
        use std::fs::File;
        use std::path::Path;

        const FSATRACE_BINARY_PATH: &str = "../fsatrace/fsatrace";
        const FSATRACE_LIBRARY_PATH: &str = "../fsatrace/fsatrace.so";

        #[test]
        fn test_traced_runner() {
            // let temporary_directory = tempfile::tempdir().expect("temporary directory");
            // let dir_path = temporary_directory.path();

            let tmp_dir_str = "/tmp/stable";
            let _ = std::fs::remove_dir_all(tmp_dir_str);
            std::fs::create_dir_all(tmp_dir_str).expect("create temporary directory");
            let dir_path: &Path = tmp_dir_str.as_ref();

            let bin_path = dir_path.join("bin");
            let stdout_path = dir_path.join("stdout");
            let stderr_path = dir_path.join("stderr");
            let trace_path = dir_path.join("trace");

            let cp_path_str = "/usr/bin/cp";
            let in_src_path_str = "in_src";
            let out_extra_path_str = "out_extra";
            let out_dst_path_str = "out_dst";

            let copied_str = "copied\n";
            let extra_str = "extra\n";

            create_and_set_permissions(
                "program",
                &bin_path,
                format!(
                    r#"#!/usr/bin/env bash

"{}" "$1" "$2"
printf "{}" > "$3"
"#,
                    cp_path_str, extra_str,
                )
                .as_bytes(),
                0o744,
            );

            std::fs::write(dir_path.join(in_src_path_str), copied_str.as_bytes())
                .expect("write in_src file");

            std::fs::copy(FSATRACE_BINARY_PATH, dir_path.join("fsatrace"))
                .expect("copy fsatrace into place");
            std::fs::copy(FSATRACE_LIBRARY_PATH, dir_path.join("fsatrace.so"))
                .expect("copy fsatrace library into place");

            {
                let stdout_file = File::create(&stdout_path).expect("stdout file");
                let stderr_file = File::create(&stderr_path).expect("stderr file");

                let mut filesystem = HostFilesystem::try_new(dir_path.to_path_buf())
                    .expect("filesystem for temporary directory");

                let input_files = FilesManifest::new([cp_path_str, in_src_path_str])
                    .try_into_identified::<ContentSha256, HostFilesystem>(&mut filesystem)
                    .expect("file identities manifest");

                let outputs_description = Outputs::new(
                    [out_extra_path_str, out_dst_path_str],
                    Outputs::empty_include_match_transforms(),
                    [],
                );

                let mut runner = TracedRunner::try_new("fsatrace", &trace_path, SimpleRunner)
                    .expect("traced runner");
                runner
                    .run_task::<HostFilesystem, ContentSha256, File, File>(
                        &mut filesystem,
                        &TaskInputs::<ContentSha256>::new(
                            EnvironmentVariables::empty(),
                            Program::new(bin_path.clone()),
                            Arguments::new([in_src_path_str, out_dst_path_str, out_extra_path_str]),
                            input_files,
                            outputs_description,
                        ),
                        stdout_file,
                        stderr_file,
                    )
                    .expect("run program");
            }

            let trace_file_contents =
                std::fs::read_to_string(&trace_path).expect("read trace file");
            let trace_file_lines: HashSet<&str> = trace_file_contents.split("\n").collect();
            let dir_path_str = dir_path.to_str().expect("temporary directory as str");

            // Note: Test does not expect perfect hermeticity; running a shell script often touches
            // files such as `/usr/bin/env`, `/usr/bin/bash`, `/etc/passwd*`.

            // Look for reads and writes: read `cp`, read `bin`, read `in_src`, write `out_dst`,
            // write `out_extra`.
            assert!(trace_file_lines
                .contains(format!("r|{}", bin_path.to_str().expect("bin path as str")).as_str()));
            assert!(trace_file_lines.contains(format!("r|{}", cp_path_str).as_str()));
            assert!(trace_file_lines
                .contains(format!("r|{}/{}", dir_path_str, in_src_path_str).as_str()));
            assert!(trace_file_lines
                .contains(format!("w|{}/{}", dir_path_str, out_dst_path_str).as_str()));
            assert!(trace_file_lines
                .contains(format!("w|{}/{}", dir_path_str, out_extra_path_str).as_str()));

            // Look for "wrong read/write" operations.
            assert!(!trace_file_lines
                .contains(format!("w|{}", bin_path.to_str().expect("bin path as str")).as_str()));
            assert!(!trace_file_lines.contains(format!("w|{}", cp_path_str).as_str()));
            assert!(!trace_file_lines
                .contains(format!("w|{}/{}", dir_path_str, in_src_path_str).as_str()));
            assert!(!trace_file_lines
                .contains(format!("r|{}/{}", dir_path_str, out_dst_path_str).as_str()));
            assert!(!trace_file_lines
                .contains(format!("r|{}/{}", dir_path_str, out_extra_path_str).as_str()));

            // Look for modify or delete operations (there should be none).
            assert!(trace_file_lines
                .iter()
                .filter(|line| line.starts_with("m"))
                .next()
                .is_none());
            assert!(trace_file_lines
                .iter()
                .filter(|line| line.starts_with("d"))
                .next()
                .is_none());
        }
    }
}
