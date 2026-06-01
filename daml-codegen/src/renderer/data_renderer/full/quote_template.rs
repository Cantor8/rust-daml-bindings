use crate::renderer::data_renderer::full::quote_contract_struct::{
    quote_contract_id_struct_name, quote_contract_struct_and_impl, quote_contract_struct_name,
};
use crate::renderer::data_renderer::full::quote_interface::{quote_interface_choices, quote_interface_trait_path};
use crate::renderer::data_renderer::full::{quote_choice, quote_daml_record_and_impl};
use crate::renderer::{quote_escaped_ident, to_module_path, RenderContext};
use daml_lf::element::DamlTemplate;
use proc_macro2::TokenStream;
use quote::quote;

pub fn quote_daml_template(ctx: &RenderContext<'_>, daml_template: &DamlTemplate<'_>) -> TokenStream {
    let struct_and_impl_tokens = quote_daml_record_and_impl(ctx, daml_template.name(), daml_template.fields(), &[]);
    let template_id_method_tokens = quote_template_id_method(
        daml_template.name(),
        ctx.package_name_for(daml_template.package_id()),
        daml_template.package_id(),
        to_module_path(daml_template.module_path()),
    );
    let make_create_method_tokens = quote_make_create_command_method(daml_template.name());
    let contract_struct_and_impl_tokens = quote_contract_struct_and_impl(daml_template.name());
    let choices_impl_tokens = quote_choice(ctx, daml_template.name(), daml_template.choices());
    let interface_impls_tokens = quote_interface_impls(daml_template);
    let interface_choice_tokens = quote_implemented_interface_choices(ctx, daml_template);
    quote!(
        #struct_and_impl_tokens
        #template_id_method_tokens
        #make_create_method_tokens
        #contract_struct_and_impl_tokens
        #choices_impl_tokens
        #interface_impls_tokens
        #interface_choice_tokens
    )
}

/// For each interface this template implements, look up the
/// [`DamlInterface`] in the archive and emit its choice methods as
/// inherent methods on the template's contract id struct.
fn quote_implemented_interface_choices(ctx: &RenderContext<'_>, daml_template: &DamlTemplate<'_>) -> TokenStream {
    let blocks: Vec<_> = daml_template
        .implements()
        .iter()
        .filter_map(|iface_name| ctx.archive().interface_by_tycon_name(iface_name))
        .map(|iface| quote_interface_choices(ctx, daml_template.name(), iface))
        .collect();
    quote!(#( #blocks )*)
}

/// Emit `impl <Interface> for FooContractId {}` blocks for each
/// interface this template implements. Exercising a choice via the
/// interface uses `<FooContractId as Interface>::interface_id()`.
fn quote_interface_impls(daml_template: &DamlTemplate<'_>) -> TokenStream {
    let contract_id_struct_name_tokens = quote_contract_id_struct_name(daml_template.name());
    let impls: Vec<_> = daml_template
        .implements()
        .iter()
        .map(|iface| {
            let trait_path = quote_interface_trait_path(iface);
            quote!(impl #trait_path for #contract_id_struct_name_tokens {})
        })
        .collect();
    quote!(#( #impls )*)
}

/// Generate the `pub fn template_id() -> DamlIdentifier` method.
///
/// Prefers package-name addressing (the v2 Ledger API convention)
/// when the containing package has a name; falls back to addressing
/// by package-id otherwise (older LF archives, anonymous packages).
pub fn quote_template_id_method(
    struct_name: &str,
    package_name: Option<&str>,
    package_id: &str,
    module_name: String,
) -> TokenStream {
    let struct_name_tokens = quote_escaped_ident(struct_name);
    let entity_name = struct_name;
    let identifier_ctor = match package_name {
        Some(name) => quote!(DamlIdentifier::from_package_name(#name, #module_name, #entity_name)),
        None => quote!(DamlIdentifier::new(#package_id, #module_name, #entity_name)),
    };
    quote!(
        impl #struct_name_tokens {
            pub fn template_id() -> DamlIdentifier {
                #identifier_ctor
            }
        }
    )
}

/// Generate the `pub fn create(...) & pub fn create_command()` methods.
pub fn quote_make_create_command_method(struct_name: &str) -> TokenStream {
    let struct_name_tokens = quote_escaped_ident(struct_name);
    let _contract_struct_name = quote_contract_struct_name(struct_name);
    quote!(
        impl #struct_name_tokens {
            pub fn create_command(&self) -> DamlCreateCommand {
                let template_id = Self::template_id();
                let value: DamlValue = self.to_owned().serialize_into();
                DamlCreateCommand::new(template_id, value.try_take_record().unwrap())
            }
        }
    )
}
