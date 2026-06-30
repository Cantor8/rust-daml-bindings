//! LF2 expression-tree → element/ conversion.
//!
//! Converts the entire LF2 expression sub-language (including the
//! `Update` effect language used by template choice bodies) under
//! `--features full`. The conversion is shape-preserving: every
//! prost-generated `ExprSum` / `UpdateSum` / `CaseAltSum` variant
//! maps to an element-layer `Daml*` counterpart with the same
//! structure.
//!
//! Known gaps:
//!  - 2.dev `InternedExpr` references the package's interned-
//!    expressions table, which isn't yet exposed on the element
//!    layer — surfaces as `InternalError`.
//!  - 2.dev `Experimental` escape hatch and `PackageImportId`
//!    package refs surface as `UnsupportedFeatureUsed`.
//!  - LF2-only builtins / literals that have no element-layer enum
//!    variant yet (`FailWithStatus`, `Keccak256Text`, hex codecs,
//!    `TypeRepTyconName`) surface as `UnknownBuiltinFunction` with
//!    the wire integer. The fix per case is "add the enum variant
//!    in `element/` and the corresponding arm here", not a
//!    structural change.

use std::borrow::Cow;

use crate::convert::interned::PackageInternedResolver;
use crate::convert::package_payload::DamlPackagePayload;
use crate::convert::type_payload::{convert_tycon_id, convert_type, convert_type_con};
use crate::convert::typevar_payload::convert_typevar_with_kind;
use crate::convert::util::Required;
use crate::element::{
    DamlAbs, DamlApp, DamlBinding, DamlBlock, DamlBuiltinFunction, DamlCase, DamlCaseAlt, DamlCaseAltCons,
    DamlCaseAltEnum, DamlCaseAltOptionalSome, DamlCaseAltSum, DamlCaseAltVariant, DamlCons, DamlCreate,
    DamlCreateInterface, DamlEnumCon, DamlExercise, DamlExerciseByKey, DamlExerciseInterface, DamlExpr, DamlFetch,
    DamlFetchInterface, DamlFieldWithExpr, DamlFromAny, DamlFromAnyException, DamlInterfaceExpr, DamlLocalValueName,
    DamlOptionalSome, DamlPrimCon, DamlPrimLit, DamlPure, DamlQueryNByKey, DamlRecCon, DamlRecProj, DamlRecUpd,
    DamlRetrieveByKey, DamlStructCon, DamlStructProj, DamlStructUpd, DamlThrow, DamlToAny, DamlToAnyException,
    DamlTryCatch, DamlTyAbs, DamlTyApp, DamlUpdate, DamlUpdateEmbedExpr, DamlValueName, DamlVarWithType,
    DamlVariantCon,
};
use crate::error::{DamlLfConvertError, DamlLfConvertResult};
use crate::lf_protobuf::daml_lf_2;
use crate::lf_protobuf::daml_lf_2::builtin_lit::Sum as BuiltinLitSum;
use crate::lf_protobuf::daml_lf_2::case_alt::Sum as CaseAltSum;
use crate::lf_protobuf::daml_lf_2::expr::Sum as ExprSum;
use crate::lf_protobuf::daml_lf_2::self_or_imported_package_id::Sum as PackageRefSum;
use crate::lf_protobuf::daml_lf_2::update::Sum as UpdateSum;
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
            Ok(DamlExpr::RecUpd(DamlRecUpd::new(tycon, Box::new(record), Box::new(update), Cow::Borrowed(field))))
        },
        ExprSum::VariantCon(vc) => {
            let tycon = convert_type_con(vc.tycon.as_ref().req()?, package)?;
            let variant_arg = convert_expr(vc.variant_arg.as_deref().req()?, package)?;
            let variant_con = package.resolve_string(vc.variant_con_interned_str)?;
            Ok(DamlExpr::VariantCon(DamlVariantCon::new(tycon, Box::new(variant_arg), Cow::Borrowed(variant_con))))
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
            Ok(DamlExpr::StructUpd(DamlStructUpd::new(Box::new(struct_expr), Box::new(update), Cow::Borrowed(field))))
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
            let types = ta.types.iter().map(|t| convert_type(t, package)).collect::<DamlLfConvertResult<Vec<_>>>()?;
            Ok(DamlExpr::TyApp(DamlTyApp::new(Box::new(expr), types)))
        },
        ExprSum::Abs(abs) => {
            let params =
                abs.param.iter().map(|p| convert_var_with_type(p, package)).collect::<DamlLfConvertResult<Vec<_>>>()?;
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
            let front = cons.front.iter().map(|e| convert_expr(e, package)).collect::<DamlLfConvertResult<Vec<_>>>()?;
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
        ExprSum::ToInterface(ti) => Ok(DamlExpr::InterfaceOp(DamlInterfaceExpr::ToInterface {
            interface_type: Box::new(convert_tycon_id(ti.interface_type.as_ref().req()?, package)?),
            template_type: Box::new(convert_tycon_id(ti.template_type.as_ref().req()?, package)?),
            template_expr: Box::new(convert_expr(ti.template_expr.as_deref().req()?, package)?),
        })),
        ExprSum::FromInterface(fi) => Ok(DamlExpr::InterfaceOp(DamlInterfaceExpr::FromInterface {
            interface_type: Box::new(convert_tycon_id(fi.interface_type.as_ref().req()?, package)?),
            template_type: Box::new(convert_tycon_id(fi.template_type.as_ref().req()?, package)?),
            interface_expr: Box::new(convert_expr(fi.interface_expr.as_deref().req()?, package)?),
        })),
        ExprSum::CallInterface(ci) => {
            let method = package.resolve_string(ci.method_interned_name)?;
            Ok(DamlExpr::InterfaceOp(DamlInterfaceExpr::CallInterface {
                interface_type: Box::new(convert_tycon_id(ci.interface_type.as_ref().req()?, package)?),
                method: Cow::Borrowed(method),
                interface_expr: Box::new(convert_expr(ci.interface_expr.as_deref().req()?, package)?),
            }))
        },
        ExprSum::ViewInterface(vi) => Ok(DamlExpr::InterfaceOp(DamlInterfaceExpr::ViewInterface {
            interface: Box::new(convert_tycon_id(vi.interface.as_ref().req()?, package)?),
            expr: Box::new(convert_expr(vi.expr.as_deref().req()?, package)?),
        })),
        ExprSum::SignatoryInterface(si) => Ok(DamlExpr::InterfaceOp(DamlInterfaceExpr::SignatoryInterface {
            interface: Box::new(convert_tycon_id(si.interface.as_ref().req()?, package)?),
            expr: Box::new(convert_expr(si.expr.as_deref().req()?, package)?),
        })),
        ExprSum::ObserverInterface(oi) => Ok(DamlExpr::InterfaceOp(DamlInterfaceExpr::ObserverInterface {
            interface: Box::new(convert_tycon_id(oi.interface.as_ref().req()?, package)?),
            expr: Box::new(convert_expr(oi.expr.as_deref().req()?, package)?),
        })),
        ExprSum::UnsafeFromInterface(ufi) => Ok(DamlExpr::InterfaceOp(DamlInterfaceExpr::UnsafeFromInterface {
            interface_type: Box::new(convert_tycon_id(ufi.interface_type.as_ref().req()?, package)?),
            template_type: Box::new(convert_tycon_id(ufi.template_type.as_ref().req()?, package)?),
            contract_id_expr: Box::new(convert_expr(ufi.contract_id_expr.as_deref().req()?, package)?),
            interface_expr: Box::new(convert_expr(ufi.interface_expr.as_deref().req()?, package)?),
        })),
        ExprSum::ToRequiredInterface(tri) => Ok(DamlExpr::InterfaceOp(DamlInterfaceExpr::ToRequiredInterface {
            required_interface: Box::new(convert_tycon_id(tri.required_interface.as_ref().req()?, package)?),
            requiring_interface: Box::new(convert_tycon_id(tri.requiring_interface.as_ref().req()?, package)?),
            expr: Box::new(convert_expr(tri.expr.as_deref().req()?, package)?),
        })),
        ExprSum::FromRequiredInterface(fri) => Ok(DamlExpr::InterfaceOp(DamlInterfaceExpr::FromRequiredInterface {
            required_interface: Box::new(convert_tycon_id(fri.required_interface.as_ref().req()?, package)?),
            requiring_interface: Box::new(convert_tycon_id(fri.requiring_interface.as_ref().req()?, package)?),
            expr: Box::new(convert_expr(fri.expr.as_deref().req()?, package)?),
        })),
        ExprSum::UnsafeFromRequiredInterface(ufri) => {
            Ok(DamlExpr::InterfaceOp(DamlInterfaceExpr::UnsafeFromRequiredInterface {
                required_interface: Box::new(convert_tycon_id(ufri.required_interface.as_ref().req()?, package)?),
                requiring_interface: Box::new(convert_tycon_id(ufri.requiring_interface.as_ref().req()?, package)?),
                contract_id_expr: Box::new(convert_expr(ufri.contract_id_expr.as_deref().req()?, package)?),
                interface_expr: Box::new(convert_expr(ufri.interface_expr.as_deref().req()?, package)?),
            }))
        },
        ExprSum::InterfaceTemplateTypeRep(ittr) => {
            Ok(DamlExpr::InterfaceOp(DamlInterfaceExpr::InterfaceTemplateTypeRep {
                interface: Box::new(convert_tycon_id(ittr.interface.as_ref().req()?, package)?),
                expr: Box::new(convert_expr(ittr.expr.as_deref().req()?, package)?),
            }))
        },
        ExprSum::ChoiceController(cc) => {
            let choice = package.resolve_string(cc.choice_interned_str)?;
            Ok(DamlExpr::InterfaceOp(DamlInterfaceExpr::ChoiceController {
                template: Box::new(convert_tycon_id(cc.template.as_ref().req()?, package)?),
                choice: Cow::Borrowed(choice),
                contract_expr: Box::new(convert_expr(cc.contract_expr.as_deref().req()?, package)?),
                choice_arg_expr: Box::new(convert_expr(cc.choice_arg_expr.as_deref().req()?, package)?),
            }))
        },
        ExprSum::ChoiceObserver(co) => {
            let choice = package.resolve_string(co.choice_interned_str)?;
            Ok(DamlExpr::InterfaceOp(DamlInterfaceExpr::ChoiceObserver {
                template: Box::new(convert_tycon_id(co.template.as_ref().req()?, package)?),
                choice: Cow::Borrowed(choice),
                contract_expr: Box::new(convert_expr(co.contract_expr.as_deref().req()?, package)?),
                choice_arg_expr: Box::new(convert_expr(co.choice_arg_expr.as_deref().req()?, package)?),
            }))
        },
        ExprSum::Update(update) => Ok(DamlExpr::Update(convert_update(update, package)?)),
        ExprSum::InternedExpr(idx) => {
            let i = usize::try_from(*idx)
                .map_err(|_| DamlLfConvertError::InternalError(format!("negative interned-expr index {idx}")))?;
            let resolved = package
                .interned_exprs_raw()
                .get(i)
                .ok_or_else(|| DamlLfConvertError::InternalError(format!("interned-expr index {i} out of range")))?;
            convert_expr(resolved, package)
        },
        // 2.dev experimental — out of scope.
        ExprSum::Experimental(_) => Err(DamlLfConvertError::UnsupportedFeatureUsed(
            package.language_version().to_string(),
            "Experimental expression".into(),
            "2.dev".into(),
        )),
    }
}

