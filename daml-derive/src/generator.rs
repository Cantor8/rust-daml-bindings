use crate::convert::{
    AttrChoice, AttrInterfaceRef, AttrRecord, AttrTemplate, AttrVariant, data_type_string_from_type,
    extract_all_choices, extract_enum, extract_record, extract_template, extract_variant,
};
use crate::{CodeGeneratorParameters, RenderModeArg};
use daml_codegen::generator::{ModuleMatcher, RenderMethod};
use daml_codegen::renderer::full::{
    quote_choice, quote_daml_enum, quote_daml_interface, quote_daml_record, quote_daml_template, quote_daml_variant,
};
use daml_codegen::renderer::quote_archive;
use daml_codegen::renderer::{RenderContext, RenderFilterMode};
use daml_lf::element::{
    DamlArchive, DamlChoice, DamlEnum, DamlInterface, DamlModule, DamlPackage, DamlRecord, DamlTemplate, DamlType,
    DamlVariant,
};
use daml_lf::{DarFile, LanguageVersion};
use darling::FromMeta;
use darling::ast::NestedMeta;
use std::borrow::Cow;
use std::collections::HashMap;
use syn::{Data, DataStruct, DeriveInput, Fields, ItemImpl};

/// Generate a Rust `TokenStream` representing the supplied Daml Archive.
pub fn generate_tokens(args: Vec<NestedMeta>) -> proc_macro::TokenStream {
    let params: CodeGeneratorParameters = CodeGeneratorParameters::from_list(&args).unwrap_or_else(|e| panic!("{}", e));
    let archive = DarFile::from_file(&params.dar_file)
        .unwrap_or_else(|e| panic!("failed to load Dar file from {}, error was: {}", &params.dar_file, e));
    let filters: Vec<_> = params.module_filter_regex.iter().map(String::as_str).collect();
    let render_method = match params.mode {
        Some(RenderModeArg::Intermediate) => RenderMethod::Intermediate,
        Some(RenderModeArg::Full) | None => RenderMethod::Full,
    };
    let applied =
        archive.apply(|archive| ModuleMatcher::new(&filters).map(|mm| quote_archive(archive, &mm, &render_method)));
    match applied {
        Ok(Ok(tokens)) => proc_macro::TokenStream::from(tokens),
        Ok(Err(e)) => panic!("failed to generate Daml code: {e}"),
        Err(e) => panic!("Daml-LF error in Daml code generator: {e}"),
    }
}

pub fn generate_template(
    input: DeriveInput,
    package_name: Option<String>,
    package_id: String,
    module_name: String,
    implements: String,
) -> proc_macro::TokenStream {
    let struct_name = input.ident.to_string();
    match &input.data {
        Data::Struct(DataStruct {
            fields: Fields::Named(fields_named),
            ..
        }) => {
            let implements = parse_implements_list(&implements);
            let template: AttrTemplate =
                extract_template(struct_name, package_id.clone(), module_name, fields_named, implements);
            let daml_template = DamlTemplate::from(&template);
            // Build a synthetic single-package archive so the
            // codegen's `ctx.package_name_for` resolves to the user-
            // provided name. When no name is given, the archive is
            // still empty and the codegen falls back to addressing
            // by package-id.
            let archive = synthesize_archive(&package_id, package_name.as_deref());
            let ctx = RenderContext::with_archive(&archive, RenderFilterMode::default());
            let expanded = quote_daml_template(&ctx, &daml_template);
            proc_macro::TokenStream::from(expanded)
        },
        _ => panic!("the DamlTemplate attribute may only be applied to a named struct type"),
    }
}

/// Parse the `implements = "..."` attribute value into structured
/// `AttrInterfaceRef`s. The expected syntax is a comma-separated
/// list of `<package-name>:<Module.Path>:<EntityName>` triples.
/// Whitespace between entries is tolerated.
///
/// Examples:
/// - `"fuji:Fuji.Asset:Holding"`
/// - `"fuji:Fuji.Asset:Holding, other-pkg:Foo.Bar:OtherIface"`
fn parse_implements_list(raw: &str) -> Vec<AttrInterfaceRef> {
    raw.split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|entry| {
            let parts: Vec<&str> = entry.split(':').collect();
            if parts.len() != 3 {
                panic!("#[DamlTemplate(implements = ...)]: expected `<pkg>:<Module.Path>:<Entity>`, got `{entry}`");
            }
            AttrInterfaceRef {
                package_name: parts[0].to_string(),
                module_path: parts[1].split('.').map(ToOwned::to_owned).collect(),
                entity_name: parts[2].to_string(),
            }
        })
        .collect()
}

