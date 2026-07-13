use heck::ToSnakeCase;
use itertools::Itertools;
use proc_macro2::{Ident, Span, TokenStream};
use quote::quote;
use std::convert::AsRef;

/// Quote a string as an identifier.
pub fn quote_ident(value: impl AsRef<str>) -> TokenStream {
    let ident = Ident::new(value.as_ref(), Span::call_site());
    quote!(#ident)
}

/// Escape and quote a string as an identifier.
pub fn quote_escaped_ident(value: impl AsRef<str>) -> TokenStream {
    quote_ident(escape_identifier(value))
}

pub fn make_ignored_ident(value: impl AsRef<str>) -> String {
    format!("_{}", value.as_ref())
}

/// Convert module path to a String.
pub fn to_module_path<'a, I: IntoIterator<Item = &'a str>>(path: I) -> String {
    path.into_iter().join(".")
}

/// Convert a string to a valid rust identifier.
pub fn to_rust_identifier(value: impl AsRef<str>) -> String {
    escape_identifier(value.as_ref().to_snake_case())
}

/// Return the leading segment of a `xxx_yyy` identifier, or the whole
/// input when there is no `_`. Empty input maps to empty.
pub fn normalize_generic_param(param: &str) -> &str {
    param.split_once('_').map_or(param, |(head, _)| head)
}

fn escape_identifier(value: impl AsRef<str>) -> String {
    let mut sanitized_ident: String = value
        .as_ref()
        .chars()
        .map(|c| {
            if matches!(c, '-' | '$' | '.') {
                '_'
            } else {
                c
            }
        })
        .collect();
    // Rust identifiers can't start with a digit. Daml's own grammar
    // forbids it for type / field / module names, but package and
    // archive names on the wire can be arbitrary — guard so a package
    // like "3d_engine" doesn't panic through Ident::new later.
    if sanitized_ident.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        sanitized_ident.insert(0, '_');
    }
    escape_keyword(&mut sanitized_ident);
    sanitized_ident
}

fn escape_keyword(ident: &mut String) -> &mut String {
    match ident.as_str() {
        "as" | "break" | "const" | "continue" | "else" | "enum" | "false" | "fn" | "for" | "if" | "impl" | "in"
        | "let" | "loop" | "match" | "mod" | "move" | "mut" | "pub" | "ref" | "return" | "static" | "struct"
        | "trait" | "true" | "type" | "unsafe" | "use" | "where" | "while" | "dyn" | "abstract" | "become" | "box"
        | "do" | "final" | "macro" | "override" | "priv" | "typeof" | "unsized" | "virtual" | "yield" | "async"
        | "await" | "try" | "self" | "super" | "extern" | "crate" | "gen" => *ident += "_",
        _ => (),
    }
    ident
}