/// Convert an LF2 `Update` (the effect language used by template
/// choice bodies) into a [`DamlUpdate`].
fn convert_update<'a>(
    proto: &daml_lf_2::Update,
    package: &'a DamlPackagePayload<'a>,
) -> DamlLfConvertResult<DamlUpdate<'a>> {
    match proto.sum.as_ref().req()? {
        UpdateSum::Pure(pure) => {
            let ty = convert_type(pure.r#type.as_ref().req()?, package)?;
            let expr = convert_expr(pure.expr.as_deref().req()?, package)?;
            Ok(DamlUpdate::Pure(DamlPure::new(ty, Box::new(expr))))
        },
        UpdateSum::Block(block) => Ok(DamlUpdate::Block(convert_block(block, package)?)),
        UpdateSum::Create(create) => {
            let template = convert_tycon_id(create.template.as_ref().req()?, package)?;
            let expr = convert_expr(create.expr.as_deref().req()?, package)?;
            Ok(DamlUpdate::Create(DamlCreate::new(Box::new(template), Box::new(expr))))
        },
        UpdateSum::CreateInterface(ci) => {
            let interface = convert_tycon_id(ci.interface.as_ref().req()?, package)?;
            let expr = convert_expr(ci.expr.as_deref().req()?, package)?;
            Ok(DamlUpdate::CreateInterface(DamlCreateInterface::new(Box::new(interface), Box::new(expr))))
        },
        UpdateSum::Exercise(ex) => {
            let template = convert_tycon_id(ex.template.as_ref().req()?, package)?;
            let cid = convert_expr(ex.cid.as_deref().req()?, package)?;
            let arg = convert_expr(ex.arg.as_deref().req()?, package)?;
            let choice = package.resolve_string(ex.choice_interned_str)?;
            Ok(DamlUpdate::Exercise(DamlExercise::new(
                Box::new(template),
                Box::new(cid),
                Box::new(arg),
                Cow::Borrowed(choice),
            )))
        },
        UpdateSum::ExerciseInterface(ei) => {
            let interface = convert_tycon_id(ei.interface.as_ref().req()?, package)?;
            let cid = convert_expr(ei.cid.as_deref().req()?, package)?;
            let arg = convert_expr(ei.arg.as_deref().req()?, package)?;
            let choice = package.resolve_string(ei.choice_interned_str)?;
            let guard = ei.guard.as_deref().map(|g| convert_expr(g, package)).transpose()?.map(Box::new);
            Ok(DamlUpdate::ExerciseInterface(DamlExerciseInterface::new(
                Box::new(interface),
                Cow::Borrowed(choice),
                Box::new(cid),
                Box::new(arg),
                guard,
            )))
        },
        UpdateSum::ExerciseByKey(ebk) => {
            let template = convert_tycon_id(ebk.template.as_ref().req()?, package)?;
            let choice = package.resolve_string(ebk.choice_interned_str)?;
            let key = convert_expr(ebk.key.as_deref().req()?, package)?;
            let arg = convert_expr(ebk.arg.as_deref().req()?, package)?;
            Ok(DamlUpdate::ExerciseByKey(DamlExerciseByKey::new(
                Box::new(template),
                Cow::Borrowed(choice),
                Box::new(key),
                Box::new(arg),
            )))
        },
        UpdateSum::Fetch(f) => {
            let template = convert_tycon_id(f.template.as_ref().req()?, package)?;
            let cid = convert_expr(f.cid.as_deref().req()?, package)?;
            Ok(DamlUpdate::Fetch(DamlFetch::new(Box::new(template), Box::new(cid))))
        },
        UpdateSum::FetchInterface(fi) => {
            let interface = convert_tycon_id(fi.interface.as_ref().req()?, package)?;
            let cid = convert_expr(fi.cid.as_deref().req()?, package)?;
            Ok(DamlUpdate::FetchInterface(DamlFetchInterface::new(Box::new(interface), Box::new(cid))))
        },
        UpdateSum::GetTime(_) => Ok(DamlUpdate::GetTime),
        UpdateSum::LookupByKey(rbk) => {
            let template = convert_tycon_id(rbk.template.as_ref().req()?, package)?;
            // The proto's RetrieveByKey only carries the template id;
            // the key value is supplied via surrounding application
            // structure. Emit an OptionalNone-of-Unit as the
            // placeholder key (DamlRetrieveByKey requires *some* expr).
            Ok(DamlUpdate::LookupByKey(DamlRetrieveByKey::new(
                Box::new(template),
                Box::new(DamlExpr::OptionalNone(crate::element::DamlType::Unit)),
            )))
        },
        UpdateSum::FetchByKey(rbk) => {
            let template = convert_tycon_id(rbk.template.as_ref().req()?, package)?;
            Ok(DamlUpdate::FetchByKey(DamlRetrieveByKey::new(
                Box::new(template),
                Box::new(DamlExpr::OptionalNone(crate::element::DamlType::Unit)),
            )))
        },
        UpdateSum::QueryNByKey(qbk) => {
            let template = convert_tycon_id(qbk.template.as_ref().req()?, package)?;
            Ok(DamlUpdate::QueryNByKey(DamlQueryNByKey::new(Box::new(template))))
        },
        UpdateSum::EmbedExpr(ee) => {
            let ty = convert_type(ee.r#type.as_ref().req()?, package)?;
            let body = convert_expr(ee.body.as_deref().req()?, package)?;
            Ok(DamlUpdate::EmbedExpr(DamlUpdateEmbedExpr::new(ty, Box::new(body))))
        },
        UpdateSum::TryCatch(tc) => {
            let return_type = convert_type(tc.return_type.as_ref().req()?, package)?;
            let try_expr = convert_expr(tc.try_expr.as_deref().req()?, package)?;
            let var = package.resolve_string(tc.var_interned_str)?;
            let catch_expr = convert_expr(tc.catch_expr.as_deref().req()?, package)?;
            Ok(DamlUpdate::TryCatch(DamlTryCatch::new(
                return_type,
                Box::new(try_expr),
                Cow::Borrowed(var),
                Box::new(catch_expr),
            )))
        },
        UpdateSum::LedgerTimeLt(expr) => Ok(DamlUpdate::LedgerTimeLt(Box::new(convert_expr(expr, package)?))),
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
/// stylistic, and downstream lookups go through the (`package_id`,
/// `module_path`, name) tuple either way (mirroring how
/// `convert_tycon_id` always emits `Absolute`).
fn convert_value_id<'a>(
    proto: &daml_lf_2::ValueId,
    package: &'a DamlPackagePayload<'a>,
) -> DamlLfConvertResult<DamlValueName<'a>> {
    let module = proto.module.as_ref().req()?;
    let pkg_id = match module.package_id.as_ref().req()?.sum.as_ref().req()? {
        PackageRefSum::SelfPackageId(_) => Cow::Borrowed(package.package_id),
        PackageRefSum::ImportedPackageIdInternedStr(idx) => Cow::Borrowed(package.resolve_string(*idx)?),
        PackageRefSum::PackageImportId(_) => {
            return Err(DamlLfConvertError::UnsupportedFeatureUsed(
                package.language_version().to_string(),
                "PackageImportId in value-name PackageRef".into(),
                "2.dev".into(),
            ));
        },
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
    let package_name = crate::convert::type_payload::resolve_package_name(package, &pkg_id);
    Ok(DamlValueName::Local(DamlLocalValueName::new(Cow::Borrowed(*name), pkg_id, package_name, full_module_path)))
}

/// Map the LF2 `BuiltinCon` enum to a [`DamlPrimCon`].
fn convert_builtin_con(code: i32) -> DamlLfConvertResult<DamlPrimCon> {
    let kind = BuiltinConProto::try_from(code).map_err(|_| DamlLfConvertError::UnknownPrimCon(code))?;
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
            let mode = ProtoRm::try_from(*code).map_err(|_| DamlLfConvertError::UnknownRoundingMode(*code))?;
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
        BuiltinLitSum::FailureCategory(code) => {
            use crate::element::FailureCategory;
            use crate::lf_protobuf::daml_lf_2::builtin_lit::FailureCategory as ProtoFc;
            let cat = ProtoFc::try_from(*code).map_err(|_| {
                DamlLfConvertError::InternalError(format!("unknown FailureCategory enum variant {code}"))
            })?;
            Ok(DamlPrimLit::FailureCategory(match cat {
                ProtoFc::InvalidIndependentOfSystemState => FailureCategory::InvalidIndependentOfSystemState,
                ProtoFc::InvalidGivenCurrentSystemStateOther => FailureCategory::InvalidGivenCurrentSystemStateOther,
            }))
        },
    }
}

/// Map LF2's `BuiltinFunction` enum to the element-layer
/// [`DamlBuiltinFunction`]. Variants that exist only in LF2 (no
/// element-layer analog yet) surface as `MissingRequiredField`.
fn convert_builtin_function(code: i32) -> DamlLfConvertResult<DamlBuiltinFunction> {
    let kind = BuiltinFunctionProto::try_from(code).map_err(|_| DamlLfConvertError::UnknownBuiltinFunction(code))?;
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
        BuiltinFunctionProto::FailWithStatus => DamlBuiltinFunction::FailWithStatus,
        BuiltinFunctionProto::Keccak256Text => DamlBuiltinFunction::Keccak256Text,
        BuiltinFunctionProto::Secp256k1Bool => DamlBuiltinFunction::Secp256k1Bool,
        BuiltinFunctionProto::Secp256k1WithEcdsaBool => DamlBuiltinFunction::Secp256k1WithEcdsaBool,
        BuiltinFunctionProto::Secp256k1ValidateKey => DamlBuiltinFunction::Secp256k1ValidateKey,
        BuiltinFunctionProto::HexToText => DamlBuiltinFunction::HexToText,
        BuiltinFunctionProto::TextToHex => DamlBuiltinFunction::TextToHex,
        BuiltinFunctionProto::Sha256Hex => DamlBuiltinFunction::Sha256Hex,
        BuiltinFunctionProto::TypeRepTyconName => DamlBuiltinFunction::TypeRepTyconName,
    })
}
