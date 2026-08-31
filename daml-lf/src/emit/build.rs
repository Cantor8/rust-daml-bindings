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
    def_data_type, expr, r#type, self_or_imported_package_id, BuiltinType,
    DefDataType, DefTemplate, Expr, FieldWithType, InternedDottedName, Module, ModuleId, Package,
    PackageMetadata, SelfOrImportedPackageId, Type, TypeConId, Unit,
};
use crate::payload::DamlLfArchivePayload;

use super::schema;

/// The LF version the emitted packages declare.
const LF_MAJOR_MINOR: &str = "1";

/// The name given to the emitted archive.
const ARCHIVE_NAME: &str = "roadrunner";

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
    let payload = DamlLfArchivePayload::from_bytes(payload_bytes)?;
    Ok(DamlLfArchive::new(
        ARCHIVE_NAME,
        payload,
        DamlLfHashFunction::Sha256,
        hash,
    ))
}

/// Serialize an archive to the bytes a participant reads.
pub fn encode_archive(archive: &DamlLfArchive, payload_bytes: Vec<u8>) -> Vec<u8> {
    Archive {
        hash_function: HashFunction::Sha256 as i32,
        payload: payload_bytes,
        hash: archive.hash.clone(),
    }
    .encode_to_vec()
}

fn build_module(module: &schema::Module, interner: &mut Interner) -> Module {
    let segments: Vec<&str> = module.name.split('.').collect();
    let name_interned_dname = interner.dotted_name(&segments);

    let data_types = module
        .templates
        .iter()
        .map(|template| build_data_type(template, interner))
        .collect();
    let templates = module
        .templates
        .iter()
        .map(|template| build_template(template, name_interned_dname, interner))
        .collect();

    Module {
        name_interned_dname,
        flags: None,
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
        choices: Vec::new(),
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
    use crate::emit::schema::{Field, FieldType, Module as SchemaModule, Template};

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
                }],
            }],
        }
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