/// Build the marker-trait tokens for a Daml interface declared via
/// `#[DamlInterface]`. The user-declared struct's body is ignored —
/// the macro replaces it with the generated trait. Methods and view
/// type are intentionally left empty: the interface trait at the
/// derive site is purely a marker for `interface_id()`-style
/// addressing.
pub fn generate_interface(
    input: DeriveInput,
    package_name: Option<String>,
    package_id: String,
    module_name: String,
) -> proc_macro::TokenStream {
    let interface_name = input.ident.to_string();
    let module_path: Vec<Cow<'_, str>> = module_name.split('.').map(|s| Cow::Owned(s.to_string())).collect();
    let daml_interface = DamlInterface::new(
        Cow::Owned(interface_name),
        Cow::Owned(package_id.clone()),
        module_path,
        Cow::Borrowed("this"),
        vec![],
        vec![],
        DamlType::Unit,
        vec![],
    );
    let archive = synthesize_archive(&package_id, package_name.as_deref());
    let ctx = RenderContext::with_archive(&archive, RenderFilterMode::default());
    let expanded = quote_daml_interface(&ctx, &daml_interface);
    proc_macro::TokenStream::from(expanded)
}

/// Build a minimal one-package `DamlArchive` carrying the
/// user-provided package-id / package-name pair. Used by the derive
/// path so the shared codegen helpers (which look the package up
/// via `RenderContext::package_name_for`) emit the right
/// `DamlIdentifier` shape.
fn synthesize_archive(package_id: &str, package_name: Option<&str>) -> DamlArchive<'static> {
    let mut packages = HashMap::new();
    let package = DamlPackage::new(
        Cow::Owned(package_name.unwrap_or_default().to_string()),
        Cow::Owned(package_id.to_string()),
        None,
        LanguageVersion::V2_1,
        DamlModule::new_root(),
    );
    packages.insert(Cow::Owned(package_id.to_string()), package);
    DamlArchive::new(Cow::Borrowed(""), Cow::Owned(package_id.to_string()), packages)
}

pub fn generate_choices(input: ItemImpl) -> proc_macro::TokenStream {
    let struct_name = data_type_string_from_type(input.self_ty.as_ref());
    let all_choices: Vec<AttrChoice> = extract_all_choices(&input.items);
    let all_daml_choices: Vec<DamlChoice<'_>> = all_choices.iter().map(DamlChoice::from).collect();
    let ctx = RenderContext::default();
    let all_choice_methods_tokens = quote_choice(&ctx, &struct_name, &all_daml_choices);
    proc_macro::TokenStream::from(all_choice_methods_tokens)
}

pub fn generate_data_struct(input: DeriveInput) -> proc_macro::TokenStream {
    let struct_name = input.ident.to_string();
    let tokens = match input.data {
        Data::Struct(data_struct) => match &data_struct.fields {
            Fields::Named(fields_named) => {
                let record: AttrRecord = extract_record(struct_name, fields_named, &input.generics);
                let daml_record = DamlRecord::from(&record);
                let ctx = RenderContext::default();
                quote_daml_record(&ctx, &daml_record)
            },
            Fields::Unnamed(_) => panic!("tuple struct not supported"),
            Fields::Unit => panic!("unit struct not supported"),
        },
        _ => panic!("the DamlData attribute may only be applied to struct types"),
    };
    proc_macro::TokenStream::from(tokens)
}

pub fn generate_data_variant(input: DeriveInput) -> proc_macro::TokenStream {
    let variant_name = input.ident.to_string();
    let tokens = match input.data {
        Data::Enum(data_enum) => {
            let variant: AttrVariant = extract_variant(variant_name, &data_enum, &input.generics);
            let daml_variant = DamlVariant::from(&variant);
            let ctx = RenderContext::default();
            quote_daml_variant(&ctx, &daml_variant)
        },
        _ => panic!("the DamlVariant attribute may only be applied to enum types"),
    };
    proc_macro::TokenStream::from(tokens)
}

pub fn generate_data_enum(input: DeriveInput) -> proc_macro::TokenStream {
    let enum_name = input.ident.to_string();
    let tokens = match input.data {
        Data::Enum(data_enum) => {
            let enum_variants = extract_enum(enum_name, &data_enum, &input.generics);
            let daml_enum = DamlEnum::from(&enum_variants);
            let ctx = RenderContext::default();
            quote_daml_enum(&ctx, &daml_enum)
        },
        _ => panic!("the DamlEnum attribute may only be applied to enum types"),
    };
    proc_macro::TokenStream::from(tokens)
}
