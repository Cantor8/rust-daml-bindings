use std::borrow::Cow;

use crate::convert::interned::PackageInternedResolver;
use crate::convert::package_payload::DamlPackagePayload;
use crate::convert::template_payload::convert_choice;
use crate::convert::type_payload::{convert_tycon_id, convert_type};
use crate::convert::util::Required;
use crate::element::{DamlInterface, DamlInterfaceMethod};
use crate::error::DamlLfConvertResult;
use crate::lf_protobuf::daml_lf_2;

/// Convert an LF2 `DefInterface` into the element-layer
/// [`DamlInterface`].
///
/// Always handles the structural fields: name, param, methods, view
/// type, requires-list, and fixed choices (with their arg/return
/// types). The Expr-typed parts of fixed choices (controllers,
/// observers, update, authorizers) and the view's expression body
/// are gated on the `full` feature.
pub fn convert_interface<'a>(
    proto: &daml_lf_2::DefInterface,
    package: &'a DamlPackagePayload<'a>,
    module_path: &[Cow<'a, str>],
) -> DamlLfConvertResult<DamlInterface<'a>> {
    let name_segments = package.resolve_dotted(proto.tycon_interned_dname)?;
    let (name, prefix) = name_segments
        .split_last()
        .map(|(last, rest)| (*last, rest))
        .ok_or(crate::error::DamlLfConvertError::MissingRequiredField)?;
    let package_id = Cow::Borrowed(package.package_id);
    let mut full_module_path: Vec<Cow<'a, str>> = module_path.to_vec();
    full_module_path.extend(prefix.iter().copied().map(Cow::Borrowed));
    let param = package.resolve_string(proto.param_interned_str)?;
    let methods = proto
        .methods
        .iter()
        .map(|m| convert_method(m, package))
        .collect::<DamlLfConvertResult<Vec<_>>>()?;
    let choices = proto
        .choices
        .iter()
        .map(|c| convert_choice(c, package, &full_module_path))
        .collect::<DamlLfConvertResult<Vec<_>>>()?;
    let view = convert_type(proto.view.as_ref().req()?, package)?;
    let requires =
        proto.requires.iter().map(|r| convert_tycon_id(r, package)).collect::<DamlLfConvertResult<Vec<_>>>()?;
    Ok(DamlInterface::new(
        Cow::Borrowed(name),
        package_id,
        full_module_path,
        Cow::Borrowed(param),
        methods,
        choices,
        view,
        requires,
    ))
}

fn convert_method<'a>(
    proto: &daml_lf_2::InterfaceMethod,
    package: &'a DamlPackagePayload<'a>,
) -> DamlLfConvertResult<DamlInterfaceMethod<'a>> {
    let name = package.resolve_string(proto.method_interned_name)?;
    let ty = convert_type(proto.r#type.as_ref().req()?, package)?;
    Ok(DamlInterfaceMethod::new(Cow::Borrowed(name), ty))
}
