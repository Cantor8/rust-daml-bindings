use proc_macro2::TokenStream;

use quote::quote;

use crate::renderer::data_renderer::full::quote_contract_struct::quote_contract_id_struct_name;
use crate::renderer::renderer_utils::quote_escaped_ident;
use crate::renderer::type_renderer::quote_type;
use crate::renderer::RenderContext;
use daml_lf::element::DamlChoice;
use heck::ToSnakeCase;

pub fn quote_choice(ctx: &RenderContext<'_>, name: &str, items: &[DamlChoice<'_>]) -> TokenStream {
    let all_choice_methods_tokens = quote_all_choice_methods(ctx, name, items);
    let contract_struct_name_tokens = quote_contract_id_struct_name(name);
    quote!(
        impl #contract_struct_name_tokens {
            #all_choice_methods_tokens
        }
    )
}

/// Generate all choice methods within the parent `impl` block.
fn quote_all_choice_methods(ctx: &RenderContext<'_>, struct_name: &str, items: &[DamlChoice<'_>]) -> TokenStream {
    let all_choice_methods: Vec<_> = items.iter().map(|choice| quote_choice_method(ctx, struct_name, choice)).collect();
    quote!(
        #( #all_choice_methods )*
    )
}

/// Generate the `pub fn foo(&self, ...)` choice method.
///
/// The choice's wire argument is the bare argument-record (Daml's
/// `with` block); the convert layer surfaces it as a single
/// `DamlField` whose `ty` is the record type. Emit the method with
/// that record as the user-facing parameter and serialise it
/// directly into the exercise command's params.
fn quote_choice_method(ctx: &RenderContext<'_>, struct_name: &str, choice: &DamlChoice<'_>) -> TokenStream {
    let choice_name = &choice.name();
    let struct_name_tokens = quote_escaped_ident(struct_name);
    let method_name_command_tokens = quote_command_method_name(&choice.name().to_snake_case());
    let _method_name_tokens = quote_escaped_ident(&choice.name().to_snake_case());
    let arg_field = choice.fields().first().expect("choice must carry an arg field");
    let arg_type_tokens = quote_type(ctx, arg_field.ty());
    quote!(
        pub fn #method_name_command_tokens(&self, arg: impl Into<#arg_type_tokens>) -> DamlExerciseCommand {
            let template_id = #struct_name_tokens::template_id();
            let params: DamlValue =
                <#arg_type_tokens as DamlSerializeInto<DamlValue>>::serialize_into(arg.into());
            DamlExerciseCommand::new(
                template_id,
                self.contract_id().as_str(),
                #choice_name,
                params
            )
        }
    )
}

fn quote_command_method_name(name: &str) -> TokenStream {
    quote_escaped_ident(format!("{}_command", name))
}
