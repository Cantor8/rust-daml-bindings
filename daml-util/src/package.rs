use std::collections::{HashMap, HashSet, VecDeque};

use daml_grpc::DamlGrpcClient;
use daml_grpc::data::package::DamlPackage;
use daml_grpc::data::{DamlError, DamlResult};
use daml_lf::element::{
    DamlAbsoluteTyCon, DamlElementVisitor, DamlNonLocalTyCon, DamlNonLocalValueName, DamlPackage as DamlLfPackage,
    DamlVisitableElement,
};
use daml_lf::{DamlLfArchive, DamlLfArchivePayload, DamlLfHashFunction, DarFile, DarManifest};
use futures::StreamExt;
use futures::stream::FuturesUnordered;
use uuid::Uuid;

/// Convenience methods for working with a collection of [`DamlPackage`].
///
/// In the following example a [`DamlPackages`] is created from all known [`DamlPackage`] on a Daml ledger and then
/// converted into [`DarFile`] using the [`ArchiveAutoNamingStyle::Uuid`] naming style:
///
/// ```no_run
/// # use daml_lf::DarFile;
/// # use daml_grpc::DamlGrpcClientBuilder;
/// # use std::thread;
/// # use daml_util::package::{DamlPackages, ArchiveAutoNamingStyle};
/// # fn main() {
/// # futures::executor::block_on(async {
/// let ledger_client = DamlGrpcClientBuilder::uri("http://127.0.0.1").connect().await.unwrap();
/// let packages = DamlPackages::from_ledger(&ledger_client).await.unwrap();
/// let dar = packages.into_dar(None, false, ArchiveAutoNamingStyle::Uuid).unwrap();
/// # })
/// # }
/// ```
#[derive(Debug)]
pub struct DamlPackages {
    packages: Vec<DamlPackage>,
}

impl DamlPackages {
    pub fn new(packages: Vec<DamlPackage>) -> Self {
        Self {
            packages,
        }
    }

    /// Create a [`DamlPackages`] from all known [`DamlPackage`] on a Daml ledger.
    pub async fn from_ledger(ledger_client: &DamlGrpcClient) -> DamlResult<Self> {
        let packages = ledger_client.package_service().list_packages().await?;
        let handles = packages
            .iter()
            .map(|pd| async move { ledger_client.package_service().get_package(pd).await })
            .collect::<FuturesUnordered<_>>();
        let all_packages =
            handles.collect::<Vec<DamlResult<_>>>().await.into_iter().collect::<DamlResult<Vec<DamlPackage>>>()?;
        Ok(Self::new(all_packages))
    }

    /// Return the hash of the [`DamlPackage`] which contains a given module or en error if no such package exists.
    ///
    /// The supplied `module_name` name is assumed to be in `DottedName` format, i.e. `TopModule.SubModule.Module`.
    pub async fn find_module(self, module_name: &str) -> DamlResult<String> {
        self.into_payloads()?
            .iter()
            .find(|(_, payload)| payload.contains_module(module_name))
            .map_or_else(|| Err("package could not be found".into()), |(package_id, _)| Ok((*package_id).to_string()))
    }

    /// Package all contained [`DamlPackage`] into a single [`DarFile`].
    ///
    /// * `main_package_id` — the id of the package to place as the DAR
    ///   main. `None` picks an arbitrary package from the set (whichever
    ///   the underlying `Vec` yields first). `Some(id)` errors out if
    ///   no contained package matches.
    /// * `filter_deps` — when `true`, walks the LF2 tree of the main
    ///   package to collect its (transitive) cross-package references
    ///   and keeps only those in the resulting DAR. When `false`, every
    ///   contained package becomes a dependency of the main, whether or
    ///   not it is actually reachable.
    /// * `auto_naming_style` — how to name each contained archive.
    pub fn into_dar(
        self,
        main_package_id: Option<&str>,
        filter_deps: bool,
        auto_naming_style: ArchiveAutoNamingStyle,
    ) -> DamlResult<DarFile> {
        let main_id = self.resolve_main_id(main_package_id)?;
        let keep = if filter_deps {
            let payloads: HashMap<String, DamlLfArchivePayload> = self.payloads_by_id()?;
            let reachable = Self::reachable_from(&main_id, &payloads)?;
            self.packages.into_iter().filter(|p| reachable.contains(p.hash())).collect()
        } else {
            self.packages
        };
        let all_archives = Self::packages_to_archives(keep, auto_naming_style)?;
        Self::archives_to_dar(all_archives, &main_id)
    }

