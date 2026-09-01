//! What a package should contain.
//!
//! Deliberately small: it carries what a participant reads from a package —
//! record types, template signatures, and the parties a contract is disclosed
//! to. How choices behave is not described, because an engine that interprets
//! the templates elsewhere supplies that.

use crate::error::{DamlLfError, DamlLfResult};

/// A type named by another, which lives in the package being built.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeRef {
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
    /// A number, and the scale it is held at.
    Numeric(u8),
    Text,
    Timestamp,
    Date,
    Party,
    ContractId(TypeRef),
    /// A contract id pointing at an interface rather than a template.
    InterfaceContractId(TypeRef),
    /// A record, variant or enum defined in the package.
    Data(TypeRef),
    List(Box<FieldType>),
    Optional(Box<FieldType>),
    /// A map from text to something.
    TextMap(Box<FieldType>),
    /// A map from anything to anything.
    GenMap(Box<FieldType>, Box<FieldType>),
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

/// A template.s contract key: what type it is.
///
/// How the key and its maintainers are computed is not described, for the
/// same reason a choice.s behaviour is not: the engine that interprets the
/// template works them out. The type is what a participant needs, to read a
/// key value it is given.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TemplateKey {
    pub key_type: FieldType,
}

/// An interface a contract id may point at, and the view it presents.
///
/// What the view is computed from is not here: that belongs to whichever
/// template implements it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Interface {
    pub name: String,
    pub view: FieldType,
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
    /// The contract key, when the template has one.
    pub key: Option<TemplateKey>,
}

/// A module's worth of templates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Module {
    /// Dotted module name, e.g. `Fuji.Asset`.
    pub name: String,
    pub templates: Vec<Template>,
    /// Types the templates name, defined here.
    pub data_types: Vec<DataType>,
    /// Interfaces the templates point at, defined here.
    pub interfaces: Vec<Interface>,
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
        self.check_named_types_exist()?;
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

/// One constructor of a variant, and what it carries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ctor {
    pub name: String,
    pub payload: FieldType,
}

/// What a user-defined type is made of.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DataBody {
    Record(Vec<Field>),
    Variant(Vec<Ctor>),
    /// Constructor names, none of which carry anything.
    Enum(Vec<String>),
}

/// A type a template's field may hold, defined so the field can name it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataType {
    pub name: String,
    pub body: DataBody,
}

impl Package {
    /// Check every named type is defined here.
    ///
    /// A field naming a type the package does not carry is a package a
    /// participant rejects, so it is caught while the cause is still in hand.
    fn check_named_types_exist(&self) -> DamlLfResult<()> {
        let mut defined: Vec<(&str, &str)> = Vec::new();
        for module in &self.modules {
            for data in &module.data_types {
                defined.push((&module.name, &data.name));
            }
            for interface in &module.interfaces {
                defined.push((&module.name, &interface.name));
            }
            for template in &module.templates {
                defined.push((&module.name, &template.name));
            }
        }

        for module in &self.modules {
            for template in &module.templates {
                let choice_types = template
                    .choices
                    .iter()
                    .flat_map(|choice| {
                        choice
                            .arguments
                            .iter()
                            .map(|field| &field.field_type)
                            .chain(std::iter::once(&choice.result))
                    });
                let field_types = template.fields.iter().map(|field| &field.field_type);
                for ty in field_types.chain(choice_types) {
                    check_type(ty, &defined)?;
                }
            }
            for data in &module.data_types {
                match &data.body {
                    DataBody::Record(fields) => {
                        for field in fields {
                            check_type(&field.field_type, &defined)?;
                        }
                    },
                    DataBody::Variant(ctors) => {
                        for ctor in ctors {
                            check_type(&ctor.payload, &defined)?;
                        }
                    },
                    DataBody::Enum(_) => (),
                }
            }
        }
        Ok(())
    }
}

/// The widest scale a Daml number may be held at.
const MAX_NUMERIC_SCALE: u8 = 37;

fn check_type(ty: &FieldType, defined: &[(&str, &str)]) -> DamlLfResult<()> {
    match ty {
        FieldType::Numeric(scale) if *scale > MAX_NUMERIC_SCALE => {
            Err(DamlLfError::new_package_build_error(format!(
                "a number is held at scale {scale}, and {MAX_NUMERIC_SCALE} is the widest"
            )))
        },
        FieldType::List(inner) | FieldType::Optional(inner) | FieldType::TextMap(inner) =>
            check_type(inner, defined),
        FieldType::GenMap(key, value) => {
            check_type(key, defined).and_then(|()| check_type(value, defined))
        },
        FieldType::ContractId(target)
        | FieldType::InterfaceContractId(target)
        | FieldType::Data(target) => {
            if defined
                .iter()
                .any(|(module, name)| *module == target.module && *name == target.name)
            {
                Ok(())
            } else {
                Err(DamlLfError::new_package_build_error(format!(
                    "no type {}:{} in this package",
                    target.module, target.name
                )))
            }
        },
        _ => Ok(()),
    }
}
