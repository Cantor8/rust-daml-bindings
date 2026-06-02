use std::borrow::Cow;
use std::convert::TryFrom;

use crate::convert::field_payload::convert_field;
use crate::convert::interned::PackageInternedResolver;
use crate::convert::package_payload::DamlPackagePayload;
use crate::convert::typevar_payload::convert_typevar_with_kind;
use crate::convert::util::Required;
use crate::element::{
    DamlAbsoluteTyCon, DamlForall, DamlStruct, DamlSyn, DamlTyCon, DamlTyConName, DamlType, DamlTypeSynName,
    DamlTypeVarWithKind, DamlVar,
};
use crate::error::{DamlLfConvertError, DamlLfConvertResult};
use crate::lf_protobuf::daml_lf_2;
use crate::lf_protobuf::daml_lf_2::self_or_imported_package_id::Sum as PackageRefSum;
use crate::lf_protobuf::daml_lf_2::r#type::Sum as TypeSum;
use crate::lf_protobuf::daml_lf_2::BuiltinType;

/// Convert an LF2 `Type` message into the element-layer
/// [`DamlType`]. Recurses through the interned-types and
/// interned-kinds tables when the wire references them.
pub fn convert_type<'a>(
    proto: &daml_lf_2::Type,
    package: &'a DamlPackagePayload<'a>,
) -> DamlLfConvertResult<DamlType<'a>> {
    match proto.sum.as_ref().req()? {
        TypeSum::Var(v) => {
            let var = package.resolve_string(v.var_interned_str)?;
            let args = convert_types(&v.args, package)?;
            Ok(DamlType::Var(DamlVar::new(Cow::Borrowed(var), args)))
        },
        TypeSum::Con(c) => {
            let tycon = convert_tycon_id(c.tycon.as_ref().req()?, package)?;
            let args = convert_types(&c.args, package)?;
            Ok(DamlType::TyCon(DamlTyCon::new(Box::new(tycon), args)))
        },
        TypeSum::Syn(s) => {
            let tysyn = convert_tysyn_id(s.tysyn.as_ref().req()?, package)?;
            let args = convert_types(&s.args, package)?;
            Ok(DamlType::Syn(DamlSyn::new(Box::new(tysyn), args)))
        },
        TypeSum::Builtin(b) => convert_builtin(b, package),
        TypeSum::Forall(f) => {
            let vars = convert_typevars(&f.vars, package)?;
            let body = convert_type(f.body.as_deref().req()?, package)?;
            Ok(DamlType::Forall(DamlForall::new(vars, Box::new(body))))
        },
        TypeSum::Struct(s) => {
            let fields = s.fields.iter().map(|f| convert_field(f, package)).collect::<DamlLfConvertResult<_>>()?;
            Ok(DamlType::Struct(DamlStruct::new(fields)))
        },
        TypeSum::Nat(n) => {
            let nat = u8::try_from(*n).map_err(|_| DamlLfConvertError::MissingRequiredField)?;
            if nat > 37 {
                return Err(DamlLfConvertError::MissingRequiredField);
            }
            Ok(DamlType::Nat(nat))
        },
        TypeSum::InternedType(idx) => {
            let idx_usize = usize::try_from(*idx).map_err(|_| DamlLfConvertError::MissingRequiredField)?;
            let resolved = package.interned_types_raw().get(idx_usize).req()?;
            convert_type(resolved, package)
        },
        // TApp is a 2.dev-only flattening replacement; we don't see
        // it in 2.1 archives. Surface as a conversion error rather
        // than silently producing the wrong shape.
        TypeSum::Tapp(_) => Err(DamlLfConvertError::MissingRequiredField),
    }
}

fn convert_types<'a>(
    protos: &[daml_lf_2::Type],
    package: &'a DamlPackagePayload<'a>,
) -> DamlLfConvertResult<Vec<DamlType<'a>>> {
    protos.iter().map(|t| convert_type(t, package)).collect()
}

fn convert_typevars<'a>(
    protos: &[daml_lf_2::TypeVarWithKind],
    package: &'a DamlPackagePayload<'a>,
) -> DamlLfConvertResult<Vec<DamlTypeVarWithKind<'a>>> {
    protos.iter().map(|p| convert_typevar_with_kind(p, package, package.interned_kinds_raw())).collect()
}