    /// Convert all contained [`DamlPackage`] into [`DamlLfArchive`].
    ///
    /// Note that the created archive is not named.
    pub fn into_archives(self, auto_naming_style: ArchiveAutoNamingStyle) -> DamlResult<Vec<DamlLfArchive>> {
        Self::packages_to_archives(self.packages, auto_naming_style)
    }

    fn packages_to_archives(
        packages: Vec<DamlPackage>,
        auto_naming_style: ArchiveAutoNamingStyle,
    ) -> DamlResult<Vec<DamlLfArchive>> {
        packages
            .into_iter()
            .map(|p| {
                let hash = p.hash().to_owned();
                let payload = Self::package_into_payload(p)?;
                let name = match auto_naming_style {
                    ArchiveAutoNamingStyle::Empty => String::default(),
                    ArchiveAutoNamingStyle::Hash => hash.clone(),
                    ArchiveAutoNamingStyle::Uuid => Uuid::new_v4().to_string(),
                };
                Ok(DamlLfArchive::new(name, payload, DamlLfHashFunction::Sha256, hash))
            })
            .collect()
    }

    /// Convert all contained [`DamlPackage`] into [`DamlLfArchivePayload`].
    pub fn into_payloads(self) -> DamlResult<Vec<(String, DamlLfArchivePayload)>> {
        self.packages
            .into_iter()
            .map(|p| {
                let hash = p.hash().to_owned();
                Self::package_into_payload(p).map(|pl| (hash, pl))
            })
            .collect::<DamlResult<Vec<_>>>()
    }

    fn package_into_payload(package: DamlPackage) -> DamlResult<DamlLfArchivePayload> {
        DamlLfArchivePayload::from_bytes(package.take_payload()).map_err(|e| DamlError::Other(e.to_string()))
    }

    fn archives_to_dar(mut all_packages: Vec<DamlLfArchive>, main_id: &str) -> DamlResult<DarFile> {
        if all_packages.is_empty() {
            return Err("expected at least one archive".into());
        }
        let main_idx = all_packages
            .iter()
            .position(|a| a.hash == main_id)
            .ok_or_else(|| DamlError::Other(format!("main package {main_id} not present in archive set")))?;
        let first = all_packages.swap_remove(main_idx);
        let rest = all_packages;
        let manifest = DarManifest::new_implied(first.name.clone(), rest.iter().map(|n| n.name.clone()).collect());
        Ok(DarFile::new(manifest, first, rest))
    }

    /// Resolve the caller-supplied main-id hint. `None` picks whatever
    /// the underlying `Vec` yields first; `Some(id)` validates that a
    /// matching package is contained.
    fn resolve_main_id(&self, hint: Option<&str>) -> DamlResult<String> {
        match hint {
            Some(id) => {
                if self.packages.iter().any(|p| p.hash() == id) {
                    Ok(id.to_owned())
                } else {
                    Err(DamlError::Other(format!("main package {id} not present in package set")))
                }
            },
            None => self
                .packages
                .first()
                .map(|p| p.hash().to_owned())
                .ok_or_else(|| DamlError::Other("expected at least one package".to_owned())),
        }
    }

