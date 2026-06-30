use std::borrow::Cow;

#[cfg(feature = "full")]
use crate::convert::expr_payload::convert_expr;
use crate::convert::interned::PackageInternedResolver;
use crate::convert::package_payload::DamlPackagePayload;
#[cfg(feature = "full")]
use crate::convert::util::Required;
use crate::element::DamlException;
use crate::error::DamlLfConvertResult;
use crate::lf_protobuf::daml_lf_2;

/// Convert an LF2 `DefException` into the element-layer
/// [`DamlException`].
///
/// Carries name + module-path + package-id under default features.
/// Under `--features full`, the `message` expression body is also
/// populated via [`convert_expr`].
pub fn convert_exception<'a>(
    proto: &daml_lf_2::DefException,
    package: &'a DamlPackagePayload<'a>,
    module_path: &[Cow<'a, str>],
) -> DamlLfConvertResult<DamlException<'a>> {
    let name_segments = package.resolve_dotted(proto.name_interned_dname)?;
    let (name, prefix) = name_segments.split_last().map(|(last, rest)| (*last, rest)).ok_or_else(|| {
        crate::error::DamlLfConvertError::InternalError(format!(
            "exception name_interned_dname {} resolves to an empty dotted-name",
            proto.name_interned_dname
        ))
    })?;
    let mut full_module_path: Vec<Cow<'a, str>> = module_path.to_vec();
    full_module_path.extend(prefix.iter().copied().map(Cow::Borrowed));
    #[cfg(feature = "full")]
    let message = convert_expr(proto.message.as_ref().req()?, package)?;
    Ok(DamlException::new(
        Cow::Borrowed(name),
        Cow::Borrowed(package.package_id),
        full_module_path,
        #[cfg(feature = "full")]
        message,
    ))
}
