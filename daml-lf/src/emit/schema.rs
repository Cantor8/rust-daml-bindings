//! What a package should contain.
//!
//! Deliberately small: it carries what a participant reads from a package —
//! record types, template signatures, and the parties a contract is disclosed
//! to. How choices behave is not described, because an engine that interprets
//! the templates elsewhere supplies that.

use crate::error::{DamlLfError, DamlLfResult};

/// A template referred to by a contract-id field. The template lives in the
/// package being built.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TemplateRef {
    /// Dotted module name, e.g. `Fuji.Asset`.
    pub module: String,
    pub name: String,
}

/// A field's type.
///
/// Covers a builtin, or a builtin applied to another such type. A field
/// holding a record, variant or enum would need that type defined alongside
/// it, which a package built this way does not carry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FieldType {
    Unit,
    Bool,
    Int64,
    Numeric,
    Text,
    Timestamp,
    Date,
    Party,
    ContractId(TemplateRef),
    List(Box<FieldType>),
    Optional(Box<FieldType>),
}

/// What a choice returns, which is drawn from the same vocabulary as a field.
pub type ResultType = FieldType;

/// One field of a template's payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Field {
    pub name: String,
    pub field_type: FieldType,
}

impl Field {
    pub fn new(name: impl Into<String>, field_type: FieldType) -> Self {
        Self {
            name: name.into(),
            field_type,
        }
    }
}

/// A choice's signature. How it behaves is not described: the emitted body
/// is a stub, because the engine that interprets the template supplies it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Choice {
    pub name: String,
    pub consuming: bool,
    /// Payload fields holding the parties that may exercise it.
    pub controllers: Vec<String>,
    /// The choice's own arguments, which become its argument record.
    pub arguments: Vec<Field>,
    pub result: ResultType,
}

/// A template: its payload, who sees it, and what may be exercised on it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Template {
    pub name: String,
    pub fields: Vec<Field>,
    /// Payload fields holding the signatories. Each must be a `Party` field.
    pub signatories: Vec<String>,
    /// Payload fields holding the observers. Each must be a `Party` field.
    pub observers: Vec<String>,
    pub choices: Vec<Choice>,
}

/// A module's worth of templates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Module {
    /// Dotted module name, e.g. `Fuji.Asset`.
    pub name: String,
    pub templates: Vec<Template>,
}

/// The package to build.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Package {
    pub name: String,
    pub version: String,
    pub modules: Vec<Module>,
}

impl Package {
    /// Check the description is self-consistent.
    ///
    /// Catches what would otherwise surface as a package a participant
    /// rejects: a contract disclosed to a field that does not exist, or to one
    /// that does not hold a party.
    pub(crate) fn validate(&self) -> DamlLfResult<()> {
        for module in &self.modules {
            for template in &module.templates {
                let choice_controllers = template
                    .choices
                    .iter()
                    .flat_map(|choice| choice.controllers.iter());
                for party_field in template
                    .signatories
                    .iter()
                    .chain(&template.observers)
                    .chain(choice_controllers)
                {
                    let field = template
                        .fields
                        .iter()
                        .find(|field| &field.name == party_field)
                        .ok_or_else(|| {
                            DamlLfError::new_package_build_error(format!(
                                "template {} has no field {party_field}",
                                template.name
                            ))
                        })?;
                    if field.field_type != FieldType::Party {
                        return Err(DamlLfError::new_package_build_error(format!(
                            "template {}: field {party_field} does not hold a party",
                            template.name
                        )));
                    }
                }
            }
        }
        Ok(())
    }
}
