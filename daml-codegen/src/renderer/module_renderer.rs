use heck::ToSnakeCase;
use proc_macro2::TokenStream;

use quote::quote;

use crate::generator::ModuleMatcher;
use crate::generator::RenderMethod;
use crate::renderer::{quote_all_data, quote_all_interfaces, quote_escaped_ident, to_module_path, RenderContext};
use daml_lf::element::DamlModule;

pub fn quote_module_tree(
    ctx: &RenderContext<'_>,
    name: &str,
    module: &DamlModule<'_>,
    module_matcher: &ModuleMatcher,
    render_method: &RenderMethod,
) -> TokenStream {
    quote_module_tree_inner(ctx, name, module, module_matcher, render_method, false)
}

/// Inner traversal that additionally carries
/// `ancestor_real_included` — whether the *nearest* non-synthetic
/// ancestor of the current module matched the module matcher.
/// Synthetic modules (created by the convert layer to host
/// variant-record-payload records) render their contents under
/// that ancestor's inclusion, not their own path (which the
/// matcher was never given).
///
/// The signal is *not* transitive past non-synthetic boundaries:
/// passing through a real-but-unmatched module resets it. Without
/// this, `ModuleMatcher`'s `matches("")` special case (the root
/// module always matches) would cascade down through every
/// intermediate and synthetic descendants would render even when
/// their real LF ancestor was filtered out.
fn quote_module_tree_inner(
    ctx: &RenderContext<'_>,
    name: &str,
    module: &DamlModule<'_>,
    module_matcher: &ModuleMatcher,
    render_method: &RenderMethod,
    ancestor_real_included: bool,
) -> TokenStream {
    let self_real_included = !module.is_synthetic() && module_matcher.matches(&to_module_path(module.path()));
    // Synthetic modules render data only when an enclosing real
    // module was included; real modules render data only when they
    // themselves were matched.
    let render_data = self_real_included || (module.is_synthetic() && ancestor_real_included);
    // Children inherit the *current* non-synthetic match state, not
    // the union with the ancestor state — passing through a real
    // module that wasn't matched resets the "nearest real ancestor
    // is included" signal. Synthetic intermediates pass the signal
    // through unchanged.
    let child_ancestor_included = if module.is_synthetic() {
        ancestor_real_included
    } else {
        self_real_included
    };
    let all_children: Vec<_> = module
        .child_modules()
        .map(|child| {
            quote_module_tree_inner(
                ctx,
                child.local_name(),
                child,
                module_matcher,
                render_method,
                child_ancestor_included,
            )
        })
        .collect();
    let all_empty_children = all_children.iter().all(TokenStream::is_empty);
    if !render_data && all_empty_children {
        quote!()
    } else {
        let module_tokens = if render_data {
            let data_tokens =
                quote_all_data(ctx, module.data_types().collect::<Vec<_>>().as_slice(), render_method);
            let interfaces: Vec<_> = module.interfaces().collect();
            let interface_tokens = quote_all_interfaces(ctx, interfaces.as_slice(), render_method);
            quote!(
                #data_tokens
                #interface_tokens
            )
        } else {
            quote!()
        };
        let module_name_tokens = quote_escaped_ident(name.to_snake_case());
        quote!(
            pub mod #module_name_tokens {
                use ::daml::prelude::*;
                #module_tokens
                #( #all_children )*
            }
        )
    }
}
