// Copyright 2023 The Artifact Executor Authors. All rights reserved.
// Use of this source code is governed by a Apache-style license that can be
// found in the LICENSE file.

use crate::blob::BlobCache;
use crate::blob::BlobPointerCache;
use crate::blob::BlobPointerFileCache;
use crate::blob::FileFormat;
use crate::blob::ReadDeserializer;
use crate::blob::StringSerializer;
use crate::blob::WriteSerializer;
use crate::canonical::TaskInputs;
use crate::canonical::TaskOutputs;
use crate::fs::Filesystem as FilesystemApi;
use crate::identity::AsTransport;
use crate::identity::IdentityScheme as IdentitySchemeApi;
use crate::runner::Runner;
use crate::runner::SimpleRunner;
use crate::transport::TaskInputs as TaskInputsTransport;
use crate::transport::TaskOutputs as TaskOutputsTransport;
use anyhow::Context as _;
use std::io::Cursor;

pub trait TaskExecutor<FS: FilesystemApi, IS: IdentitySchemeApi> {
    fn load_or_execute(
        &mut self,
        working_directory: &mut FS,
        inputs: &TaskInputs<IS>,
    ) -> anyhow::Result<TaskOutputs<IS>>;

    fn load_or_execute_identity(
        &mut self,
        working_directory: &mut FS,
        inputs_identity: &IS::Identity,
    ) -> anyhow::Result<TaskOutputs<IS>>;

    fn force_execute(
        &mut self,
        working_directory: &mut FS,
        inputs: &TaskInputs<IS>,
    ) -> anyhow::Result<TaskOutputs<IS>>;

    fn force_execute_identity(
        &mut self,
        working_directory: &mut FS,
        inputs_identity: &IS::Identity,
    ) -> anyhow::Result<TaskOutputs<IS>>;
}

pub struct CacheDirectoryTaskExecutor<
    FS: FilesystemApi,
    IS: IdentitySchemeApi,
    S: FileFormat + ReadDeserializer + StringSerializer + WriteSerializer,
    R: Runner,
> {
    blobs_cache: BlobCache<FS, IS, S>,
    outputs_pointers: BlobPointerCache<FS, IS, S>,
    stdouts_pointers: BlobPointerFileCache<FS, IS>,
    stderrs_pointers: BlobPointerFileCache<FS, IS>,
    runner: R,
}

impl<
        FS: FilesystemApi,
        IS: IdentitySchemeApi,
        S: FileFormat + ReadDeserializer + StringSerializer + WriteSerializer,
        R: Runner,
    > CacheDirectoryTaskExecutor<FS, IS, S, R>
{
    const DEFAULT_BLOBS_DIRECTORY: &str = "blobs";
    const DEFAULT_OUTPUTS_POINTERS_DIRECTORY: &str = "inputs_to_outputs";
    const DEFAULT_STDOUTS_POINTERS_DIRECTORY: &str = "inputs_to_stdouts";
    const DEFAULT_STDERRS_POINTERS_DIRECTORY: &str = "inputs_to_stderrs";

    pub fn new_with_runner(mut filesystem: FS, runner: R) -> anyhow::Result<Self> {
        let blobs_filesystem = filesystem
            .sub_system(Self::DEFAULT_BLOBS_DIRECTORY)
            .context("creating blobs directory")?;
        let outputs_filesystem = filesystem
            .sub_system(Self::DEFAULT_OUTPUTS_POINTERS_DIRECTORY)
            .context("creating inputs->outputs pointers directory")?;
        let stdouts_filesystem = filesystem
            .sub_system(Self::DEFAULT_STDOUTS_POINTERS_DIRECTORY)
            .context("creating stdouts directory")?;
        let stderrs_filesystem = filesystem
            .sub_system(Self::DEFAULT_STDERRS_POINTERS_DIRECTORY)
            .context("creating stderrs directory")?;

        let blobs_cache = BlobCache::new(blobs_filesystem);
        let outputs_pointers = BlobPointerCache::new(outputs_filesystem);
        let stdouts_pointers = BlobPointerFileCache::new(stdouts_filesystem);
        let stderrs_pointers = BlobPointerFileCache::new(stderrs_filesystem);

        Ok(Self {
            blobs_cache,
            outputs_pointers,
            stdouts_pointers,
            stderrs_pointers,
            runner,
        })
    }

    fn do_force_execute(
        &mut self,
        working_directory: &mut FS,
        inputs: &TaskInputs<IS>,
        inputs_identity: &IS::Identity,
    ) -> anyhow::Result<TaskOutputs<IS>> {
        let stdout_file = self
            .stdouts_pointers
            .open_file_for_write(inputs_identity)
            .context("opening stdout file for task executor")?;
        let stderr_file = self
            .stderrs_pointers
            .open_file_for_write(inputs_identity)
            .context("opening stderr file for task executor")?;
        self.runner
            .run_task(working_directory, inputs, stdout_file, stderr_file)
            .context("executing task")?;

        (working_directory, inputs)
            .try_into()
            .context("computing concrete outputs for task executor")
    }
}

