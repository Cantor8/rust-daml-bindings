use std::borrow::Cow;

use crate::convert::interned::PackageInternedResolver;
use crate::convert::package_payload::DamlPackagePayload;
use crate::convert::type_payload::{convert_tycon_id, convert_type};
use crate::convert::util::Required;
use crate::element::{DamlChoice, DamlDefKey, DamlField, DamlTyConName};
use crate::error::DamlLfConvertResult;
use crate::lf_protobuf::daml_lf_2;

/// Convert an LF2 `TemplateChoice` into the element-layer
/// [`DamlChoice`]. The arg-binder becomes a single-element `fields`
/// vector (`Vec<DamlField>` with one entry).
///
/// 3.5 doesn't touch the choice's Expr-typed fields (controllers,
/// observers, update, authorizers). They live behind
/// `#[cfg(feature = "full")]` in [`DamlChoice`] and re-attach in
/// 3.8 when Expr conversion lands.
pub fn convert_choice<'a>(
    proto: &daml_lf_2::TemplateChoice,
    package: &'a DamlPackagePayload<'a>,
    module_path: &[Cow<'a, str>],
    package_id: &Cow<'a, str>,
) -> DamlLfConvertResult<DamlChoice<'a>> {
    let name = package.resolve_string(proto.name_interned_str)?;
    let self_binder = package.resolve_string(proto.self_binder_interned_str)?;
    let arg_binder = proto.arg_binder.as_ref().req()?;
    let arg_name = package.resolve_string(arg_binder.var_interned_str)?;
    let arg_ty = convert_type(arg_binder.r#type.as_ref().req()?, package)?;
    let return_type = convert_type(proto.ret_type.as_ref().req()?, package)?;
    let arg_field = DamlField::new(Cow::Borrowed(arg_name), arg_ty);
    Ok(DamlChoice::new(
        Cow::Borrowed(name),
        package_id.clone(),
        module_path.to_vec(),
        vec![arg_field],
        return_type,
        proto.consuming,
        Cow::Borrowed(self_binder),
        // Expr-typed choice bodies (update, controllers, observers,
        // authorizers) are populated in 3.8 under the `full` feature.
    ))
}

/// Convert an LF2 `DefTemplate::DefKey` into the element-layer
/// [`DamlDefKey`]. Only the key's type carries through in 3.5;
/// `maintainers` and `key_expr` are `full`-gated and land in 3.8.
pub fn convert_def_key<'a>(
    proto: &daml_lf_2::def_template::DefKey,
    package: &'a DamlPackagePayload<'a>,
) -> DamlLfConvertResult<DamlDefKey<'a>> {
    let ty = convert_type(proto.r#type.as_ref().req()?, package)?;
    Ok(DamlDefKey::new(ty))
}

/// Build the list of interfaces a template implements, by tycon name.
/// Body conversion (method values + view expression) is deferred:
/// methods and view expressions live behind `#[cfg(feature =
/// "full")]` and land in 3.8.
pub fn convert_implements<'a>(
    implements: &[daml_lf_2::def_template::Implements],
    package: &'a DamlPackagePayload<'a>,
) -> DamlLfConvertResult<Vec<DamlTyConName<'a>>> {
    implements.iter().map(|i| convert_tycon_id(i.interface.as_ref().req()?, package)).collect()
}
