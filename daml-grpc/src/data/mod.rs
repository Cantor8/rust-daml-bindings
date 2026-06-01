/// Command completion status information.
pub mod completion;

/// Transaction filters.
pub mod filter;

/// Ledger offset information.
pub mod offset;

/// Ledger package information.
pub mod package;

/// The details of a Daml party.
pub mod party;

/// Participant users and their rights.
pub mod user;

/// Identity-provider configuration.
pub mod identity_provider;

/// CommandInspectionService data types — debug views of in-flight
/// commands.
pub mod inspection;

/// Daml values, records, enums & variants.
pub mod value {
    mod record;
    mod record_build;
    mod record_field;
    mod values;
    mod variant;
    pub use self::record::DamlRecord;
    pub use self::record_build::DamlRecordBuilder;
    pub use self::record_field::DamlRecordField;
    pub use self::values::DamlValue;
    pub use self::variant::{DamlEnum, DamlVariant};
}

/// Created, Archived & Exercised events.
pub mod event {
    mod archived;
    mod created;
    mod event_types;
    mod exercised;
    pub use self::archived::DamlArchivedEvent;
    pub use self::created::{DamlCreatedEvent, DamlInterfaceView};
    pub use self::event_types::DamlEvent;
    pub use self::exercised::DamlExercisedEvent;
}

/// Create and Exercise commands.
pub mod command {
    mod command_types;
    mod create;
    mod create_and_exercise;
    mod exercise;
    mod exercise_by_key;
    pub use self::command_types::DamlCommand;
    pub use self::create::DamlCreateCommand;
    pub use self::create_and_exercise::DamlCreateAndExerciseCommand;
    pub use self::exercise::DamlExerciseCommand;
    pub use self::exercise_by_key::DamlExerciseByKeyCommand;
}

mod commands;
pub use self::commands::{
    DamlCommands, DamlCommandsDeduplicationPeriod, DamlDisclosedContract, DamlMinLedgerTime, DamlPrefetchContractKey,
};

mod error;
pub use self::error::DamlError;
pub use self::error::DamlResult;

mod identifier;
pub use self::identifier::DamlIdentifier;

mod transaction;
pub use self::transaction::DamlTransaction;

/// Cross-synchronizer contract reassignment: data + commands.
pub mod reassignment;

mod configuration;
pub use self::configuration::DamlLedgerConfiguration;

mod active;
pub use active::DamlActiveContracts;

mod time_model;
pub use time_model::DamlTimeModel;

mod features;
pub use features::{
    DamlExperimentalCommandInspectionService, DamlExperimentalFeatures, DamlExperimentalStaticTime,
    DamlFeaturesDescriptor, DamlOffsetCheckpointFeature, DamlPackageFeature, DamlPartyManagementFeature,
    DamlUserManagementFeature,
};