/// Public wrapper around the typevar conversion so other convert
/// modules (data, module, template, …) can use it without pulling
/// in `convert::typevar_payload` directly.
pub fn convert_type_params<'a>(
    protos: &[daml_lf_2::TypeVarWithKind],
    package: &'a DamlPackagePayload<'a>,
) -> DamlLfConvertResult<Vec<DamlTypeVarWithKind<'a>>> {
    convert_typevars(protos, package)
}

fn convert_builtin<'a>(
    builtin: &daml_lf_2::r#type::Builtin,
    package: &'a DamlPackagePayload<'a>,
) -> DamlLfConvertResult<DamlType<'a>> {
    let kind = BuiltinType::from_i32(builtin.builtin).req()?;
    let args = convert_types(&builtin.args, package)?;
    Ok(match kind {
        BuiltinType::Unit => DamlType::Unit,
        BuiltinType::Bool => DamlType::Bool,
        BuiltinType::Int64 => DamlType::Int64,
        BuiltinType::Date => DamlType::Date,
        BuiltinType::Timestamp => DamlType::Timestamp,
        BuiltinType::Numeric => DamlType::Numeric(args),
        BuiltinType::Party => DamlType::Party,
        BuiltinType::Text => DamlType::Text,
        BuiltinType::ContractId => DamlType::ContractId(args.into_iter().next().map(Box::new)),
        BuiltinType::Optional => DamlType::Optional(args),
        BuiltinType::List => DamlType::List(args),
        BuiltinType::Genmap => DamlType::GenMap(args),
        BuiltinType::Textmap => DamlType::TextMap(args),
        BuiltinType::Any => DamlType::Any,
        BuiltinType::AnyException => DamlType::AnyException,
        BuiltinType::TypeRep => DamlType::TypeRep,
        BuiltinType::Arrow => DamlType::Arrow,
        BuiltinType::Update => DamlType::Update,
        BuiltinType::Bignumeric => DamlType::Bignumeric,
        BuiltinType::FailureCategory => DamlType::FailureCategory,
        BuiltinType::RoundingMode => DamlType::RoundingMode,
    })
}

/// Convert an LF2 `r#type::Con` (a possibly-applied type constructor)
/// into a [`DamlTyCon`]: the constructor name plus its type
/// arguments. The result is the same shape produced by the matching
/// arm in [`convert_type`]. Used by the `convert_expr` path under
/// `--features full`.
#[allow(dead_code)]
pub fn convert_type_con<'a>(
    proto: &daml_lf_2::r#type::Con,
    package: &'a DamlPackagePayload<'a>,
) -> DamlLfConvertResult<DamlTyCon<'a>> {
    let tycon = convert_tycon_id(proto.tycon.as_ref().req()?, package)?;
    let args = convert_types(&proto.args, package)?;
    Ok(DamlTyCon::new(Box::new(tycon), args))
}

/// Convert an LF2 `TypeConId` into a [`DamlTyConName::Absolute`].
///
/// 3.4 always produces the `Absolute` variant — the `Local` /
/// `NonLocal` distinction in the element layer is a stylistic
/// convenience for downstream code rather than a correctness
/// requirement; lookups via `DamlArchive::data_by_tycon_name` work
/// against `Absolute` just fine.
pub fn convert_tycon_id<'a>(
    proto: &daml_lf_2::TypeConId,
    package: &'a DamlPackagePayload<'a>,
) -> DamlLfConvertResult<DamlTyConName<'a>> {
    let (pkg_id, module_path) = resolve_module_id(proto.module.as_ref().req()?, package)?;
    let data_name_segments = package.resolve_dotted(proto.name_interned_dname)?;
    // The element shape uses the *last* dotted-name segment as the
    // data name; preceding segments are folded into the module path.
    // (LF allows nested data type names but in practice DAML compiles
    // them as flat names; we conservatively use the last segment
    // and append the prefix to module_path.)
    let (data_name, extra_module_path) = split_data_name(&data_name_segments)?;
    let mut full_module_path: Vec<Cow<'a, str>> = module_path.into_iter().map(Cow::Borrowed).collect();
    full_module_path.extend(extra_module_path.iter().copied().map(Cow::Borrowed));
    // Resolve `package_name` for both self- and cross-package
    // references. Self-refs use `package.name` directly;
    // cross-package refs go through the archive-level
    // `pkg_names` table that `DamlArchivePayload::try_from`
    // stamps onto every package. When no table is available
    // (out-of-archive reference or a payload built standalone)
    // we fall back to the empty string, matching the previous
    // behaviour.
    let package_name = resolve_package_name(package, &pkg_id);
    Ok(DamlTyConName::Absolute(DamlAbsoluteTyCon::new(
        Cow::Borrowed(data_name),
        Cow::Owned(pkg_id),
        package_name,
        full_module_path,
    )))
}

