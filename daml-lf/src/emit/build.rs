//! Turning a [`schema::Package`] into a Daml-LF archive.
//!
//! Names in a package are interned: modules, data types and fields refer to
//! entries in the package's string and dotted-name tables rather than carrying
//! their text. The [`Interner`] builds those tables as the package is walked.

use std::collections::HashMap;

use prost::Message;
use sha2::{Digest, Sha256};

use crate::archive::{DamlLfArchive, DamlLfHashFunction};
use crate::error::DamlLfResult;
use crate::lf_protobuf::daml_lf::{archive_payload, Archive, ArchivePayload, HashFunction};
use crate::lf_protobuf::daml_lf_2::{
    builtin_lit, def_data_type, expr, r#type, self_or_imported_package_id, BuiltinFunction,
    BuiltinLit, BuiltinType, DefDataType, DefTemplate, Expr, FeatureFlags, FieldWithType,
    InternedDottedName,
    Module, ModuleId, Package, PackageMetadata, SelfOrImportedPackageId, TemplateChoice, Type,
    TypeConId, Unit, VarWithType,
};
use crate::payload::DamlLfArchivePayload;

use super::schema;

/// The LF version the emitted packages declare.
const LF_MAJOR_MINOR: &str = "1";

/// Collects the package's name tables, handing out the indices names are
/// referred to by.
#[derive(Default)]
struct Interner {
    strings: Vec<String>,
    string_indices: HashMap<String, i32>,
    dotted_names: Vec<InternedDottedName>,
    dotted_indices: HashMap<Vec<i32>, i32>,
}

impl Interner {
    fn string(&mut self, value: &str) -> i32 {
        if let Some(index) = self.string_indices.get(value) {
            return *index;
        }
        let index = self.strings.len() as i32;
        self.strings.push(value.to_owned());
        self.string_indices.insert(value.to_owned(), index);
        index
    }

    /// Intern a dotted name, e.g. a module name or a data type's name.
    fn dotted_name(&mut self, segments: &[&str]) -> i32 {
        let interned: Vec<i32> = segments.iter().map(|segment| self.string(segment)).collect();
        if let Some(index) = self.dotted_indices.get(&interned) {
            return *index;
        }
        let index = self.dotted_names.len() as i32;
        self.dotted_names.push(InternedDottedName {
            segments_interned_str: interned.clone(),
        });
        self.dotted_indices.insert(interned, index);
        index
    }
}

/// Build the archive describing `package`.
///
/// The archive's hash is its package id: the identity a participant knows the
/// templates by.
pub fn build_archive(package: &schema::Package) -> DamlLfResult<DamlLfArchive> {
    let (payload_bytes, hash) = build_payload(package)?;
    let payload = DamlLfArchivePayload::from_bytes(payload_bytes)?;
    Ok(DamlLfArchive::new(
        package.name.clone(),
        payload,
        DamlLfHashFunction::Sha256,
        hash,
    ))
}

/// The serialized payload of `package` and the hash naming it.
pub(crate) fn build_payload(package: &schema::Package) -> DamlLfResult<(Vec<u8>, String)> {
    package.validate()?;

    let mut interner = Interner::default();
    let modules = package
        .modules
        .iter()
        .map(|module| build_module(module, &mut interner))
        .collect::<Vec<_>>();

    let metadata = PackageMetadata {
        name_interned_str: interner.string(&package.name),
        version_interned_str: interner.string(&package.version),
        upgraded_package_id: None,
    };

    let lf_package = Package {
        modules,
        interned_strings: interner.strings,
        interned_dotted_names: interner.dotted_names,
        metadata: Some(metadata),
        interned_types: Vec::new(),
        interned_kinds: Vec::new(),
        interned_exprs: Vec::new(),
        imports_sum: None,
    };

    let payload_bytes = ArchivePayload {
        minor: LF_MAJOR_MINOR.to_owned(),
        patch: 0,
        sum: Some(archive_payload::Sum::DamlLf2(lf_package.encode_to_vec())),
    }
    .encode_to_vec();

    let hash = hex::encode(Sha256::digest(&payload_bytes));
    Ok((payload_bytes, hash))
}

