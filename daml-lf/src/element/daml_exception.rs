use std::borrow::Cow;

use bounded_static::ToStatic;
use serde::Serialize;

#[cfg(feature = "full")]
use crate::element::DamlExpr;
use crate::element::visitor::{DamlElementVisitor, DamlVisitableElement};

/// A Daml exception. LF2's `DefException` is a tiny structure: a
/// dotted-name plus the expression that renders a user-facing error
/// message when the exception is raised.
///
/// 3.7 carries only the structural fields (name + location);
/// the `message` expression body is `full`-gated and lands in 3.8.
#[derive(Debug, Serialize, Clone, ToStatic)]
pub struct DamlException<'a> {
    name: Cow<'a, str>,
    package_id: Cow<'a, str>,
    module_path: Vec<Cow<'a, str>>,
    #[cfg(feature = "full")]
    message: DamlExpr<'a>,
}

impl<'a> DamlException<'a> {
    pub fn new(
        name: Cow<'a, str>,
        package_id: Cow<'a, str>,
        module_path: Vec<Cow<'a, str>>,
        #[cfg(feature = "full")] message: DamlExpr<'a>,
    ) -> Self {
        Self {
            name,
            package_id,
            module_path,
            #[cfg(feature = "full")]
            message,
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn package_id(&self) -> &str {
        &self.package_id
    }

    pub fn module_path(&self) -> impl Iterator<Item = &str> {
        self.module_path.iter().map(AsRef::as_ref)
    }

    #[cfg(feature = "full")]
    pub fn message(&self) -> &DamlExpr<'a> {
        &self.message
    }
}

impl<'a> DamlVisitableElement<'a> for DamlException<'a> {
    fn accept(&'a self, visitor: &'a mut impl DamlElementVisitor) {
        visitor.pre_visit_exception(self);
        #[cfg(feature = "full")]
        self.message.accept(visitor);
        visitor.post_visit_exception(self);
    }
}
