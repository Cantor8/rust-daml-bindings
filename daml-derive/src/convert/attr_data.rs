use crate::convert::{AttrField, AttrType, extract_enum_data, extract_struct_data};
use syn::{DataEnum, FieldsNamed, GenericParam, Generics};

pub struct AttrRecord {
    pub name: String,
    pub fields: Vec<AttrField>,
    pub type_arguments: Vec<String>,
}

pub struct AttrTemplate {
    pub name: String,
    pub package_id: String,
    pub module_path: Vec<String>,
    pub fields: Vec<AttrField>,
    /// Interface refs, formatted as `pkg:Module.Path:EntityName`
    /// (see `parse_implements_ref` in `attribute_converter.rs`).
    /// Codegen turns each into an `impl <Iface> for FooContractId {}`
    /// block plus the matching exercise-via-interface helpers.
    pub implements: Vec<AttrInterfaceRef>,
}

/// Parsed reference to an interface: package-name, module path,
/// and entity name.
#[derive(Debug, Clone)]
pub struct AttrInterfaceRef {
    pub package_name: String,
    pub module_path: Vec<String>,
    pub entity_name: String,
}

pub struct AttrVariant {
    pub name: String,
    pub fields: Vec<AttrField>,
    pub type_arguments: Vec<String>,
}

pub struct AttrEnum {
    pub name: String,
    pub fields: Vec<String>,
    pub type_arguments: Vec<String>,
}

pub fn extract_record(name: String, fields_named: &FieldsNamed, generics: &Generics) -> AttrRecord {
    AttrRecord {
        name,
        fields: extract_struct_data(fields_named),
        type_arguments: extract_generic_type_arguments(generics),
    }
}

pub fn extract_template(
    name: String,
    package_id: String,
    module_path: String,
    fields_named: &FieldsNamed,
    implements: Vec<AttrInterfaceRef>,
) -> AttrTemplate {
    AttrTemplate {
        name,
        package_id,
        module_path: module_path.split('.').map(ToOwned::to_owned).collect(),
        fields: extract_struct_data(fields_named),
        implements,
    }
}

pub fn extract_variant(name: String, data_enum: &DataEnum, generics: &Generics) -> AttrVariant {
    AttrVariant {
        name,
        fields: extract_enum_data(data_enum),
        type_arguments: extract_generic_type_arguments(generics),
    }
}

pub fn extract_enum(name: String, data_enum: &DataEnum, generics: &Generics) -> AttrEnum {
    let fields: Vec<String> = extract_enum_data(data_enum)
        .into_iter()
        .map(|v| {
            if AttrType::Unit == v.field_type {
                v.field_label
            } else {
                panic!("DamlEnum variants may not have type parameters (use DamlVariant instead)")
            }
        })
        .collect();
    AttrEnum {
        name,
        fields,
        type_arguments: extract_generic_type_arguments(generics),
    }
}

fn extract_generic_type_arguments(generics: &Generics) -> Vec<String> {
    generics
        .params
        .iter()
        .filter_map(|param| {
            if let GenericParam::Type(ty) = param {
                Some(ty.ident.to_string())
            } else {
                None
            }
        })
        .collect()
}
