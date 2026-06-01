use std::borrow::Cow;

use crate::convert::interned::PackageInternedResolver;
use crate::convert::package_payload::DamlPackagePayload;
use crate::convert::type_payload::convert_type;
use crate::convert::util::Required;
use crate::element::DamlField;
use crate::error::DamlLfConvertResult;
use crate::lf_protobuf::daml_lf_2;

/// Convert an LF2 `FieldWithType` (a `(name, type)` pair used by
/// records, variants, and struct types) into the element-layer
/// [`DamlField`].
pub fn convert_field<'a>(
    proto: &daml_lf_2::FieldWithType,
    package: &'a DamlPackagePayload<'a>,
) -> DamlLfConvertResult<DamlField<'a>> {
    let name = package.resolve_string(proto.field_interned_str)?;
    let ty = convert_type(proto.r#type.as_ref().req()?, package)?;
    Ok(DamlField::new(Cow::Borrowed(name), ty))
}