/// Serialize the archive envelope a participant reads.
pub(crate) fn encode_archive(payload_bytes: Vec<u8>, hash: &str) -> Vec<u8> {
    Archive {
        hash_function: HashFunction::Sha256 as i32,
        payload: payload_bytes,
        hash: hash.to_owned(),
    }
    .encode_to_vec()
}

fn build_module(module: &schema::Module, interner: &mut Interner) -> Module {
    let segments: Vec<&str> = module.name.split('.').collect();
    let name_interned_dname = interner.dotted_name(&segments);

    // Each template contributes its payload record, and one record per choice
    // for that choice's arguments.
    let mut data_types = Vec::new();
    for template in &module.templates {
        data_types.push(build_data_type(template, interner));
        for choice in &template.choices {
            data_types.push(build_choice_data_type(choice, interner));
        }
    }
    let templates = module
        .templates
        .iter()
        .map(|template| build_template(template, name_interned_dname, interner))
        .collect();

    Module {
        name_interned_dname,
        // Settled invariants in LF2, but the field is still required.
        flags: Some(FeatureFlags {
            forbid_party_literals: true,
            dont_divulge_contract_ids_in_create_arguments: true,
            dont_disclose_non_consuming_choices_to_observers: true,
        }),
        synonyms: Vec::new(),
        data_types,
        values: Vec::new(),
        templates,
        exceptions: Vec::new(),
        interfaces: Vec::new(),
    }
}

/// The record behind a template's payload.
fn build_data_type(template: &schema::Template, interner: &mut Interner) -> DefDataType {
    let fields = template
        .fields
        .iter()
        .map(|field| FieldWithType {
            field_interned_str: interner.string(&field.name),
            r#type: Some(field_type(&field.field_type)),
        })
        .collect();

    DefDataType {
        location: None,
        name_interned_dname: interner.dotted_name(&[&template.name]),
        params: Vec::new(),
        serializable: true,
        data_cons: Some(def_data_type::DataCons::Record(def_data_type::Fields {
            fields,
        })),
    }
}

fn build_template(
    template: &schema::Template,
    module_dname: i32,
    interner: &mut Interner,
) -> DefTemplate {
    // The payload is bound to a name the party expressions project out of.
    let param = "this";
    let param_interned_str = interner.string(param);

    DefTemplate {
        tycon_interned_dname: interner.dotted_name(&[&template.name]),
        param_interned_str,
        precond: None,
        signatories: Some(party_list(
            &template.signatories,
            param,
            template,
            module_dname,
            interner,
        )),
        observers: Some(party_list(
            &template.observers,
            param,
            template,
            module_dname,
            interner,
        )),
        location: None,
        choices: template
            .choices
            .iter()
            .map(|choice| build_choice(choice, template, param, module_dname, interner))
            .collect(),
        implements: Vec::new(),
        key: None,
    }
}

/// `[this.a, this.b, ...]` as an LF expression of type `List Party`.
fn party_list(
    field_names: &[String],
    param: &str,
    template: &schema::Template,
    module_dname: i32,
    interner: &mut Interner,
) -> Expr {
    let party = Type {
        sum: Some(r#type::Sum::Builtin(r#type::Builtin {
            builtin: BuiltinType::Party as i32,
            args: Vec::new(),
        })),
    };
    let nil = Expr {
        location: None,
        sum: Some(expr::Sum::Nil(expr::Nil {
            r#type: Some(party.clone()),
        })),
    };
    if field_names.is_empty() {
        return nil;
    }

    let this = Expr {
        location: None,
        sum: Some(expr::Sum::VarInternedStr(interner.string(param))),
    };
    let record = template_tycon(module_dname, &template.name, interner);
    let front = field_names
        .iter()
        .map(|name| Expr {
            location: None,
            sum: Some(expr::Sum::RecProj(Box::new(expr::RecProj {
                tycon: Some(record.clone()),
                field_interned_str: interner.string(name),
                record: Some(Box::new(this.clone())),
            }))),
        })
        .collect();

    Expr {
        location: None,
        sum: Some(expr::Sum::Cons(Box::new(expr::Cons {
            r#type: Some(party),
            front,
            tail: Some(Box::new(nil)),
        }))),
    }
}


