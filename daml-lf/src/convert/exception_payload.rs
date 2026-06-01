use std::borrow::Cow;

use crate::convert::interned::PackageInternedResolver;
use crate::convert::package_payload::DamlPackagePayload;
use crate::element::DamlException;
use crate::error::DamlLfConvertResult;
use crate::lf_protobuf::daml_lf_2;

/// Convert an LF2 `DefException` into the element-layer
/// [`DamlException`].
///
/// 3.7 carries name + module-path + package-id only. The `message`
/// expression body is `full`-feature and arrives in 3.8.
pub fn convert_exception<'a>(
    proto: &daml_lf_2::DefException,
    package: &'a DamlPackagePayload<'a>,
    module_path: &[Cow<'a, str>],
) -> DamlLfConvertResult<DamlException<'a>> {
    let name_segments = package.resolve_dotted(proto.name_interned_dname)?;
    let (name, prefix) = name_segments
        .split_last()
        .map(|(last, rest)| (*last, rest))
        .ok_or(crate::error::DamlLfConvertError::MissingRequiredField)?;
    let mut full_module_path: Vec<Cow<'a, str>> = module_path.to_vec();
    full_module_path.extend(prefix.iter().copied().map(Cow::Borrowed));
    Ok(DamlException::new(
        Cow::Borrowed(name),
        Cow::Borrowed(package.package_id),
        full_module_path,
        // `message` is an Expr — populated in 3.8 under the full
        // feature. Reference here would force an unused-import; the
        // call-site in build_exceptions skips it because the field
        // is #[cfg(feature = "full")] on DamlException::new.
    ))
}
