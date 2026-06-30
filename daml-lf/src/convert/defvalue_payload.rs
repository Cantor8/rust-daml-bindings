//! LF2 `DefValue` → element-layer [`DamlDefValue`] conversion.
//!
//! Only enabled under `--features full`, since `DamlDefValue` carries
//! a [`crate::element::DamlExpr`] body which is itself `full`-gated.

use std::borrow::Cow;

use crate::convert::expr_payload::convert_expr;
use crate::convert::interned::PackageInternedResolver;
use crate::convert::package_payload::DamlPackagePayload;
use crate::convert::type_payload::convert_type;
use crate::convert::util::Required;
use crate::element::DamlDefValue;
use crate::error::DamlLfConvertResult;
use crate::lf_protobuf::daml_lf_2;

/// Convert an LF2 `DefValue` into a [`DamlDefValue`].
///
/// The expression body is delegated to [`convert_expr`].
pub fn convert_def_value<'a>(
    proto: &daml_lf_2::DefValue,
    package: &'a DamlPackagePayload<'a>,
) -> DamlLfConvertResult<DamlDefValue<'a>> {
    let name_with_type = proto.name_with_type.as_ref().req()?;
    let name_segments = package.resolve_dotted(name_with_type.name_interned_dname)?;
    // Values in LF2 use dotted names too; element-layer keys by a
    // single string. Join the dotted name with '.' so collisions are
    // impossible and the dotted shape is preserved.
    let name: Cow<'a, str> = if name_segments.len() == 1 {
        Cow::Borrowed(name_segments[0])
    } else {
        Cow::Owned(name_segments.join("."))
    };
    let ty = convert_type(name_with_type.r#type.as_ref().req()?, package)?;
    let expr = convert_expr(proto.expr.as_ref().req()?, package)?;
    Ok(DamlDefValue::new(name, ty, expr))
}