/// The record behind a choice's arguments.
fn build_choice_data_type(choice: &schema::Choice, interner: &mut Interner) -> DefDataType {
    let fields = choice
        .arguments
        .iter()
        .map(|field| FieldWithType {
            field_interned_str: interner.string(&field.name),
            r#type: Some(field_type(&field.field_type)),
        })
        .collect();

    DefDataType {
        location: None,
        name_interned_dname: interner.dotted_name(&[&choice.name]),
        params: Vec::new(),
        serializable: true,
        data_cons: Some(def_data_type::DataCons::Record(def_data_type::Fields {
            fields,
        })),
    }
}

fn build_choice(
    choice: &schema::Choice,
    template: &schema::Template,
    param: &str,
    module_dname: i32,
    interner: &mut Interner,
) -> TemplateChoice {
    let ret_type = result_type(&choice.result);
    TemplateChoice {
        location: None,
        name_interned_str: interner.string(&choice.name),
        consuming: choice.consuming,
        controllers: Some(party_list(
            &choice.controllers,
            param,
            template,
            module_dname,
            interner,
        )),
        observers: Some(empty_party_list()),
        arg_binder: Some(VarWithType {
            var_interned_str: interner.string("arg"),
            r#type: Some(Type {
                sum: Some(r#type::Sum::Con(template_tycon(
                    module_dname,
                    &choice.name,
                    interner,
                ))),
            }),
        }),
        ret_type: Some(ret_type.clone()),
        update: Some(stub_body(&ret_type, &choice.name, interner)),
        self_binder_interned_str: interner.string("self"),
        authorizers: None,
    }
}

/// The body emitted for every choice.
///
/// A package built here describes types, not behaviour: the engine that
/// interprets the template runs the choice. Should something reach this body,
/// failing loudly beats behaving as though the choice did nothing.
fn stub_body(ret_type: &Type, choice_name: &str, interner: &mut Interner) -> Expr {
    let update_ret = Type {
        sum: Some(r#type::Sum::Builtin(r#type::Builtin {
            builtin: BuiltinType::Update as i32,
            args: vec![ret_type.clone()],
        })),
    };
    let error = Expr {
        location: None,
        sum: Some(expr::Sum::Builtin(BuiltinFunction::Error as i32)),
    };
    let error_at_type = Expr {
        location: None,
        sum: Some(expr::Sum::TyApp(Box::new(expr::TyApp {
            expr: Some(Box::new(error)),
            types: vec![update_ret],
        }))),
    };
    let message = Expr {
        location: None,
        sum: Some(expr::Sum::BuiltinLit(BuiltinLit {
            sum: Some(builtin_lit::Sum::TextInternedStr(interner.string(&format!(
                "choice {choice_name} is interpreted by an external engine"
            )))),
        })),
    };
    Expr {
        location: None,
        sum: Some(expr::Sum::App(Box::new(expr::App {
            fun: Some(Box::new(error_at_type)),
            args: vec![message],
        }))),
    }
}

fn empty_party_list() -> Expr {
    Expr {
        location: None,
        sum: Some(expr::Sum::Nil(expr::Nil {
            r#type: Some(party_type()),
        })),
    }
}

fn party_type() -> Type {
    Type {
        sum: Some(r#type::Sum::Builtin(r#type::Builtin {
            builtin: BuiltinType::Party as i32,
            args: Vec::new(),
        })),
    }
}

fn result_type(result: &schema::ResultType) -> Type {
    let builtin = match result {
        schema::ResultType::Unit => BuiltinType::Unit,
        schema::ResultType::Party => BuiltinType::Party,
        schema::ResultType::Text => BuiltinType::Text,
        schema::ResultType::Int64 => BuiltinType::Int64,
        schema::ResultType::Bool => BuiltinType::Bool,
    };
    Type {
        sum: Some(r#type::Sum::Builtin(r#type::Builtin {
            builtin: builtin as i32,
            args: Vec::new(),
        })),
    }
}
/// The type constructor naming a template's record, within this package.
fn template_tycon(
    module_dname: i32,
    template_name: &str,
    interner: &mut Interner,
) -> r#type::Con {
    r#type::Con {
        tycon: Some(TypeConId {
            module: Some(ModuleId {
                package_id: Some(SelfOrImportedPackageId {
                    sum: Some(self_or_imported_package_id::Sum::SelfPackageId(Unit {})),
                }),
                module_name_interned_dname: module_dname,
            }),
            name_interned_dname: interner.dotted_name(&[template_name]),
        }),
        args: Vec::new(),
    }
}

