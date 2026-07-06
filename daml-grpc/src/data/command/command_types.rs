use std::convert::TryFrom;

use crate::data::command::create::DamlCreateCommand;
use crate::data::command::exercise::DamlExerciseCommand;
use crate::data::command::exercise_by_key::DamlExerciseByKeyCommand;
use crate::data::command::DamlCreateAndExerciseCommand;
use crate::data::{DamlError, DamlResult};
use crate::grpc_protobuf::com::daml::ledger::api::v2::command::Command as CommandKind;
use crate::grpc_protobuf::com::daml::ledger::api::v2::Command;
use crate::util::Required;

/// A Daml ledger command.
#[derive(Debug, Eq, PartialEq, Clone)]
pub enum DamlCommand {
    Create(DamlCreateCommand),
    Exercise(DamlExerciseCommand),
    ExerciseByKey(DamlExerciseByKeyCommand),
    CreateAndExercise(DamlCreateAndExerciseCommand),
}

impl From<DamlCommand> for Command {
    fn from(daml_command: DamlCommand) -> Self {
        Command {
            command: Some(match daml_command {
                DamlCommand::Create(c) => c.into(),
                DamlCommand::Exercise(c) => c.into(),
                DamlCommand::ExerciseByKey(c) => c.into(),
                DamlCommand::CreateAndExercise(c) => c.into(),
            }),
        }
    }
}

impl TryFrom<Command> for DamlCommand {
    type Error = DamlError;

    fn try_from(c: Command) -> DamlResult<Self> {
        Ok(match c.command.req()? {
            CommandKind::Create(c) => Self::Create(DamlCreateCommand::try_from(c)?),
            CommandKind::Exercise(c) => Self::Exercise(DamlExerciseCommand::try_from(c)?),
            CommandKind::ExerciseByKey(c) => Self::ExerciseByKey(DamlExerciseByKeyCommand::try_from(c)?),
            CommandKind::CreateAndExercise(c) => Self::CreateAndExercise(DamlCreateAndExerciseCommand::try_from(c)?),
        })
    }
}
