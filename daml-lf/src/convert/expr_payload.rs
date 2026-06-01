//! LF2 expression-tree → element/ conversion.
//!
//! 3.8b implements the leaf variants of [`DamlExpr`]:
//! `VarInternedStr`, `Val`, `Builtin`, `BuiltinCon`, `BuiltinLit`,
//! `Nil`, `OptionalNone`, and `TypeRep`. All non-leaf variants still
//! return [`DamlLfConvertError::MissingRequiredField`] — those land
//! in 3.8c+ (record/variant/enum, App/Case/Let, Update, Throw,
//! interface ops).
//!
//! Per-checkpoint scope:
//!  - 3.8b: leaves (this checkpoint)
//!  - 3.8c: record / variant / enum / struct construction +
//!    projection + update + To/FromAny.
//!  - 3.8d: application / abstraction / case / let / cons /
//!    optional-some.
//!  - 3.8e: exception expressions (Throw, ToAnyException,
//!    FromAnyException).
//!  - 3.8f: interface expressions (ToInterface, FromInterface,
//!    CallInterface, ViewInterface, …).
//!  - 3.8g: Update statement (its own nested oneof with ~12
//!    sub-variants).
//!
//! LF2 grew several builtins and literals (FailWithStatus,
//! Keccak256Text, hex codecs, FailureCategory literals,
//! TypeRepTyconName) that the [`DamlBuiltinFunction`] /
//! [`DamlPrimLit`] enums in `element/` don't model yet. These
//! variants surface as `MissingRequiredField` until a real consumer
//! needs them; the path to fix is "add the enum variant in
//! `element/` and the corresponding arm here", not a structural
//! change.

use std::borrow::Cow;

use crate::convert::interned::PackageInternedResolver;
use crate::convert::package_payload::DamlPackagePayload;
use crate::convert::type_payload::convert_type;
use crate::convert::util::Required;
use crate::element::{
    DamlBuiltinFunction, DamlExpr, DamlLocalValueName, DamlPrimCon, DamlPrimLit, DamlValueName,
};
use crate::error::{DamlLfConvertError, DamlLfConvertResult};
use crate::lf_protobuf::daml_lf_2;
use crate::lf_protobuf::daml_lf_2::builtin_lit::Sum as BuiltinLitSum;
use crate::lf_protobuf::daml_lf_2::expr::Sum as ExprSum;
use crate::lf_protobuf::daml_lf_2::self_or_imported_package_id::Sum as PackageRefSum;
use crate::lf_protobuf::daml_lf_2::{BuiltinCon as BuiltinConProto, BuiltinFunction as BuiltinFunctionProto};