fn field_type(field_type: &schema::FieldType) -> Type {
    let builtin = match field_type {
        schema::FieldType::Party => BuiltinType::Party,
        schema::FieldType::Text => BuiltinType::Text,
        schema::FieldType::Int64 => BuiltinType::Int64,
        schema::FieldType::Bool => BuiltinType::Bool,
    };
    Type {
        sum: Some(r#type::Sum::Builtin(r#type::Builtin {
            builtin: builtin as i32,
            args: Vec::new(),
        })),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::emit::schema::{
        Choice, Field, FieldType, Module as SchemaModule, ResultType, Template,
    };

    fn iou() -> schema::Package {
        schema::Package {
            name: "RoadrunnerExample".to_owned(),
            version: "1.0.0".to_owned(),
            modules: vec![SchemaModule {
                name: "Example.Iou".to_owned(),
                templates: vec![Template {
                    name: "Iou".to_owned(),
                    fields: vec![
                        Field::new("issuer", FieldType::Party),
                        Field::new("owner", FieldType::Party),
                        Field::new("amount", FieldType::Int64),
                    ],
                    signatories: vec!["issuer".to_owned()],
                    observers: vec!["owner".to_owned()],
                    choices: vec![Choice {
                        name: "Transfer".to_owned(),
                        consuming: true,
                        controllers: vec!["owner".to_owned()],
                        arguments: vec![Field::new("newOwner", FieldType::Party)],
                        result: ResultType::Unit,
                    }],
                }],
            }],
        }
    }


    #[test]
    fn a_template_and_its_choice_read_back() {
        // Decoding is what the rest of this crate does, so it is the check
        // that matters: what was built has to come back as a template.
        let archive = build_archive(&iou()).expect("builds");
        let (name, choices, signatories) = archive
            .payload
            .apply(|package| {
                let module = package
                    .root_module()
                    .child_modules()
                    .flat_map(|module| module.child_modules())
                    .next()
                    .expect("the Example.Iou module");
                let template = module
                    .data_types()
                    .find_map(|data| match data {
                        crate::element::DamlData::Template(template) => Some(template.clone()),
                        _ => None,
                    })
                    .expect("the Iou template");
                (
                    template.name().to_owned(),
                    template
                        .choices()
                        .iter()
                        .map(|choice| choice.name().to_owned())
                        .collect::<Vec<_>>(),
                    template.fields().len(),
                )
            })
            .expect("decodes");

        assert_eq!(name, "Iou");
        assert_eq!(choices, vec!["Transfer".to_owned()]);
        assert_eq!(signatories, 3);
    }
    #[test]
    fn a_built_package_reads_back() {
        let archive = build_archive(&iou()).expect("builds");
        assert!(archive.payload.contains_module("Example.Iou"));
        assert_eq!(archive.payload.list_modules(), vec!["Example.Iou".to_owned()]);
    }

    #[test]
    fn the_hash_is_the_package_id() {
        let archive = build_archive(&iou()).expect("builds");
        // A package id is the hex sha256 of the payload it names.
        assert_eq!(archive.hash.len(), 64);
        assert!(archive.hash.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn a_package_is_identified_by_what_it_contains() {
        let one = build_archive(&iou()).expect("builds");
        let mut renamed = iou();
        renamed.modules[0].templates[0].fields[2].name = "quantity".to_owned();
        let other = build_archive(&renamed).expect("builds");
        assert_ne!(one.hash, other.hash);
    }

    #[test]
    fn a_contract_must_be_disclosed_to_a_party() {
        let mut wrong = iou();
        wrong.modules[0].templates[0].signatories = vec!["amount".to_owned()];
        let error = build_archive(&wrong).expect_err("amount is not a party");
        assert!(error.to_string().contains("does not hold a party"));

        let mut missing = iou();
        missing.modules[0].templates[0].observers = vec!["nobody".to_owned()];
        let error = build_archive(&missing).expect_err("no such field");
        assert!(error.to_string().contains("has no field nobody"));
    }
}
