use crate::format::TaskInput;
use crate::format::TaskOutput;
use crate::manifest::Arguments;
use crate::manifest::EnvironmentVariables;
use crate::manifest::FileIdentitiesManifest;
use crate::manifest::Program;
use serde::de::DeserializeOwned;
use serde::Serialize;

#[derive(Clone, Debug, PartialEq)]
pub struct Inputs<'a, Identity> {
    environment_variables: &'a EnvironmentVariables,
    program: &'a Program,
    arguments: &'a Arguments,
    input_files: &'a FileIdentitiesManifest<Identity>,
    output_files: &'a FileIdentitiesManifest<Identity>,
}

impl<'a, Identity> Inputs<'a, Identity>
where
    Identity: Clone + DeserializeOwned + Serialize,
{
    pub fn into_transport(self) -> TaskInput<Identity> {
        TaskInput {
            environment_variables: self.environment_variables.as_manifest(),
            program: self.program.into(),
            arguments: self.arguments.into(),
            input_files: self.input_files.as_transport(),
            output_files: self.output_files.as_transport(),
        }
    }

    pub fn as_transport(&self) -> TaskInput<Identity> {
        let self_clone: Self = self.clone();
        self_clone.into_transport()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Outputs<'a, Identity> {
    input_files_with_program: &'a FileIdentitiesManifest<Identity>,
    output_files: &'a FileIdentitiesManifest<Identity>,
}

impl<'a, Identity> Outputs<'a, Identity>
where
    Identity: Clone + DeserializeOwned + Serialize,
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
