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
    builtin_lit, def_data_type, def_template, expr, r#type, self_or_imported_package_id,
    BuiltinFunction,
    BuiltinLit, BuiltinType, DefDataType, DefTemplate, Expr, FeatureFlags, FieldWithType,
    InternedDottedName,
    Module, ModuleId, Package, PackageMetadata, SelfOrImportedPackageId, TemplateChoice, Type,
    TypeConId, Unit, VarWithType,
};
use crate::payload::DamlLfArchivePayload;

use super::schema;

/// The Daml-LF 2 minor version the emitted packages declare.
///
/// Contract keys need this much, and from 2.2 a type is applied rather than
/// carrying its arguments inline, so one version keeps one encoding.
const LF_MINOR: &str = "3";

/// Collects the package's name tables, handing out the indices names are
/// referred to by.
#[derive(Default)]
struct Interner {
    strings: Vec<String>,
    string_indices: HashMap<String, i32>,
    dotted_names: Vec<InternedDottedName>,
    dotted_indices: HashMap<Vec<i32>, i32>,
    types: Vec<Type>,
    type_indices: HashMap<Vec<u8>, i32>,
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

    /// Intern a type and return a reference to it.
    ///
    /// Types may only appear in a package through this table, so every type a
    /// definition mentions goes through here.
    fn r#type(&mut self, value: Type) -> Type {
        let key = value.encode_to_vec();
        let index = match self.type_indices.get(&key) {
            Some(index) => *index,
            None => {
                let index = self.types.len() as i32;
                self.types.push(value);
                self.type_indices.insert(key, index);
                index
            },
        };
        Type {
            sum: Some(r#type::Sum::InternedType(index)),
        }
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
        interned_types: interner.types,
        interned_kinds: Vec::new(),
        interned_exprs: Vec::new(),
        imports_sum: None,
    };

    let payload_bytes = ArchivePayload {
        minor: LF_MINOR.to_owned(),
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
    for data in &module.data_types {
        data_types.push(build_user_data_type(data, interner));
    }
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
            r#type: Some(field_type(&field.field_type, interner)),
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
        key: template
            .key
            .as_ref()
            .map(|key| build_key(key, interner)),
    }
}

/// A template.s key: its type, and stubs for how it and its maintainers are
/// worked out.
///
/// The expressions are stubs for the same reason a choice.s body is: whoever
/// interprets the template computes them, and a participant reads the type.
fn build_key(key: &schema::TemplateKey, interner: &mut Interner) -> def_template::DefKey {
    let key_type = field_type(&key.key_type, interner);
    let party = party_type(interner);
    let list = builtin(BuiltinType::List, interner);
    let party_list_type = applied(list, vec![party], interner);
    let arrow = builtin(BuiltinType::Arrow, interner);
    let maintainers_type = applied(arrow, vec![key_type.clone(), party_list_type], interner);
    def_template::DefKey {
        r#type: Some(key_type.clone()),
        key_expr: Some(stub_expr(&key_type, "the engine computes this key", interner)),
        maintainers: Some(stub_expr(
            &maintainers_type,
            "the engine computes this key.s maintainers",
            interner,
        )),
    }
}