/// Convert an LF2 `Expr` into the element-layer [`DamlExpr`].
pub fn convert_expr<'a>(
    proto: &daml_lf_2::Expr,
    package: &'a DamlPackagePayload<'a>,
) -> DamlLfConvertResult<DamlExpr<'a>> {
    match proto.sum.as_ref().req()? {
        ExprSum::VarInternedStr(idx) => {
            let name = package.resolve_string(*idx)?;
            Ok(DamlExpr::Var(Cow::Borrowed(name)))
        },
        ExprSum::Val(value_id) => {
            let value_name = convert_value_id(value_id, package)?;
            Ok(DamlExpr::Val(Box::new(value_name)))
        },
        ExprSum::Builtin(code) => Ok(DamlExpr::Builtin(convert_builtin_function(*code)?)),
        ExprSum::BuiltinCon(code) => Ok(DamlExpr::PrimCon(convert_builtin_con(*code)?)),
        ExprSum::BuiltinLit(lit) => Ok(DamlExpr::PrimLit(convert_builtin_lit(lit, package)?)),
        ExprSum::Nil(nil) => {
            let ty = convert_type(nil.r#type.as_ref().req()?, package)?;
            Ok(DamlExpr::Nil(ty))
        },
        ExprSum::OptionalNone(none) => {
            let ty = convert_type(none.r#type.as_ref().req()?, package)?;
            Ok(DamlExpr::OptionalNone(ty))
        },
        ExprSum::TypeRep(ty) => Ok(DamlExpr::TypeRep(convert_type(ty, package)?)),
        // 3.8c+: record / variant / enum / struct.
        ExprSum::RecCon(_)
        | ExprSum::RecProj(_)
        | ExprSum::RecUpd(_)
        | ExprSum::VariantCon(_)
        | ExprSum::EnumCon(_)
        | ExprSum::StructCon(_)
        | ExprSum::StructProj(_)
        | ExprSum::StructUpd(_)
        | ExprSum::ToAny(_)
        | ExprSum::FromAny(_)
        // 3.8d: App / Case / Let / Cons / OptionalSome.
        | ExprSum::App(_)
        | ExprSum::TyApp(_)
        | ExprSum::Abs(_)
        | ExprSum::TyAbs(_)
        | ExprSum::Case(_)
        | ExprSum::Let(_)
        | ExprSum::Cons(_)
        | ExprSum::OptionalSome(_)
        // 3.8e: exceptions.
        | ExprSum::ToAnyException(_)
        | ExprSum::FromAnyException(_)
        | ExprSum::Throw(_)
        // 3.8f: interfaces.
        | ExprSum::ToInterface(_)
        | ExprSum::FromInterface(_)
        | ExprSum::CallInterface(_)
        | ExprSum::ViewInterface(_)
        | ExprSum::SignatoryInterface(_)
        | ExprSum::ObserverInterface(_)
        | ExprSum::UnsafeFromInterface(_)
        | ExprSum::ToRequiredInterface(_)
        | ExprSum::FromRequiredInterface(_)
        | ExprSum::UnsafeFromRequiredInterface(_)
        | ExprSum::InternedExpr(_)
        | ExprSum::InterfaceTemplateTypeRep(_)
        | ExprSum::ChoiceController(_)
        | ExprSum::ChoiceObserver(_)
        // 3.8g: Update.
        | ExprSum::Update(_)
        // 2.dev experimental — out of scope.
        | ExprSum::Experimental(_) => Err(DamlLfConvertError::MissingRequiredField),
    }
}

/// Convert an LF2 `ValueId` (module reference + interned dotted
/// name) into a [`DamlValueName`]. We always produce the `Local`
/// variant: the element layer's `Local`/`NonLocal` distinction is
/// stylistic, and downstream lookups go through the (package_id,
/// module_path, name) tuple either way (mirroring how
/// `convert_tycon_id` always emits `Absolute`).
fn convert_value_id<'a>(
    proto: &daml_lf_2::ValueId,
    package: &'a DamlPackagePayload<'a>,
) -> DamlLfConvertResult<DamlValueName<'a>> {
    let module = proto.module.as_ref().req()?;
    let pkg_id = match module.package_id.as_ref().req()?.sum.as_ref().req()? {
        PackageRefSum::SelfPackageId(_) => Cow::Borrowed(package.package_id),
        PackageRefSum::ImportedPackageIdInternedStr(idx) => Cow::Borrowed(package.resolve_string(*idx)?),
        PackageRefSum::PackageImportId(_) => return Err(DamlLfConvertError::MissingRequiredField),
    };
    let module_path: Vec<Cow<'a, str>> =
        package.resolve_dotted(module.module_name_interned_dname)?.into_iter().map(Cow::Borrowed).collect();
    // LF allows multi-segment value names; the element shape uses
    // the terminal segment as the name and folds the prefix into the
    // module path (same convention as `convert_tycon_id`).
    let name_segments = package.resolve_dotted(proto.name_interned_dname)?;
    let (name, prefix) = name_segments.split_last().req()?;
    let mut full_module_path = module_path;
    full_module_path.extend(prefix.iter().copied().map(Cow::Borrowed));
    Ok(DamlValueName::Local(DamlLocalValueName::new(
        Cow::Borrowed(*name),
        pkg_id,
        // Package-name resolution requires cross-package archive
        // context the convert layer doesn't carry yet — same posture
        // as `convert_tycon_id`.
        Cow::Borrowed(""),
        full_module_path,
    )))
}

/// Map the LF2 `BuiltinCon` enum to a [`DamlPrimCon`].
fn convert_builtin_con(code: i32) -> DamlLfConvertResult<DamlPrimCon> {
    let kind = BuiltinConProto::from_i32(code).req()?;
    Ok(match kind {
        BuiltinConProto::ConUnit => DamlPrimCon::Unit,
        BuiltinConProto::ConFalse => DamlPrimCon::False,
        BuiltinConProto::ConTrue => DamlPrimCon::True,
    })
}

