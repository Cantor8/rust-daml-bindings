//! LF2 expression-tree → element/ conversion.
//!
//! 3.8a sets up the scaffold: `convert_expr` enumerates every LF2
//! `Expr::Sum` variant exhaustively and returns an error for each
//! one. The shape of the conversion is in place, the harness
//! compiles under `--features full`, but no actual variant is wired
//! up yet. Subsequent sub-checkpoints (3.8b onwards) implement the
//! variants in groups:
//!
//!  - 3.8b: leaf variants (Var, Val, BuiltinCon, BuiltinLit, Nil,
//!    OptionalNone, BuiltinFunction).
//!  - 3.8c: record / variant / enum / struct construction +
//!    projection + update.
//!  - 3.8d: application / abstraction / case / let / cons /
//!    optional-some / type-rep.
//!  - 3.8e: exception expressions (Throw, ToAnyException,
//!    FromAnyException) + Any handling.
//!  - 3.8f: interface expressions (ToInterface, FromInterface,
//!    CallInterface, ViewInterface, …).
//!  - 3.8g: Update statement (its own nested oneof with ~12
//!    sub-variants).
//!
//! Until those land, any DAR with values, choice bodies, or other
//! Expr-typed surfaces will fail conversion under the `full`
//! feature with `DamlLfConvertError::MissingRequiredField`. Under
//! the default feature set, none of this code is reachable.

use crate::convert::package_payload::DamlPackagePayload;
use crate::convert::util::Required;
use crate::element::DamlExpr;
use crate::error::{DamlLfConvertError, DamlLfConvertResult};
use crate::lf_protobuf::daml_lf_2;
use crate::lf_protobuf::daml_lf_2::expr::Sum as ExprSum;

/// Convert an LF2 `Expr` into the element-layer [`DamlExpr`].
///
/// Currently returns `MissingRequiredField` for every variant —
/// scaffold only. See module-level docs.
pub fn convert_expr<'a>(
    proto: &daml_lf_2::Expr,
    _package: &'a DamlPackagePayload<'a>,
) -> DamlLfConvertResult<DamlExpr<'a>> {
    match proto.sum.as_ref().req()? {
        ExprSum::VarInternedStr(_)
        | ExprSum::Val(_)
        | ExprSum::Builtin(_)
        | ExprSum::BuiltinCon(_)
        | ExprSum::BuiltinLit(_)
        | ExprSum::RecCon(_)
        | ExprSum::RecProj(_)
        | ExprSum::RecUpd(_)
        | ExprSum::VariantCon(_)
        | ExprSum::EnumCon(_)
        | ExprSum::StructCon(_)
        | ExprSum::StructProj(_)
        | ExprSum::StructUpd(_)
        | ExprSum::App(_)
        | ExprSum::TyApp(_)
        | ExprSum::Abs(_)
        | ExprSum::TyAbs(_)
        | ExprSum::Case(_)
        | ExprSum::Let(_)
        | ExprSum::Nil(_)
        | ExprSum::Cons(_)
        | ExprSum::Update(_)
        | ExprSum::OptionalNone(_)
        | ExprSum::OptionalSome(_)
        | ExprSum::ToAny(_)
        | ExprSum::FromAny(_)
        | ExprSum::TypeRep(_)
        | ExprSum::ToAnyException(_)
        | ExprSum::FromAnyException(_)
        | ExprSum::Throw(_)
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
        | ExprSum::Experimental(_) => Err(DamlLfConvertError::MissingRequiredField),
    }
}
