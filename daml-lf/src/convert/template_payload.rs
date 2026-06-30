use std::borrow::Cow;

#[cfg(feature = "full")]
use crate::convert::expr_payload::convert_expr;
use crate::convert::interned::PackageInternedResolver;
use crate::convert::package_payload::DamlPackagePayload;
use crate::convert::type_payload::{convert_tycon_id, convert_type};
use crate::convert::util::Required;
use crate::element::{DamlChoice, DamlDefKey, DamlField, DamlTyConName};
use crate::error::{DamlLfConvertError, DamlLfConvertResult};
use crate::lf_protobuf::daml_lf_2;

/// Convert an LF2 `TemplateChoice` into the element-layer
/// [`DamlChoice`]. The arg-binder becomes a single-element `fields`
/// vector (`Vec<DamlField>` with one entry).
///
/// The choice's owning `package_id` is taken from `package` rather
/// than passed in; `module_path` is the only piece of context the
/// caller has to supply (it's clone-per-choice today because
/// [`DamlChoice`] stores the path by value — a structural change to
/// that field would let multiple choices share an `Arc<[..]>`).
///
/// Under `--features full`, the choice's Expr-typed bodies (update,
/// controllers, observers) are populated via [`convert_expr`].
/// The LF2.dev `authorizers` field is surfaced as
/// [`UnsupportedFeatureUsed`](DamlLfConvertError::UnsupportedFeatureUsed)
/// rather than silently dropped.
pub fn convert_choice<'a>(
    proto: &daml_lf_2::TemplateChoice,
    package: &'a DamlPackagePayload<'a>,
    module_path: &[Cow<'a, str>],
) -> DamlLfConvertResult<DamlChoice<'a>> {
    if proto.authorizers.is_some() {
        return Err(DamlLfConvertError::UnsupportedFeatureUsed(
            package.language_version().to_string(),
            "TemplateChoice.authorizers (LF2.dev-only)".into(),
            "2.dev".into(),
        ));
    }
    let name = package.resolve_string(proto.name_interned_str)?;
    let self_binder = package.resolve_string(proto.self_binder_interned_str)?;
    let arg_binder = proto.arg_binder.as_ref().req()?;
    let arg_name = package.resolve_string(arg_binder.var_interned_str)?;
    let arg_ty = convert_type(arg_binder.r#type.as_ref().req()?, package)?;
    let return_type = convert_type(proto.ret_type.as_ref().req()?, package)?;
    let arg_field = DamlField::new(Cow::Borrowed(arg_name), arg_ty);
    #[cfg(feature = "full")]
    let update = convert_expr(proto.update.as_ref().req()?, package)?;
    #[cfg(feature = "full")]
    let controllers = convert_expr(proto.controllers.as_ref().req()?, package)?;
    #[cfg(feature = "full")]
    let observers = convert_expr(proto.observers.as_ref().req()?, package)?;
    Ok(DamlChoice::new(
        Cow::Borrowed(name),
        Cow::Borrowed(package.package_id),
        module_path.to_vec(),
        vec![arg_field],
        return_type,
        proto.consuming,
        Cow::Borrowed(self_binder),
        #[cfg(feature = "full")]
        update,
        #[cfg(feature = "full")]
        controllers,
        #[cfg(feature = "full")]
        observers,
    ))
}

/// Convert an LF2 `DefTemplate::DefKey` into the element-layer
/// [`DamlDefKey`]. Only the key's type carries through under default
/// features; `maintainers` and `key_expr` are populated under
/// `--features full`.
pub fn convert_def_key<'a>(
    proto: &daml_lf_2::def_template::DefKey,
    package: &'a DamlPackagePayload<'a>,
) -> DamlLfConvertResult<DamlDefKey<'a>> {
    let ty = convert_type(proto.r#type.as_ref().req()?, package)?;
    #[cfg(feature = "full")]
    let maintainers = convert_expr(proto.maintainers.as_ref().req()?, package)?;
    #[cfg(feature = "full")]
    let key_expr = convert_expr(proto.key_expr.as_ref().req()?, package)?;
    Ok(DamlDefKey::new(
        ty,
        #[cfg(feature = "full")]
        maintainers,
        #[cfg(feature = "full")]
        key_expr,
    ))
}

/// Build the list of interfaces a template implements, by tycon name.
/// Only the tycon names are surfaced here; the method bodies and
/// view expression live behind `#[cfg(feature = "full")]`.
pub fn convert_implements<'a>(
    implements: &[daml_lf_2::def_template::Implements],
    package: &'a DamlPackagePayload<'a>,
) -> DamlLfConvertResult<Vec<DamlTyConName<'a>>> {
    implements.iter().map(|i| convert_tycon_id(i.interface.as_ref().req()?, package)).collect()
}
