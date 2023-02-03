use crate::format::TaskInput;
use crate::format::TaskOutput;
use crate::manifest::Arguments;
use crate::manifest::EnvironmentVariables;
use crate::manifest::FileIdentitiesManifest;
use crate::manifest::Program;
use serde::Deserialize;
use serde::Serialize;

#[derive(Clone, Debug, PartialEq)]
pub struct Inputs<'a, Id> {
    environment_variables: &'a EnvironmentVariables,
    program: &'a Program,
    arguments: &'a Arguments,
    input_files: &'a FileIdentitiesManifest<Id>,
    output_files: &'a FileIdentitiesManifest<Id>,
}

impl<'a, Id> Inputs<'a, Id>
where
    Id: Clone + Serialize,
    for<'de2> Id: Deserialize<'de2>,
{
    pub fn into_transport(self) -> TaskInput<Id> {
        TaskInput {
            environment_variables: self.environment_variables.as_manifest(),
            program: self.program.into(),
            arguments: self.arguments.into(),
            input_files: self.input_files.as_transport(),
            output_files: self.output_files.as_transport(),
        }
    }

    pub fn as_transport(&self) -> TaskInput<Id> {
        let self_clone: Self = self.clone();
        self_clone.into_transport()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Outputs<'a, Id> {
    input_files_with_program: &'a FileIdentitiesManifest<Id>,
    output_files: &'a FileIdentitiesManifest<Id>,
}

impl<'a, Id> Outputs<'a, Id>
where
    Id: Clone + Serialize,
    for<'de2> Id: Deserialize<'de2>,
{
    fn into_transport(self) -> TaskOutput<Id> {
        TaskOutput {
            input_files_with_program: self.input_files_with_program.as_transport(),
            output_files: self.output_files.as_transport(),
        }
    }

    pub fn as_transport(&self) -> TaskOutput<Id> {
        let self_clone: Self = self.clone();
        self_clone.into_transport()
    }
}
