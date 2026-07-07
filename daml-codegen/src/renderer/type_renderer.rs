use crate::renderer::renderer_utils::{quote_escaped_ident, quote_ident};
use crate::renderer::RenderContext;
use daml_lf::element::{DamlAbsoluteTyCon, DamlNonLocalTyCon, DamlTyCon, DamlTyConName, DamlType};
use heck::ToSnakeCase;
use proc_macro2::TokenStream;
use quote::quote;
use std::iter;

pub fn quote_type(ctx: &RenderContext<'_>, daml_type: &DamlType<'_>) -> TokenStream {
    match daml_type {
        DamlType::List(args) =>
            if let Some(arg) = args.first() {
                let prim_name_tokens = quote_escaped_ident(daml_type.name());
                let prim_param_tokens = quote_type(ctx, arg);
                quote!(#prim_name_tokens<#prim_param_tokens>)
            } else {
                let prim_name_tokens = quote_escaped_ident(daml_type.name());
                quote!(#prim_name_tokens)
            },
        DamlType::TextMap(args) | DamlType::GenMap(args) =>
            if let (Some(k), Some(v)) = (args.first(), args.get(1)) {
                let prim_name_tokens = quote_escaped_ident(daml_type.name());
                let prim_key_param_tokens = quote_type(ctx, k);
                let prim_value_param_tokens = quote_type(ctx, v);
                quote!(#prim_name_tokens<#prim_key_param_tokens, #prim_value_param_tokens>)
            } else {
                let prim_name_tokens = quote_escaped_ident(daml_type.name());
                quote!(#prim_name_tokens)
            },
        DamlType::Optional(args) | DamlType::Numeric(args) =>
            if let Some(arg) = args.first() {
                let prim_name_tokens = quote_escaped_ident(daml_type.name());
                let prim_param_tokens = quote_type(ctx, arg);
                quote!(#prim_name_tokens<#prim_param_tokens>)
            } else {
                let prim_name_tokens = quote_escaped_ident(daml_type.name());
                quote!(#prim_name_tokens)
            },
        DamlType::ContractId(_) => quote_escaped_ident(daml_type.name()),
        DamlType::TyCon(tycon) => quote_tycon(ctx, tycon),
        DamlType::BoxedTyCon(tycon) => {
            let tycon = quote_tycon(ctx, tycon);
            quote!(Box<#tycon>)
        },
        DamlType::Var(var) => {
            let var_tokens = quote_ident(sanitize_var_name(var.var()).to_uppercase());
            quote!(#var_tokens)
        },
        DamlType::Nat(n) => quote_ident(format!("{}{}", daml_type.name(), n)),
        DamlType::Int64
        | DamlType::Text
        | DamlType::Timestamp
        | DamlType::Party
        | DamlType::Bool
        | DamlType::Unit
        | DamlType::Date => quote_escaped_ident(daml_type.name()),
        DamlType::Update
        | DamlType::Arrow
        | DamlType::Any
        | DamlType::TypeRep
        | DamlType::Bignumeric
        | DamlType::RoundingMode
        | DamlType::AnyException
        | DamlType::FailureCategory
        | DamlType::Forall(_)
        | DamlType::Struct(_)
        | DamlType::Syn(_) => panic!("cannot render unsupported type: {}", daml_type.name()),
    }
}

pub fn quote_tycon(ctx: &RenderContext<'_>, tycon: &DamlTyCon<'_>) -> TokenStream {
    let type_arguments_tokens = quote_generic_type_arguments(ctx, tycon.type_arguments());
    match tycon.tycon() {
        DamlTyConName::Local(local_tycon) => {
            let target_type_tokens = quote_escaped_ident(local_tycon.data_name());
            quote!(#target_type_tokens #type_arguments_tokens)
        },
        DamlTyConName::NonLocal(non_local_tycon) => {
            let target_type_tokens = quote_escaped_ident(non_local_tycon.data_name());
            let target_path_tokens = quote_non_local_path(non_local_tycon);
            quote!(#target_path_tokens #target_type_tokens #type_arguments_tokens)
        },
        DamlTyConName::Absolute(abs_tycon) => {
            let target_type_tokens = quote_escaped_ident(abs_tycon.data_name());
            let target_path_tokens = quote_absolute_tycon(ctx, abs_tycon);
            quote!(#target_path_tokens #target_type_tokens #type_arguments_tokens)
        },
    }
}

fn quote_generic_type_arguments(ctx: &RenderContext<'_>, type_arguments: &[DamlType<'_>]) -> TokenStream {
    if type_arguments.is_empty() {
        quote!()
    } else {
        let all_type_arguments: Vec<_> = type_arguments.iter().map(|t| quote_type(ctx, t)).collect();
        quote!( < #( #all_type_arguments ),* > )
    }
}

fn quote_absolute_tycon(_ctx: &RenderContext<'_>, abs_tycon: &DamlAbsoluteTyCon<'_>) -> TokenStream {
    let pkg_name = abs_tycon.package_name();
    let path: Vec<&str> = if pkg_name.is_empty() {
        abs_tycon.module_path().map(AsRef::as_ref).collect()
    } else {
        iter::once(pkg_name).chain(abs_tycon.module_path().map(AsRef::as_ref)).collect()
    };
    let target_path_tokens: Vec<_> =
        path.into_iter().map(ToSnakeCase::to_snake_case).map(quote_escaped_ident).collect();
    quote!(
        crate :: #( #target_path_tokens :: )*
    )
}

fn quote_non_local_path(tycon: &DamlNonLocalTyCon<'_>) -> TokenStream {
    let current_full_path: Vec<_> = iter::once(tycon.source_package_name())
        .chain(tycon.source_module_path().map(AsRef::as_ref))
        .map(ToSnakeCase::to_snake_case)
        .collect();
    let target_full_path: Vec<_> = iter::once(tycon.target_package_name())
        .chain(tycon.target_module_path().map(AsRef::as_ref))
        .map(ToSnakeCase::to_snake_case)
        .collect();
    let common_prefix_length =
        current_full_path.iter().zip(target_full_path.iter()).take_while(|(a, b)| a == b).count();
    let supers_needed = current_full_path.len().saturating_sub(common_prefix_length);
    let supers: Vec<_> = (0..supers_needed).map(|_| quote!(super)).collect();
    let target_tail_tokens: Vec<_> = target_full_path
        .iter()
        .skip(common_prefix_length)
        .map(|s| quote_escaped_ident(s.clone()))
        .collect();
    quote!( #( #supers :: )* #( #target_tail_tokens :: )* )
}

/// Replace spaces in a Daml type-var name with underscores so it can
/// be used as a Rust generic-parameter identifier. Not to be confused
/// with the crate-level `normalize_generic_param` (which strips the
/// trailing `_yyy` from `xxx_yyy` names).
fn sanitize_var_name(var: &str) -> String {
    var.replace(' ', "_")
}
