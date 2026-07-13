//! Render Daml-LF interfaces (LF2) as Rust marker traits.
//!
//! Each interface becomes a `pub trait <IName>` with a default
//! `interface_id() -> DamlIdentifier` method that addresses the
//! interface by package-name (matching the v2 Ledger API convention
//! used by [`super::quote_template::quote_template_id_method`]).
//!
//! Templates that implement the interface receive an
//! `impl <IName> for <FooContractId> {}` block — emitted from
//! `quote_template.rs` — making `FooContractId` usable wherever an
//! `<IName>`-bound value is expected.
//!
//! Interface-declared choices become `<iface>_<choice>_command(...)`
//! methods on the implementing template's contract id struct (see
//! [`quote_interface_choices`]). The `<iface>_` prefix avoids
//! collisions with the template's own choices and with choices from
//! other implemented interfaces.

use crate::renderer::data_renderer::full::quote_contract_struct::quote_contract_id_struct_name;
use crate::renderer::renderer_utils::quote_escaped_ident;
use crate::renderer::type_renderer::quote_type;
use crate::renderer::{RenderContext, to_module_path};
use daml_lf::element::{DamlInterface, DamlTyConName};
use heck::ToSnakeCase;
use proc_macro2::TokenStream;
use quote::quote;
use std::iter;

/// Render a `crate::pkg_name::module::path::Iface` token path for an
/// interface's tycon name. Mirrors [`crate::renderer::type_renderer`]'s
/// `quote_absolute_tycon`; used to refer to interface traits from
/// implementing-template impl blocks (which may live in a different
/// module).
pub fn quote_interface_trait_path(tycon: &DamlTyConName<'_>) -> TokenStream {
    let (package_name, module_path, data_name) = match tycon {
        DamlTyConName::Absolute(abs) => (abs.package_name(), abs.module_path().collect::<Vec<_>>(), abs.data_name()),
        DamlTyConName::Local(local) => {
            (local.package_name(), local.module_path().collect::<Vec<_>>(), local.data_name())
        },
        DamlTyConName::NonLocal(nlocal) => {
            (nlocal.target_package_name(), nlocal.target_module_path().collect::<Vec<_>>(), nlocal.data_name())
        },
    };
    let path: Vec<&str> = if package_name.is_empty() {
        module_path
    } else {
        iter::once(package_name).chain(module_path).collect()
    };
    let segments: Vec<_> = path.into_iter().map(ToSnakeCase::to_snake_case).map(quote_escaped_ident).collect();
    let name_tokens = quote_escaped_ident(data_name);
    quote!(crate :: #( #segments :: )* #name_tokens)
}

/// Render the absolute `crate::pkg::module::Iface` path for an
/// interface given its package-name, module-path, and entity-name.
/// Used when we have the [`DamlInterface`] directly (not a tycon
/// name) — e.g. when generating interface-choice methods on a
/// template's contract id.
fn quote_interface_path_from_parts<'a, M: IntoIterator<Item = &'a str>>(
    package_name: Option<&'a str>,
    module_path: M,
    entity_name: &str,
) -> TokenStream {
    let path: Vec<&str> = match package_name {
        Some(name) if !name.is_empty() => iter::once(name).chain(module_path).collect(),
        _ => module_path.into_iter().collect(),
    };
    let segments: Vec<_> = path.into_iter().map(ToSnakeCase::to_snake_case).map(quote_escaped_ident).collect();
    let name_tokens = quote_escaped_ident(entity_name);
    quote!(crate :: #( #segments :: )* #name_tokens)
}

/// Emit interface-choice exercise methods on a template's contract
/// id struct. For each choice declared on the interface, generate
///
/// ```ignore
/// pub fn <iface_snake>_<choice_snake>_command(&self, ...) -> DamlExerciseCommand {
///     let template_id = <Iface>::interface_id();
///     ...
///     DamlExerciseCommand::new(template_id, self.contract_id().as_str(), "Choice", params)
/// }
/// ```
///
/// The interface-name prefix avoids collisions with the template's
/// own choice methods (and with choices from other implemented
/// interfaces).
pub fn quote_interface_choices(
    ctx: &RenderContext<'_>,
    template_name: &str,
    interface: &DamlInterface<'_>,
) -> TokenStream {
    let contract_id_struct = quote_contract_id_struct_name(template_name);
    let iface_path = quote_interface_path_from_parts(
        ctx.package_name_for(interface.package_id()),
        interface.module_path(),
        interface.name(),
    );
    let iface_prefix = interface.name().to_snake_case();
    let method_tokens: Vec<_> = interface
        .choices()
        .iter()
        .map(|choice| {
            let choice_name_lit = choice.name();
            let choice_snake = choice.name().to_snake_case();
            let method_ident = quote_escaped_ident(format!("{iface_prefix}_{choice_snake}_command"));
            // v2 Daml expects the choice argument as the bare record
            // type (e.g. `Reassign { target: Party }`), not wrapped
            // in an outer `{ arg: ... }` envelope. The convert layer
            // surfaces the choice's signature as a single `arg`
            // DamlField whose `ty` is the record type — emit the
            // method with that record as the parameter and
            // serialise it directly.
            let arg_field = choice.fields().first().expect("choice must carry an arg field");
            let arg_type_tokens = quote_type(ctx, arg_field.ty());
            quote!(
                pub fn #method_ident(&self, arg: impl Into<#arg_type_tokens>) -> DamlExerciseCommand {
                    // Qualified path-call: a trait method without
                    // `&self` can't be invoked via `Trait::method()`
                    // because Rust can't infer the `Self` type. The
                    // `<Self as Iface>::interface_id()` form pins it.
                    let template_id = <Self as #iface_path>::interface_id();
                    let params: DamlValue =
                        <#arg_type_tokens as DamlSerializeInto<DamlValue>>::serialize_into(arg.into());
                    DamlExerciseCommand::new(
                        template_id,
                        self.contract_id().as_str(),
                        #choice_name_lit,
                        params
                    )
                }
            )
        })
        .collect();
    if method_tokens.is_empty() {
        quote!()
    } else {
        quote!(
            impl #contract_id_struct {
                #( #method_tokens )*
            }
        )
    }
}

pub fn quote_daml_interface(ctx: &RenderContext<'_>, interface: &DamlInterface<'_>) -> TokenStream {
    let trait_name_tokens = quote_escaped_ident(interface.name());
    let module_name = to_module_path(interface.module_path());
    let entity_name = interface.name();
    let identifier_ctor = match ctx.package_name_for(interface.package_id()) {
        Some(name) => quote!(DamlIdentifier::from_package_name(#name, #module_name, #entity_name)),
        None => {
            let package_id = interface.package_id();
            quote!(DamlIdentifier::new(#package_id, #module_name, #entity_name))
        },
    };
    quote!(
        /// Marker trait for the Daml interface.
        ///
        /// Templates implementing this interface receive a generated
        /// `impl` block; address them in exercise commands by
        /// `<Template as #trait_name_tokens>::interface_id()`.
        pub trait #trait_name_tokens {
            /// Returns the interface's `DamlIdentifier`, addressed
            /// by package-name when the source package is named.
            fn interface_id() -> DamlIdentifier {
                #identifier_ctor
            }
        }
    )
}
