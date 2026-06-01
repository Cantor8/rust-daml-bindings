use crate::convert::{
    data_type_string_from_type, extract_all_choices, extract_enum, extract_record, extract_template, extract_variant,
    AttrChoice, AttrRecord, AttrTemplate, AttrVariant,
};
use crate::CodeGeneratorParameters;
use daml_codegen::generator::{ModuleMatcher, RenderMethod};
use daml_codegen::renderer::full::{
    quote_choice, quote_daml_enum, quote_daml_record, quote_daml_template, quote_daml_variant,
};
use daml_codegen::renderer::quote_archive;
use daml_codegen::renderer::{RenderContext, RenderFilterMode};
use daml_lf::element::{
    DamlArchive, DamlChoice, DamlEnum, DamlModule, DamlPackage, DamlRecord, DamlTemplate, DamlVariant,
};
use daml_lf::{DarFile, LanguageVersion};
use darling::FromMeta;
use quote::quote;
use std::borrow::Cow;
use std::collections::HashMap;
use syn::{AttributeArgs, Data, DataStruct, DeriveInput, Fields, ItemImpl};

/// Generate a Rust `TokenStream` representing the supplied Daml Archive.
pub fn generate_tokens(args: AttributeArgs) -> proc_macro::TokenStream {
    let params: CodeGeneratorParameters = CodeGeneratorParameters::from_list(&args).unwrap_or_else(|e| panic!("{}", e));
    let archive = DarFile::from_file(&params.dar_file)
        .unwrap_or_else(|e| panic!("failed to load Dar file from {}, error was: {}", &params.dar_file, e));
    let filters: Vec<_> = params.module_filter_regex.iter().map(String::as_str).collect();
    let render_method = match &params.mode {
        Some(name) if name.to_ascii_lowercase() == "intermediate" => RenderMethod::Intermediate,
        Some(name) if name.to_ascii_lowercase() == "full" => RenderMethod::Full,
        Some(name) => panic!("unknown mode: {}, expected Intermediate or Full", name),
        _ => RenderMethod::Full,
    };
    let applied =
        archive.apply(|archive| ModuleMatcher::new(&filters).map(|mm| quote_archive(archive, &mm, &render_method)));
    match applied {
        Ok(Ok(tokens)) => proc_macro::TokenStream::from(tokens),
        Ok(Err(e)) => panic!("failed to generate Daml code: {0}", e),
        Err(e) => panic!("Daml-LF error in Daml code generator: {0}", e),
    }
}

pub fn generate_template(
    input: DeriveInput,
    package_name: Option<String>,
    package_id: String,
    module_name: String,
) -> proc_macro::TokenStream {
    let struct_name = input.ident.to_string();
    match &input.data {
        Data::Struct(DataStruct {
            fields: Fields::Named(fields_named),
            ..
        }) => {
            let template: AttrTemplate = extract_template(struct_name, package_id.clone(), module_name, fields_named);
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
    let expanded = quote!(
        #tokens
    );
    proc_macro::TokenStream::from(expanded)
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
    let expanded = quote!(
        #tokens
    );
    proc_macro::TokenStream::from(expanded)
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
    let expanded = quote!(
        #tokens
    );
    proc_macro::TokenStream::from(expanded)
}
