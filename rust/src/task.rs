// Copyright 2023 The Artifact Executor Authors. All rights reserved.
// Use of this source code is governed by a Apache-style license that can be
// found in the LICENSE file.

use crate::format::TaskInput;
use crate::format::TaskOutput;
use crate::identity::AsTransport as _;
use crate::identity::Identity as IdentityBound;
use crate::identity::IntoTransport;
use crate::manifest::Arguments;
use crate::manifest::EnvironmentVariables;
use crate::manifest::FileIdentitiesManifest;
use crate::manifest::Program;
use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq)]
pub struct Inputs<Identity: IdentityBound> {
    environment_variables: EnvironmentVariables,
    program: Program,
    arguments: Arguments,
    input_files: FileIdentitiesManifest<Identity>,
    output_files: FileIdentitiesManifest<Identity>,
}

impl<Identity: IdentityBound> Inputs<Identity> {
    pub fn environment_variables(&self) -> impl Iterator<Item = &(String, String)> {
        self.environment_variables.environment_variables()
    }

    pub fn program(&self) -> &PathBuf {
        self.program.program()
    }

    pub fn arguments(&self) -> impl Iterator<Item = &String> {
        self.arguments.arguments()
    }

    pub fn input_files(&self) -> impl Iterator<Item = &(PathBuf, Option<Identity>)> {
        self.input_files.identities()
    }

    pub fn output_files(&self) -> impl Iterator<Item = &(PathBuf, Option<Identity>)> {
        self.output_files.identities()
    }
}

impl<Identity: IdentityBound> TryFrom<TaskInput<Identity>> for Inputs<Identity> {
    type Error = anyhow::Error;

    fn try_from(transport: TaskInput<Identity>) -> anyhow::Result<Self> {
        Ok(Self {
            environment_variables: EnvironmentVariables::try_from_manifest(
                transport.environment_variables,
            )?,
            program: transport.program.into(),
            arguments: transport.arguments.into(),
            input_files: transport.input_files.try_into()?,
            output_files: transport.output_files.try_into()?,
        })
    }
}

impl<Identity> IntoTransport for Inputs<Identity>
where
    Identity: IdentityBound,
{
    type Transport = TaskInput<Identity>;

    fn into_transport(self) -> TaskInput<Identity> {
        Self::Transport {
            environment_variables: self.environment_variables.as_manifest(),
            program: self.program.as_transport(),
            arguments: self.arguments.as_transport(),
            input_files: self.input_files.as_transport(),
            output_files: self.output_files.as_transport(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Outputs<Identity: IdentityBound> {
    input_files_with_program: FileIdentitiesManifest<Identity>,
    output_files: FileIdentitiesManifest<Identity>,
}

impl<Identity: IdentityBound> TryFrom<TaskOutput<Identity>> for Outputs<Identity> {
    type Error = anyhow::Error;

    fn try_from(transport: TaskOutput<Identity>) -> anyhow::Result<Self> {
        Ok(Self {
            input_files_with_program: transport.input_files_with_program.try_into()?,
            output_files: transport.output_files.try_into()?,
        })
    }
}

impl<Identity> Outputs<Identity>
where
    Identity: IdentityBound,
{
    fn into_transport(self) -> TaskOutput<Identity> {
        TaskOutput {
            input_files_with_program: self.input_files_with_program.as_transport(),
            output_files: self.output_files.as_transport(),
        }
    }

    pub fn as_transport(&self) -> TaskOutput<Identity> {
        let self_clone: Self = self.clone();
        self_clone.into_transport()
    }
}

impl<Identity> IntoTransport for Outputs<Identity>
where
    Identity: IdentityBound,
{
    type Transport = TaskOutput<Identity>;

    fn into_transport(self) -> Self::Transport {
        Self::Transport {
            input_files_with_program: self.input_files_with_program.as_transport(),
            output_files: self.output_files.as_transport(),
        }
    }
}
