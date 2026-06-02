use proc_macro2::TokenStream;

use quote::quote;

use crate::renderer::quote_escaped_ident;
use crate::renderer::type_renderer::quote_type;
use crate::renderer::RenderContext;
use daml_lf::element::DamlField;

pub fn quote_fields(ctx: &RenderContext<'_>, field: &[&DamlField<'_>]) -> TokenStream {
    let all_fields_tokens: Vec<_> = field.iter().map(|&field| quote_field(ctx, field)).collect();
    quote!(
        #( #all_fields_tokens ),*
    )
}

fn quote_field(ctx: &RenderContext<'_>, field: &DamlField<'_>) -> TokenStream {
    let name_tokens = quote_escaped_ident(field.name());
    let type_tokens = quote_type(ctx, field.ty());
    quote!(
        #name_tokens: #type_tokens
    )
}