/// `error  "message"` — an expression of the right type that stands in for
/// one only an interpreter needs.
fn stub_expr(ty: &Type, message: &str, interner: &mut Interner) -> Expr {
    let error = Expr {
        location: None,
        sum: Some(expr::Sum::Builtin(BuiltinFunction::Error as i32)),
    };
    let error_at_type = Expr {
        location: None,
        sum: Some(expr::Sum::TyApp(Box::new(expr::TyApp {
            expr: Some(Box::new(error)),
            types: vec![ty.clone()],
        }))),
    };
    Expr {
        location: None,
        sum: Some(expr::Sum::App(Box::new(expr::App {
            fun: Some(Box::new(error_at_type)),
            args: vec![Expr {
                location: None,
                sum: Some(expr::Sum::BuiltinLit(BuiltinLit {
                    sum: Some(builtin_lit::Sum::TextInternedStr(interner.string(message))),
                })),
            }],
        }))),
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
    let party = party_type(interner);
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
            r#type: Some(field_type(&field.field_type, interner)),
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
    let ret_type = field_type(&choice.result, interner);
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
        observers: Some(empty_party_list(interner)),
        arg_binder: Some(VarWithType {
            var_interned_str: interner.string("arg"),
            r#type: Some({
                let con = template_tycon(module_dname, &choice.name, interner);
                interner.r#type(Type {
                    sum: Some(r#type::Sum::Con(con)),
                })
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
    let update = builtin(BuiltinType::Update, interner);
    let update_ret = applied(update, vec![ret_type.clone()], interner);
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

fn empty_party_list(interner: &mut Interner) -> Expr {
    Expr {
        location: None,
        sum: Some(expr::Sum::Nil(expr::Nil {
            r#type: Some(party_type(interner)),
        })),
    }
}

fn party_type(interner: &mut Interner) -> Type {
    builtin(BuiltinType::Party, interner)
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

fn field_type(ty: &schema::FieldType, interner: &mut Interner) -> Type {
    let (builtin, args) = match ty {
        schema::FieldType::Unit => (BuiltinType::Unit, Vec::new()),
        schema::FieldType::Bool => (BuiltinType::Bool, Vec::new()),
        schema::FieldType::Int64 => (BuiltinType::Int64, Vec::new()),
        schema::FieldType::Text => (BuiltinType::Text, Vec::new()),
        schema::FieldType::Timestamp => (BuiltinType::Timestamp, Vec::new()),
        schema::FieldType::Date => (BuiltinType::Date, Vec::new()),
        schema::FieldType::Party => (BuiltinType::Party, Vec::new()),
        // A numeric carries its scale as a type argument.
        schema::FieldType::Numeric(scale) => (
            BuiltinType::Numeric,
            vec![interner.r#type(Type {
                sum: Some(r#type::Sum::Nat(i64::from(*scale))),
            })],
        ),
        // A contract id is parameterised by the template it points at.
        schema::FieldType::ContractId(target) => (
            BuiltinType::ContractId,
            vec![type_con(target, interner)],
        ),
        // A user-defined type is named, not built: its definition stands
        // beside whatever holds it.
        schema::FieldType::Data(target) => return type_con(target, interner),
        schema::FieldType::List(inner) => {
            (BuiltinType::List, vec![field_type(inner, interner)])
        },
        schema::FieldType::Optional(inner) => {
            (BuiltinType::Optional, vec![field_type(inner, interner)])
        },
        schema::FieldType::TextMap(inner) => {
            (BuiltinType::Textmap, vec![field_type(inner, interner)])
        },
        schema::FieldType::GenMap(key, value) => (
            BuiltinType::Genmap,
            vec![field_type(key, interner), field_type(value, interner)],
        ),
    };
    let base = self::builtin(builtin, interner);
    applied(base, args, interner)
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
                data_types: Vec::new(),
                name: "Example.Iou".to_owned(),
                templates: vec![Template {
                    key: None,
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

/// A user-defined type's own definition, which a field naming it needs.
fn build_user_data_type(data: &schema::DataType, interner: &mut Interner) -> DefDataType {
    // A name may be dotted — the record behind a variant's named fields is
    // named after the variant holding it.
    let segments: Vec<&str> = data.name.split('.').collect();
    let data_cons = match &data.body {
        schema::DataBody::Record(fields) => {
            def_data_type::DataCons::Record(fields_with_types(fields, interner))
        },
        schema::DataBody::Variant(ctors) => {
            let fields = ctors
                .iter()
                .map(|ctor| FieldWithType {
                    field_interned_str: interner.string(&ctor.name),
                    r#type: Some(field_type(&ctor.payload, interner)),
                })
                .collect();
            def_data_type::DataCons::Variant(def_data_type::Fields { fields })
        },
        schema::DataBody::Enum(names) => {
            def_data_type::DataCons::Enum(def_data_type::EnumConstructors {
                constructors_interned_str: names
                    .iter()
                    .map(|name| interner.string(name))
                    .collect(),
            })
        },
    };

    DefDataType {
        location: None,
        name_interned_dname: interner.dotted_name(&segments),
        params: Vec::new(),
        serializable: true,
        data_cons: Some(data_cons),
    }
}

fn fields_with_types(
    fields: &[schema::Field],
    interner: &mut Interner,
) -> def_data_type::Fields {
    def_data_type::Fields {
        fields: fields
            .iter()
            .map(|field| FieldWithType {
                field_interned_str: interner.string(&field.name),
                r#type: Some(field_type(&field.field_type, interner)),
            })
            .collect(),
    }
}

/// A builtin type on its own.
fn builtin(builtin: BuiltinType, interner: &mut Interner) -> Type {
    interner.r#type(Type {
        sum: Some(r#type::Sum::Builtin(r#type::Builtin {
            builtin: builtin as i32,
            args: Vec::new(),
        })),
    })
}

/// A type applied to arguments, one at a time.
///
/// The arguments used to hang off the type itself; since 2.2 they are applied
/// as they are written, so `List Party` is `List` applied to `Party`.
fn applied(base: Type, args: Vec<Type>, interner: &mut Interner) -> Type {
    args.into_iter().fold(base, |lhs, rhs| {
        interner.r#type(Type {
            sum: Some(r#type::Sum::Tapp(Box::new(r#type::TApp {
                lhs: Some(Box::new(lhs)),
                rhs: Some(Box::new(rhs)),
            }))),
        })
    })
}

/// The interned type naming a record, variant, enum or template in this
/// package.
fn type_con(target: &schema::TypeRef, interner: &mut Interner) -> Type {
    let module_segments: Vec<&str> = target.module.split('.').collect();
    let module_dname = interner.dotted_name(&module_segments);
    // A name may be dotted: the record behind a variant is named after the
    // variant holding it.
    let name_segments: Vec<&str> = target.name.split('.').collect();
    let con = r#type::Con {
        tycon: Some(TypeConId {
            module: Some(ModuleId {
                package_id: Some(SelfOrImportedPackageId {
                    sum: Some(self_or_imported_package_id::Sum::SelfPackageId(Unit {})),
                }),
                module_name_interned_dname: module_dname,
            }),
            name_interned_dname: interner.dotted_name(&name_segments),
        }),
        args: Vec::new(),
    };
    interner.r#type(Type {
        sum: Some(r#type::Sum::Con(con)),
    })
}
