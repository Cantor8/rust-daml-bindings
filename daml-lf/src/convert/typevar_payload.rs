use std::borrow::Cow;
use std::convert::TryFrom;

use crate::convert::interned::PackageInternedResolver;
use crate::convert::util::Required;
use crate::element::{DamlArrow, DamlKind, DamlTypeVarWithKind};
use crate::error::{DamlLfConvertError, DamlLfConvertResult};
use crate::lf_protobuf::daml_lf_2;
use crate::lf_protobuf::daml_lf_2::kind::Sum as KindSum;

/// Convert an LF2 `Kind` message into the element-layer
/// [`DamlKind`]. Recurses through the interned-kind table when the
/// wire references one.
///
/// `interned_kinds` is a `&[daml_lf_2::Kind]` taken from the
/// containing package; it's the table referenced by
/// `KindSum::InternedKind`. Always passed in to break the otherwise-
/// circular dependency between Kind and "the package it lives in."
pub fn convert_kind(
    kind: &daml_lf_2::Kind,
    interned_kinds: &[daml_lf_2::Kind],
) -> DamlLfConvertResult<DamlKind> {
    match kind.sum.as_ref().req()? {
        KindSum::Star(_) => Ok(DamlKind::Star),
        KindSum::Nat(_) => Ok(DamlKind::Nat),
        KindSum::Arrow(arrow) => {
            let params = arrow
                .params
                .iter()
                .map(|p| convert_kind(p, interned_kinds))
                .collect::<DamlLfConvertResult<Vec<_>>>()?;
            let result = convert_kind(arrow.result.as_deref().req()?, interned_kinds)?;
            Ok(DamlKind::Arrow(Box::new(DamlArrow::new(params, result))))
        },
        KindSum::InternedKind(idx) => {
            let idx_usize =
                usize::try_from(*idx).map_err(|_| DamlLfConvertError::MissingRequiredField)?;
            let resolved = interned_kinds.get(idx_usize).req()?;
            convert_kind(resolved, interned_kinds)
        },
    }
}

/// Convert an LF2 `TypeVarWithKind` into the element-layer
/// [`DamlTypeVarWithKind`].
pub fn convert_typevar_with_kind<'a, R: PackageInternedResolver>(
    proto: &daml_lf_2::TypeVarWithKind,
    resolver: &'a R,
    interned_kinds: &[daml_lf_2::Kind],
) -> DamlLfConvertResult<DamlTypeVarWithKind<'a>> {
    let var = resolver.resolve_string(proto.var_interned_str)?;
    let kind = convert_kind(proto.kind.as_ref().req()?, interned_kinds)?;
    Ok(DamlTypeVarWithKind::new(Cow::Borrowed(var), kind))
}