/// Map a `BuiltinLit` oneof to a [`DamlPrimLit`].
fn convert_builtin_lit<'a>(
    proto: &daml_lf_2::BuiltinLit,
    package: &'a DamlPackagePayload<'a>,
) -> DamlLfConvertResult<DamlPrimLit<'a>> {
    match proto.sum.as_ref().req()? {
        BuiltinLitSum::Int64(v) => Ok(DamlPrimLit::Int64(*v)),
        BuiltinLitSum::Timestamp(v) => Ok(DamlPrimLit::Timestamp(*v)),
        BuiltinLitSum::Date(v) => Ok(DamlPrimLit::Date(*v)),
        BuiltinLitSum::NumericInternedStr(idx) => {
            let s = package.resolve_string(*idx)?;
            Ok(DamlPrimLit::Numeric(Cow::Borrowed(s)))
        },
        BuiltinLitSum::TextInternedStr(idx) => {
            let s = package.resolve_string(*idx)?;
            Ok(DamlPrimLit::Text(Cow::Borrowed(s)))
        },
        BuiltinLitSum::RoundingMode(code) => {
            use crate::element::RoundingMode;
            use crate::lf_protobuf::daml_lf_2::builtin_lit::RoundingMode as ProtoRm;
            let mode = ProtoRm::from_i32(*code).req()?;
            Ok(DamlPrimLit::RoundingMode(match mode {
                ProtoRm::Up => RoundingMode::Up,
                ProtoRm::Down => RoundingMode::Down,
                ProtoRm::Ceiling => RoundingMode::Ceiling,
                ProtoRm::Floor => RoundingMode::Floor,
                ProtoRm::HalfUp => RoundingMode::HalfUp,
                ProtoRm::HalfDown => RoundingMode::HalfDown,
                ProtoRm::HalfEven => RoundingMode::HalfEven,
                ProtoRm::Unnecessary => RoundingMode::Unnecessary,
            }))
        },
        // FailureCategory is the literal argument to FailWithStatus,
        // which the element-layer DamlPrimLit doesn't model yet.
        BuiltinLitSum::FailureCategory(_) => Err(DamlLfConvertError::MissingRequiredField),
    }
}

