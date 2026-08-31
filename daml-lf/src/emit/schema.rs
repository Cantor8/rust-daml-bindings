//! What a package should contain.
//!
//! Deliberately small: it carries what a participant reads from a package —
//! record types, template signatures, and the parties a contract is disclosed
//! to. How choices behave is not described, because an engine that interprets
//! the templates elsewhere supplies that.

use crate::error::{DamlLfError, DamlLfResult};

/// A field's type, limited to what a template payload can hold today.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FieldType {
    Party,
    Text,
    Int64,
    Bool,
}

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

/// A template: its payload and who sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Template {
    pub name: String,
    pub fields: Vec<Field>,
    /// Payload fields holding the signatories. Each must be a `Party` field.
    pub signatories: Vec<String>,
    /// Payload fields holding the observers. Each must be a `Party` field.
    pub observers: Vec<String>,
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
                for party_field in template.signatories.iter().chain(&template.observers) {
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
