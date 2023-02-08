use crate::format::TaskInput;
use crate::format::TaskOutput;
use crate::identity::AsTransport as _;
use crate::identity::Identity as IdentityBound;
use crate::identity::IntoTransport;
use crate::manifest::Arguments;
use crate::manifest::EnvironmentVariables;
use crate::manifest::FileIdentitiesManifest;
use crate::manifest::Program;

#[derive(Clone, Debug, PartialEq)]
pub struct Inputs<'a, Identity> {
    environment_variables: &'a EnvironmentVariables,
    program: &'a Program,
    arguments: &'a Arguments,
    input_files: &'a FileIdentitiesManifest<Identity>,
    output_files: &'a FileIdentitiesManifest<Identity>,
}

impl<'a, Identity> IntoTransport for Inputs<'a, Identity>
where
    Identity: IdentityBound,
{
    type Transport = TaskInput<Identity>;

    fn into_transport(self) -> TaskInput<Identity> {
        Self::Transport {
            environment_variables: self.environment_variables.as_manifest(),
            program: self.program.into(),
            arguments: self.arguments.into(),
            input_files: self.input_files.as_transport(),
            output_files: self.output_files.as_transport(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Outputs<'a, Identity> {
    input_files_with_program: &'a FileIdentitiesManifest<Identity>,
    output_files: &'a FileIdentitiesManifest<Identity>,
}

impl<'a, Identity> Outputs<'a, Identity>
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

impl<'a, Identity> IntoTransport for Outputs<'a, Identity>
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
