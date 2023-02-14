// Copyright 2023 The Artifact Executor Authors. All rights reserved.
// Use of this source code is governed by a Apache-style license that can be
// found in the LICENSE file.

use crate::identity::AsTransport as _;
use crate::identity::IdentityScheme as IdentitySchemeApi;
use crate::identity::IntoTransport;
use crate::manifest::Arguments;
use crate::manifest::EnvironmentVariables;
use crate::manifest::FileIdentitiesManifest;
use crate::manifest::Outputs as OutputsDescription;
use crate::manifest::Program;
use crate::transport::TaskInput;
use crate::transport::TaskOutput;
use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq)]
pub struct Inputs<IS: IdentitySchemeApi> {
    environment_variables: EnvironmentVariables,
    program: Program,
    arguments: Arguments,
    input_files: FileIdentitiesManifest<IS>,
    outputs_description: OutputsDescription,
}

impl<IS: IdentitySchemeApi> Inputs<IS> {
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

    pub fn outputs_description(&self) -> &OutputsDescription {
        &self.outputs_description
    }
}

impl<IS: IdentitySchemeApi> TryFrom<TaskInput<IS>> for Inputs<IS> {
    type Error = anyhow::Error;

    fn try_from(transport: TaskInput<IS>) -> anyhow::Result<Self> {
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

impl<IS: IdentitySchemeApi> IntoTransport for Inputs<IS> {
    type Transport = TaskInput<IS>;

    fn into_transport(self) -> TaskInput<IS> {
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
pub struct Outputs<IS: IdentitySchemeApi> {
    input_files_with_program: FileIdentitiesManifest<IS>,
    output_files: FileIdentitiesManifest<IS>,
}

impl<IS: IdentitySchemeApi> TryFrom<TaskOutput<IS>> for Outputs<IS> {
    type Error = anyhow::Error;

    fn try_from(transport: TaskOutput<IS>) -> anyhow::Result<Self> {
        Ok(Self {
            input_files_with_program: transport.input_files_with_program.try_into()?,
            output_files: transport.output_files.try_into()?,
        })
    }
}

impl<IS: IdentitySchemeApi> IntoTransport for Outputs<IS> {
    type Transport = TaskOutput<IS>;

    fn into_transport(self) -> Self::Transport {
        Self::Transport {
            input_files_with_program: self.input_files_with_program.as_transport(),
            output_files: self.output_files.as_transport(),
        }
    }
}