impl<
        FS: FilesystemApi,
        IS: IdentitySchemeApi,
        S: FileFormat + ReadDeserializer + StringSerializer + WriteSerializer,
    > CacheDirectoryTaskExecutor<FS, IS, S, SimpleRunner>
{
    pub fn new(filesystem: FS) -> anyhow::Result<Self> {
        Self::new_with_runner(filesystem, SimpleRunner)
    }
}

impl<
        FS: FilesystemApi,
        IS: IdentitySchemeApi,
        S: FileFormat + ReadDeserializer + StringSerializer + WriteSerializer,
        R: Runner,
    > TaskExecutor<FS, IS> for CacheDirectoryTaskExecutor<FS, IS, S, R>
{
    fn load_or_execute(
        &mut self,
        working_directory: &mut FS,
        inputs: &TaskInputs<IS>,
    ) -> anyhow::Result<TaskOutputs<IS>> {
        let mut inputs_contents = vec![];
        let imports_transport = inputs.as_transport();
        S::to_writer(&mut inputs_contents, &imports_transport)
            .context("serializing inputs object for task executor")?;
        let inputs_identity = IS::identify_content(Cursor::new(inputs_contents))
            .context("identifying serialized inputs object for task executor")?;
        if let Ok(cached_outputs_identity) =
            self.outputs_pointers.read_blob_pointer(&inputs_identity)
        {
            self.blobs_cache
                .read_blob::<TaskOutputsTransport<IS>>(&cached_outputs_identity)
                .context("deserializing cached outputs description blob for task executor")?
                .try_into()
                .context("verifiying cached outputs description blob for task executor")
        } else {
            self.force_execute(working_directory, inputs)
        }
    }

    fn load_or_execute_identity(
        &mut self,
        working_directory: &mut FS,
        inputs_identity: &IS::Identity,
    ) -> anyhow::Result<TaskOutputs<IS>> {
        if let Ok(cached_outputs_identity) =
            self.outputs_pointers.read_blob_pointer(inputs_identity)
        {
            self.blobs_cache
                .read_blob::<TaskOutputsTransport<IS>>(&cached_outputs_identity)
                .context("deserializing cached outputs description blob for task executor")?
                .try_into()
                .context("verifying cached outputs description blob for task executor")
        } else {
            self.force_execute_identity(working_directory, inputs_identity)
        }
    }

    fn force_execute(
        &mut self,
        working_directory: &mut FS,
        inputs: &TaskInputs<IS>,
    ) -> anyhow::Result<TaskOutputs<IS>> {
        let mut inputs_contents = vec![];
        S::to_writer(&mut inputs_contents, &inputs.as_transport())
            .context("serializing inputs object for task executor")?;
        let inputs_identity = IS::identify_content(Cursor::new(inputs_contents))
            .context("identifying serialized inputs object for task executor")?;
        self.do_force_execute(working_directory, inputs, &inputs_identity)
    }

    fn force_execute_identity(
        &mut self,
        working_directory: &mut FS,
        inputs_identity: &IS::Identity,
    ) -> anyhow::Result<TaskOutputs<IS>> {
        let inputs: TaskInputs<IS> = self
            .blobs_cache
            .read_blob::<TaskInputsTransport<IS>>(&inputs_identity)
            .context("opening inputs blob for task executor")?
            .try_into()
            .context("deserializing inputs blob for task executor")?;
        self.do_force_execute(working_directory, &inputs, inputs_identity)
    }
}