    fn payloads_by_id(&self) -> DamlResult<HashMap<String, DamlLfArchivePayload>> {
        self.packages
            .iter()
            .map(|p| {
                let hash = p.hash().to_owned();
                let payload = DamlLfArchivePayload::from_bytes(p.payload().to_vec())
                    .map_err(|e| DamlError::Other(e.to_string()))?;
                Ok((hash, payload))
            })
            .collect()
    }

    /// BFS from `root` through the payload map, collecting the set of
    /// package-ids that `root` transitively references. `root` itself
    /// is included in the returned set. Package-ids referenced from
    /// `root` that aren't in `payloads` are ignored — they may not be
    /// available to this participant, or the payload map may already
    /// be filtered.
    fn reachable_from(root: &str, payloads: &HashMap<String, DamlLfArchivePayload>) -> DamlResult<HashSet<String>> {
        let mut visited: HashSet<String> = HashSet::new();
        let mut queue: VecDeque<String> = VecDeque::new();
        queue.push_back(root.to_owned());
        while let Some(id) = queue.pop_front() {
            if !visited.insert(id.clone()) {
                continue;
            }
            let Some(payload) = payloads.get(&id) else {
                continue;
            };
            let refs = collect_referenced_package_ids(payload).map_err(|e| DamlError::Other(e.to_string()))?;
            for r in refs {
                if !visited.contains(&r) {
                    queue.push_back(r);
                }
            }
        }
        Ok(visited)
    }
}

/// Walk the decoded LF2 tree of `payload`, collecting every
/// cross-package reference: type-constructor uses (`Absolute` and
/// `NonLocal` variants) and value-name uses (`NonLocal` variant).
/// The set is the payload's cross-package reference footprint.
///
/// `Local` variants (same-package) are ignored, and self-references
/// via non-local names (source and target packages equal) are
/// filtered out too.
fn collect_referenced_package_ids(payload: &DamlLfArchivePayload) -> daml_lf::DamlLfResult<HashSet<String>> {
    payload.clone().apply(|package: &DamlLfPackage<'_>| {
        let mut visitor = ReferencedPackagesVisitor {
            self_package_id: package.package_id().to_owned(),
            referenced: HashSet::new(),
        };
        package.accept(&mut visitor);
        visitor.referenced
    })
}

struct ReferencedPackagesVisitor {
    self_package_id: String,
    referenced: HashSet<String>,
}

impl ReferencedPackagesVisitor {
    fn record(&mut self, pkg: &str) {
        if !pkg.is_empty() && pkg != self.self_package_id {
            self.referenced.insert(pkg.to_owned());
        }
    }
}

impl DamlElementVisitor for ReferencedPackagesVisitor {
    fn pre_visit_absolute_tycon<'a>(&mut self, abs: &'a DamlAbsoluteTyCon<'a>) {
        self.record(abs.package_id());
    }

    fn pre_visit_non_local_tycon<'a>(&mut self, non_local: &'a DamlNonLocalTyCon<'a>) {
        self.record(non_local.target_package_id());
    }

    fn pre_visit_non_local_value_name<'a>(&mut self, non_local: &'a DamlNonLocalValueName<'a>) {
        self.record(non_local.target_package_id());
    }
}

/// The automatic naming style to use when creating a `DamlLfArchive` from an unnamed `DamlPackage`.
#[derive(Clone, Copy, Debug)]
pub enum ArchiveAutoNamingStyle {
    /// Name the `DamlLfArchive` with an empty String.
    Empty,
    /// Name the `DamlLfArchive` with the archive hash.
    Hash,
    /// Name the `DamlLfArchive` with a `uuid`.
    Uuid,
}

/// Return the id of a package which contains a given module name or en error if no such package exists.
///
/// The supplied `module_name` name is assumed to be in `DottedName` format, i.e. `TopModule.SubModule.Module`.
pub async fn find_module_package_id(ledger_client: &DamlGrpcClient, module_name: &str) -> DamlResult<String> {
    DamlPackages::from_ledger(ledger_client).await?.find_module(module_name).await
}
