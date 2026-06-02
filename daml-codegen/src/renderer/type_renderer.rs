use crate::renderer::renderer_utils::quote_escaped_ident;
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
            let var_tokens = quote_ident(normalize_generic_param(var.var()).to_uppercase());
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
        | DamlType::Scenario
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

fn quote_absolute_tycon(ctx: &RenderContext<'_>, abs_tycon: &DamlAbsoluteTyCon<'_>) -> TokenStream {
    // The convert layer only fills in `package_name` for
    // self-references (see `convert_tycon_id` in `daml-lf`); for
    // cross-package refs the name is empty. Recover it by looking
    // the package up by id in the render-context archive — that's
    // the same map `RenderContext::package_name_for` consults.
    let resolved_pkg_name = if abs_tycon.package_name().is_empty() {
        ctx.package_name_for(abs_tycon.package_id())
    } else {
        Some(abs_tycon.package_name())
    };
    let path: Vec<&str> = match resolved_pkg_name {
        Some(name) if !name.is_empty() =>
            iter::once(name).chain(abs_tycon.module_path().map(AsRef::as_ref)).collect(),
        _ => abs_tycon.module_path().map(AsRef::as_ref).collect(),
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

fn normalize_generic_param(var: &str) -> String {
    var.replace(' ', "_")
}

fn quote_ident(s: impl AsRef<str>) -> TokenStream {
    let ident = proc_macro2::Ident::new(s.as_ref(), proc_macro2::Span::call_site());
    quote!(#ident)
}