/// Resolve a package-id to its package-name. For self-references
/// the current package's name is used directly; for cross-package
/// references the package's `pkg_names` table (populated by
/// [`DamlArchivePayload::try_from`]) is consulted. Returns an empty
/// `Cow` when the id is unknown (out-of-archive reference or a
/// standalone payload).
pub(crate) fn resolve_package_name<'a>(package: &'a DamlPackagePayload<'a>, pkg_id: &str) -> Cow<'a, str> {
    if pkg_id == package.package_id {
        Cow::Borrowed(package.name.as_str())
    } else {
        match package.cross_pkg_name(pkg_id) {
            Some(name) => Cow::Owned(name.to_owned()),
            None => Cow::Borrowed(""),
        }
    }
}

/// Convert an LF2 `TypeSynId` into a [`DamlTypeSynName`].
pub fn convert_tysyn_id<'a>(
    proto: &daml_lf_2::TypeSynId,
    package: &'a DamlPackagePayload<'a>,
) -> DamlLfConvertResult<DamlTypeSynName<'a>> {
    let (pkg_id, module_path) = resolve_module_id(proto.module.as_ref().req()?, package)?;
    let data_name_segments = package.resolve_dotted(proto.name_interned_dname)?;
    let (data_name, extra_module_path) = split_data_name(&data_name_segments)?;
    let mut full_module_path: Vec<Cow<'a, str>> = module_path.into_iter().map(Cow::Borrowed).collect();
    full_module_path.extend(extra_module_path.iter().copied().map(Cow::Borrowed));
    // DamlTypeSynName is an alias for DamlTyConName today — both use
    // the same (data_name, package_id, package_name, module_path)
    // shape — so we reuse DamlAbsoluteTyCon here.
    let package_name = resolve_package_name(package, &pkg_id);
    Ok(DamlTypeSynName::Absolute(DamlAbsoluteTyCon::new(
        Cow::Borrowed(data_name),
        Cow::Owned(pkg_id),
        package_name,
        full_module_path,
    )))
}

/// Resolve a `ModuleId` to (package_id, module_path_segments).
fn resolve_module_id<'a, R: PackageInternedResolver>(
    proto: &daml_lf_2::ModuleId,
    resolver: &'a R,
) -> DamlLfConvertResult<(String, Vec<&'a str>)> {
    let pkg_id = resolve_package_ref(proto.package_id.as_ref().req()?, resolver)?;
    let module_path = resolver.resolve_dotted(proto.module_name_interned_dname)?;
    Ok((pkg_id, module_path))
}

/// Resolve a `SelfOrImportedPackageId` to a package-id string.
fn resolve_package_ref<R: PackageInternedResolver>(
    proto: &daml_lf_2::SelfOrImportedPackageId,
    resolver: &R,
) -> DamlLfConvertResult<String> {
    match proto.sum.as_ref().req()? {
        PackageRefSum::SelfPackageId(_) => Ok(resolver.package_id().to_owned()),
        PackageRefSum::ImportedPackageIdInternedStr(idx) => Ok(resolver.resolve_string(*idx)?.to_owned()),
        // PackageImports table (2.dev only) — not yet handled.
        PackageRefSum::PackageImportId(_) => Err(DamlLfConvertError::MissingRequiredField),
    }
}

/// Split a dotted-name (`["Foo", "Bar", "Baz"]`) into its terminal
/// element and the prefix that precedes it. The two lifetimes are
/// kept distinct deliberately: the inner `&'a str` lives at the
/// interning-table lifetime (the package), while the outer `&'b`
/// borrows from the local `Vec` that called this function.
fn split_data_name<'a, 'b>(segments: &'b [&'a str]) -> DamlLfConvertResult<(&'a str, &'b [&'a str])> {
    let (last, rest) = segments.split_last().req()?;
    Ok((*last, rest))
}
