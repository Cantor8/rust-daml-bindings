//! LF2 expression-tree → element/ conversion.
//!
//! 3.8e adds the exception-construction variants: `Throw`,
//! `ToAnyException`, and `FromAnyException`. `TryCatch` is part of
//! the `Update` sub-oneof and lands with 3.8g.
//!
//! Per-checkpoint scope:
//!  - 3.8b: leaves.
//!  - 3.8c: record / variant / enum / struct + To/FromAny.
//!  - 3.8d: App / Abs / Case / Let / Cons / OptionalSome.
//!  - 3.8e: Throw / ToAnyException / FromAnyException
//!    (this checkpoint).
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
use crate::convert::type_payload::{convert_tycon_id, convert_type, convert_type_con};
use crate::convert::typevar_payload::convert_typevar_with_kind;
use crate::convert::util::Required;
use crate::element::{
    DamlAbs, DamlApp, DamlBinding, DamlBlock, DamlBuiltinFunction, DamlCase, DamlCaseAlt, DamlCaseAltCons,
    DamlCaseAltEnum, DamlCaseAltOptionalSome, DamlCaseAltSum, DamlCaseAltVariant, DamlCons, DamlEnumCon, DamlExpr,
    DamlFieldWithExpr, DamlFromAny, DamlFromAnyException, DamlLocalValueName, DamlOptionalSome, DamlPrimCon,
    DamlPrimLit, DamlRecCon, DamlRecProj, DamlRecUpd, DamlStructCon, DamlStructProj, DamlStructUpd, DamlThrow,
    DamlToAny, DamlToAnyException, DamlTyAbs, DamlTyApp, DamlValueName, DamlVarWithType, DamlVariantCon,
};
use crate::error::{DamlLfConvertError, DamlLfConvertResult};
use crate::lf_protobuf::daml_lf_2;
use crate::lf_protobuf::daml_lf_2::builtin_lit::Sum as BuiltinLitSum;
use crate::lf_protobuf::daml_lf_2::case_alt::Sum as CaseAltSum;
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
        ExprSum::RecCon(rc) => {
            let tycon = convert_type_con(rc.tycon.as_ref().req()?, package)?;
            let fields = convert_field_exprs(&rc.fields, package)?;
            Ok(DamlExpr::RecCon(DamlRecCon::new(tycon, fields)))
        },
        ExprSum::RecProj(rp) => {
            let tycon = convert_type_con(rp.tycon.as_ref().req()?, package)?;
            let record = convert_expr(rp.record.as_deref().req()?, package)?;
            let field = package.resolve_string(rp.field_interned_str)?;
            Ok(DamlExpr::RecProj(DamlRecProj::new(tycon, Box::new(record), Cow::Borrowed(field))))
        },
        ExprSum::RecUpd(ru) => {
            let tycon = convert_type_con(ru.tycon.as_ref().req()?, package)?;
            let record = convert_expr(ru.record.as_deref().req()?, package)?;
            let update = convert_expr(ru.update.as_deref().req()?, package)?;
            let field = package.resolve_string(ru.field_interned_str)?;
            Ok(DamlExpr::RecUpd(DamlRecUpd::new(
                tycon,
                Box::new(record),
                Box::new(update),
                Cow::Borrowed(field),
            )))
        },
        ExprSum::VariantCon(vc) => {
            let tycon = convert_type_con(vc.tycon.as_ref().req()?, package)?;
            let variant_arg = convert_expr(vc.variant_arg.as_deref().req()?, package)?;
            let variant_con = package.resolve_string(vc.variant_con_interned_str)?;
            Ok(DamlExpr::VariantCon(DamlVariantCon::new(
                tycon,
                Box::new(variant_arg),
                Cow::Borrowed(variant_con),
            )))
        },
        ExprSum::EnumCon(ec) => {
            let tycon = convert_tycon_id(ec.tycon.as_ref().req()?, package)?;
            let enum_con = package.resolve_string(ec.enum_con_interned_str)?;
            Ok(DamlExpr::EnumCon(DamlEnumCon::new(Box::new(tycon), Cow::Borrowed(enum_con))))
        },
        ExprSum::StructCon(sc) => {
            let fields = convert_field_exprs(&sc.fields, package)?;
            Ok(DamlExpr::StructCon(DamlStructCon::new(fields)))
        },
        ExprSum::StructProj(sp) => {
            let struct_expr = convert_expr(sp.r#struct.as_deref().req()?, package)?;
            let field = package.resolve_string(sp.field_interned_str)?;
            Ok(DamlExpr::StructProj(DamlStructProj::new(Box::new(struct_expr), Cow::Borrowed(field))))
        },
        ExprSum::StructUpd(su) => {
            let struct_expr = convert_expr(su.r#struct.as_deref().req()?, package)?;
            let update = convert_expr(su.update.as_deref().req()?, package)?;
            let field = package.resolve_string(su.field_interned_str)?;
            Ok(DamlExpr::StructUpd(DamlStructUpd::new(
                Box::new(struct_expr),
                Box::new(update),
                Cow::Borrowed(field),
            )))
        },
        ExprSum::ToAny(ta) => {
            let ty = convert_type(ta.r#type.as_ref().req()?, package)?;
            let expr = convert_expr(ta.expr.as_deref().req()?, package)?;
            Ok(DamlExpr::ToAny(DamlToAny::new(ty, Box::new(expr))))
        },
        ExprSum::FromAny(fa) => {
            let ty = convert_type(fa.r#type.as_ref().req()?, package)?;
            let expr = convert_expr(fa.expr.as_deref().req()?, package)?;
            Ok(DamlExpr::FromAny(DamlFromAny::new(ty, Box::new(expr))))
        },
        ExprSum::App(app) => {
            let fun = convert_expr(app.fun.as_deref().req()?, package)?;
            let args = app.args.iter().map(|a| convert_expr(a, package)).collect::<DamlLfConvertResult<Vec<_>>>()?;
            Ok(DamlExpr::App(DamlApp::new(Box::new(fun), args)))
        },
        ExprSum::TyApp(ta) => {
            let expr = convert_expr(ta.expr.as_deref().req()?, package)?;
            let types =
                ta.types.iter().map(|t| convert_type(t, package)).collect::<DamlLfConvertResult<Vec<_>>>()?;
            Ok(DamlExpr::TyApp(DamlTyApp::new(Box::new(expr), types)))
        },
        ExprSum::Abs(abs) => {
            let params = abs
                .param
                .iter()
                .map(|p| convert_var_with_type(p, package))
                .collect::<DamlLfConvertResult<Vec<_>>>()?;
            let body = convert_expr(abs.body.as_deref().req()?, package)?;
            Ok(DamlExpr::Abs(DamlAbs::new(params, Box::new(body))))
        },
        ExprSum::TyAbs(ta) => {
            let params = ta
                .param
                .iter()
                .map(|p| convert_typevar_with_kind(p, package, package.interned_kinds_raw()))
                .collect::<DamlLfConvertResult<Vec<_>>>()?;
            let body = convert_expr(ta.body.as_deref().req()?, package)?;
            Ok(DamlExpr::TyAbs(DamlTyAbs::new(params, Box::new(body))))
        },
        ExprSum::Case(case) => {
            let scrut = convert_expr(case.scrut.as_deref().req()?, package)?;
            let alts =
                case.alts.iter().map(|a| convert_case_alt(a, package)).collect::<DamlLfConvertResult<Vec<_>>>()?;
            Ok(DamlExpr::Case(DamlCase::new(Box::new(scrut), alts)))
        },
        ExprSum::Let(block) => Ok(DamlExpr::Let(convert_block(block, package)?)),
        ExprSum::Cons(cons) => {
            let ty = convert_type(cons.r#type.as_ref().req()?, package)?;
            let front =
                cons.front.iter().map(|e| convert_expr(e, package)).collect::<DamlLfConvertResult<Vec<_>>>()?;
            let tail = convert_expr(cons.tail.as_deref().req()?, package)?;
            Ok(DamlExpr::Cons(DamlCons::new(ty, front, Box::new(tail))))
        },
        ExprSum::OptionalSome(os) => {
            let ty = convert_type(os.r#type.as_ref().req()?, package)?;
            let body = convert_expr(os.value.as_deref().req()?, package)?;
            Ok(DamlExpr::OptionalSome(DamlOptionalSome::new(ty, Box::new(body))))
        },
        ExprSum::ToAnyException(tae) => {
            let ty = convert_type(tae.r#type.as_ref().req()?, package)?;
            let expr = convert_expr(tae.expr.as_deref().req()?, package)?;
            Ok(DamlExpr::ToAnyException(DamlToAnyException::new(ty, Box::new(expr))))
        },
        ExprSum::FromAnyException(fae) => {
            let ty = convert_type(fae.r#type.as_ref().req()?, package)?;
            let expr = convert_expr(fae.expr.as_deref().req()?, package)?;
            Ok(DamlExpr::FromAnyException(DamlFromAnyException::new(ty, Box::new(expr))))
        },
        ExprSum::Throw(throw) => {
            let return_type = convert_type(throw.return_type.as_ref().req()?, package)?;
            let exception_type = convert_type(throw.exception_type.as_ref().req()?, package)?;
            let exception_expr = convert_expr(throw.exception_expr.as_deref().req()?, package)?;
            Ok(DamlExpr::Throw(DamlThrow::new(return_type, exception_type, Box::new(exception_expr))))
        },
        // 3.8f: interfaces.
        ExprSum::ToInterface(_)
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

/// Convert an LF2 `VarWithType` into a [`DamlVarWithType`].
fn convert_var_with_type<'a>(
    proto: &daml_lf_2::VarWithType,
    package: &'a DamlPackagePayload<'a>,
) -> DamlLfConvertResult<DamlVarWithType<'a>> {
    let var = package.resolve_string(proto.var_interned_str)?;
    let ty = convert_type(proto.r#type.as_ref().req()?, package)?;
    Ok(DamlVarWithType::new(ty, Cow::Borrowed(var)))
}

/// Convert an LF2 `Block` (used by both `Let` and `Update::Block`).
fn convert_block<'a>(
    proto: &daml_lf_2::Block,
    package: &'a DamlPackagePayload<'a>,
) -> DamlLfConvertResult<DamlBlock<'a>> {
    let bindings = proto
        .bindings
        .iter()
        .map(|b| {
            let binder = convert_var_with_type(b.binder.as_ref().req()?, package)?;
            let bound = convert_expr(b.bound.as_ref().req()?, package)?;
            Ok(DamlBinding::new(binder, bound))
        })
        .collect::<DamlLfConvertResult<Vec<_>>>()?;
    let body = convert_expr(proto.body.as_deref().req()?, package)?;
    Ok(DamlBlock::new(bindings, Box::new(body)))
}

/// Convert an LF2 `CaseAlt` (pattern + result body).
fn convert_case_alt<'a>(
    proto: &daml_lf_2::CaseAlt,
    package: &'a DamlPackagePayload<'a>,
) -> DamlLfConvertResult<DamlCaseAlt<'a>> {
    let body = convert_expr(proto.body.as_ref().req()?, package)?;
    let sum = match proto.sum.as_ref().req()? {
        CaseAltSum::Default(_) => DamlCaseAltSum::Default,
        CaseAltSum::Variant(v) => {
            let con = convert_tycon_id(v.con.as_ref().req()?, package)?;
            let variant = package.resolve_string(v.variant_interned_str)?;
            let binder = package.resolve_string(v.binder_interned_str)?;
            DamlCaseAltSum::Variant(DamlCaseAltVariant::new(con, Cow::Borrowed(variant), Cow::Borrowed(binder)))
        },
        CaseAltSum::BuiltinCon(code) => DamlCaseAltSum::PrimCon(convert_builtin_con(*code)?),
        CaseAltSum::Nil(_) => DamlCaseAltSum::Nil,
        CaseAltSum::Cons(c) => {
            let var_head = package.resolve_string(c.var_head_interned_str)?;
            let var_tail = package.resolve_string(c.var_tail_interned_str)?;
            DamlCaseAltSum::Cons(DamlCaseAltCons::new(Cow::Borrowed(var_head), Cow::Borrowed(var_tail)))
        },
        CaseAltSum::OptionalNone(_) => DamlCaseAltSum::OptionalNone,
        CaseAltSum::OptionalSome(os) => {
            let var_body = package.resolve_string(os.var_body_interned_str)?;
            DamlCaseAltSum::OptionalSome(DamlCaseAltOptionalSome::new(Cow::Borrowed(var_body)))
        },
        CaseAltSum::Enum(e) => {
            let con = convert_tycon_id(e.con.as_ref().req()?, package)?;
            let constructor = package.resolve_string(e.constructor_interned_str)?;
            DamlCaseAltSum::Enum(DamlCaseAltEnum::new(con, Cow::Borrowed(constructor)))
        },
    };
    Ok(DamlCaseAlt::new(body, sum))
}

/// Convert a slice of LF2 `FieldWithExpr` into the element-layer
/// [`DamlFieldWithExpr`] vector used by record / struct
/// construction.
fn convert_field_exprs<'a>(
    protos: &[daml_lf_2::FieldWithExpr],
    package: &'a DamlPackagePayload<'a>,
) -> DamlLfConvertResult<Vec<DamlFieldWithExpr<'a>>> {
    protos
        .iter()
        .map(|f| {
            let name = package.resolve_string(f.field_interned_str)?;
            let expr = convert_expr(f.expr.as_ref().req()?, package)?;
            Ok(DamlFieldWithExpr::new(Cow::Borrowed(name), expr))
        })
        .collect()
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