/// Map LF2's `BuiltinFunction` enum to the element-layer
/// [`DamlBuiltinFunction`]. Variants that exist only in LF2 (no
/// element-layer analog yet) surface as `MissingRequiredField`.
fn convert_builtin_function(code: i32) -> DamlLfConvertResult<DamlBuiltinFunction> {
    let kind = BuiltinFunctionProto::from_i32(code).req()?;
    Ok(match kind {
        BuiltinFunctionProto::Trace => DamlBuiltinFunction::Trace,
        BuiltinFunctionProto::Error => DamlBuiltinFunction::Error,
        BuiltinFunctionProto::Equal => DamlBuiltinFunction::Equal,
        BuiltinFunctionProto::LessEq => DamlBuiltinFunction::LessEq,
        BuiltinFunctionProto::Less => DamlBuiltinFunction::Less,
        BuiltinFunctionProto::GreaterEq => DamlBuiltinFunction::GreaterEq,
        BuiltinFunctionProto::Greater => DamlBuiltinFunction::Greater,
        BuiltinFunctionProto::AddInt64 => DamlBuiltinFunction::AddInt64,
        BuiltinFunctionProto::SubInt64 => DamlBuiltinFunction::SubInt64,
        BuiltinFunctionProto::MulInt64 => DamlBuiltinFunction::MulInt64,
        BuiltinFunctionProto::DivInt64 => DamlBuiltinFunction::DivInt64,
        BuiltinFunctionProto::ModInt64 => DamlBuiltinFunction::ModInt64,
        BuiltinFunctionProto::ExpInt64 => DamlBuiltinFunction::ExpInt64,
        BuiltinFunctionProto::AddNumeric => DamlBuiltinFunction::AddNumeric,
        BuiltinFunctionProto::SubNumeric => DamlBuiltinFunction::SubNumeric,
        BuiltinFunctionProto::MulNumeric => DamlBuiltinFunction::MulNumeric,
        BuiltinFunctionProto::DivNumeric => DamlBuiltinFunction::DivNumeric,
        BuiltinFunctionProto::RoundNumeric => DamlBuiltinFunction::RoundNumeric,
        BuiltinFunctionProto::CastNumeric => DamlBuiltinFunction::CastNumeric,
        BuiltinFunctionProto::ShiftNumeric => DamlBuiltinFunction::ShiftNumeric,
        BuiltinFunctionProto::Int64ToNumeric => DamlBuiltinFunction::Int64ToNumeric,
        BuiltinFunctionProto::NumericToInt64 => DamlBuiltinFunction::NumericToInt64,
        BuiltinFunctionProto::Int64ToText => DamlBuiltinFunction::Int64ToText,
        BuiltinFunctionProto::NumericToText => DamlBuiltinFunction::NumericToText,
        BuiltinFunctionProto::TimestampToText => DamlBuiltinFunction::TimestampToText,
        BuiltinFunctionProto::DateToText => DamlBuiltinFunction::DateToText,
        BuiltinFunctionProto::PartyToText => DamlBuiltinFunction::PartyToText,
        BuiltinFunctionProto::TextToParty => DamlBuiltinFunction::TextToParty,
        BuiltinFunctionProto::TextToInt64 => DamlBuiltinFunction::TextToInt64,
        BuiltinFunctionProto::TextToNumeric => DamlBuiltinFunction::TextToNumeric,
        BuiltinFunctionProto::ContractIdToText => DamlBuiltinFunction::ContractIdToText,
        BuiltinFunctionProto::Sha256Text => DamlBuiltinFunction::Sha256Text,
        BuiltinFunctionProto::ExplodeText => DamlBuiltinFunction::ExplodeText,
        BuiltinFunctionProto::AppendText => DamlBuiltinFunction::AppendText,
        BuiltinFunctionProto::ImplodeText => DamlBuiltinFunction::ImplodeText,
        BuiltinFunctionProto::CodePointsToText => DamlBuiltinFunction::CodePointsToText,
        // Element layer historically transposed the name; same op.
        BuiltinFunctionProto::TextToCodePoints => DamlBuiltinFunction::TextPointsToCode,
        BuiltinFunctionProto::DateToUnixDays => DamlBuiltinFunction::DateToUnixDays,
        BuiltinFunctionProto::UnixDaysToDate => DamlBuiltinFunction::UnixDaysToDate,
        BuiltinFunctionProto::TimestampToUnixMicroseconds => DamlBuiltinFunction::TimestampToUnixMicroseconds,
        BuiltinFunctionProto::UnixMicrosecondsToTimestamp => DamlBuiltinFunction::UnixMicrosecondsToTimestamp,
        BuiltinFunctionProto::CoerceContractId => DamlBuiltinFunction::CoerceContractId,
        BuiltinFunctionProto::Foldl => DamlBuiltinFunction::Foldl,
        BuiltinFunctionProto::Foldr => DamlBuiltinFunction::Foldr,
        BuiltinFunctionProto::EqualList => DamlBuiltinFunction::EqualList,
        BuiltinFunctionProto::GenmapEmpty => DamlBuiltinFunction::GenmapEmpty,
        BuiltinFunctionProto::GenmapInsert => DamlBuiltinFunction::GenmapInsert,
        BuiltinFunctionProto::GenmapLookup => DamlBuiltinFunction::GenmapLookup,
        BuiltinFunctionProto::GenmapDelete => DamlBuiltinFunction::GenmapDelete,
        BuiltinFunctionProto::GenmapKeys => DamlBuiltinFunction::GenmapKeys,
        BuiltinFunctionProto::GenmapValues => DamlBuiltinFunction::GenmapValues,
        BuiltinFunctionProto::GenmapSize => DamlBuiltinFunction::GenmapSize,
        BuiltinFunctionProto::TextmapEmpty => DamlBuiltinFunction::TextmapEmpty,
        BuiltinFunctionProto::TextmapInsert => DamlBuiltinFunction::TextmapInsert,
        BuiltinFunctionProto::TextmapLookup => DamlBuiltinFunction::TextmapLookup,
        BuiltinFunctionProto::TextmapDelete => DamlBuiltinFunction::TextmapDelete,
        BuiltinFunctionProto::TextmapToList => DamlBuiltinFunction::TextmapToList,
        BuiltinFunctionProto::TextmapSize => DamlBuiltinFunction::TextmapSize,
        BuiltinFunctionProto::AnyExceptionMessage => DamlBuiltinFunction::AnyExceptionMessage,
        BuiltinFunctionProto::ScaleBignumeric => DamlBuiltinFunction::ScaleBignumeric,
        BuiltinFunctionProto::PrecisionBignumeric => DamlBuiltinFunction::PrecisionBignumeric,
        BuiltinFunctionProto::AddBignumeric => DamlBuiltinFunction::AddBignumeric,
        BuiltinFunctionProto::SubBignumeric => DamlBuiltinFunction::SubBignumeric,
        BuiltinFunctionProto::MulBignumeric => DamlBuiltinFunction::MulBignumeric,
        BuiltinFunctionProto::DivBignumeric => DamlBuiltinFunction::DivBignumeric,
        BuiltinFunctionProto::ShiftRightBignumeric => DamlBuiltinFunction::ShiftRightBignumeric,
        BuiltinFunctionProto::BignumericToNumeric => DamlBuiltinFunction::BigNumericToNumeric,
        BuiltinFunctionProto::NumericToBignumeric => DamlBuiltinFunction::NumericToBigNumeric,
        BuiltinFunctionProto::BignumericToText => DamlBuiltinFunction::BigNumericToText,
        // LF2-only builtins that the element layer doesn't model
        // yet. Adding a new `DamlBuiltinFunction` variant is the fix.
        BuiltinFunctionProto::FailWithStatus
        | BuiltinFunctionProto::Keccak256Text
        | BuiltinFunctionProto::Secp256k1Bool
        | BuiltinFunctionProto::HexToText
        | BuiltinFunctionProto::TextToHex
        | BuiltinFunctionProto::Sha256Hex
        | BuiltinFunctionProto::Secp256k1WithEcdsaBool
        | BuiltinFunctionProto::Secp256k1ValidateKey
        | BuiltinFunctionProto::TypeRepTyconName => return Err(DamlLfConvertError::MissingRequiredField),
    })
}
