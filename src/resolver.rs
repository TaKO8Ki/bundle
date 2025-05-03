use std::collections::HashMap;

use pubgrub::{
    Dependencies, DependencyConstraints, DependencyProvider, OfflineDependencyProvider, Ranges,
    resolve,
};
use tracing::{Level, error, instrument};
// use pubgrub::SemanticVersion;
// use pubgrub::{Dependencies, DependencyProvider, OfflineDependencyProvider};
// use semver::{Version, VersionReq};
// use std::collections::HashMap;
// use std::fmt;
// use thiserror::Error;

use crate::version::{RichReq, RubyVersion};

pub struct Resolver {
    dependency_provider: OfflineDependencyProvider<String, RichReq>,
    lock_meta: HashMap<(String, RubyVersion), Vec<(String, Vec<String>)>>,
}

impl Resolver {
    pub fn new() -> Self {
        Resolver {
            dependency_provider: OfflineDependencyProvider::new(),
            lock_meta: HashMap::new(),
        }
    }

    #[instrument(level = Level::INFO, skip_all)]
    pub fn resolve(&self) -> anyhow::Result<HashMap<String, RubyVersion>> {
        let root_pkg = "root".to_string();
        let root_ver = RubyVersion::new(0, 0, 0);
        Ok(resolve(&self.dependency_provider, root_pkg, root_ver)?
            .into_iter()
            .collect())
    }

    #[instrument(level = Level::DEBUG, skip_all)]
    pub fn get_dependencies(
        &self,
        package: &String,
        version: &RubyVersion,
    ) -> Option<DependencyConstraints<String, RichReq>> {
        match self.dependency_provider.get_dependencies(package, version) {
            Ok(Dependencies::Available(deps)) => Some(deps),
            Ok(Dependencies::Unavailable(err)) => {
                error!("Package dependencies are unavailable: {err}");
                None
            }
            Err(err) => {
                error!("Failed to get dependencies: {err}");
                None
            }
        }
    }

    #[instrument(level = Level::DEBUG, skip_all)]
    pub fn get_dependencies_str(
        &self,
        package: &String,
        version: &RubyVersion,
    ) -> Option<&Vec<(String, Vec<String>)>> {
        self.lock_meta.get(&(package.clone(), version.clone()))
    }

    pub fn add_dependencies(
        &mut self,
        gem: String,
        version: RubyVersion,
        constraints: Vec<(String, RichReq, Vec<String>)>,
    ) {
        self.dependency_provider.add_dependencies(
            gem.clone(),
            version.clone(),
            constraints.iter().map(|c| (c.0.clone(), c.1.clone())),
        );
        self.lock_meta.entry((gem, version)).or_insert(
            constraints
                .iter()
                .map(|c| (c.0.clone(), c.2.clone()))
                .collect(),
        );
    }
}

// use crate::compact_index_client::{CompactIndexClient, GemDependency, GemVersion};
// use crate::gemfile_parser::GemDependency as GemfileDependency;

// #[derive(Error, Debug)]
// pub enum ResolverError {
//     #[error("Dependency resolution error: {0}")]
//     PubGrub(String),

//     #[error("Version parsing error: {0}")]
//     VersionParsing(#[from] semver::Error),

//     #[error("Compact index error: {0}")]
//     CompactIndex(#[from] crate::compact_index_client::CompactIndexError),

//     #[error("Other error: {0}")]
//     Other(String),
// }

// pub type Result<T> = std::result::Result<T, ResolverError>;

// // PubGrubのためのパッケージ型を定義
// #[derive(Debug, Clone, PartialEq, Eq, Ord, PartialOrd, Hash)]
// pub struct GemPackage(String);

// impl fmt::Display for GemPackage {
//     fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
//         write!(f, "{}", self.0)
//     }
// }

// // 依存関係プロバイダー実装
// pub struct GemDependencyProvider {
//     client: CompactIndexClient,
//     cache: HashMap<String, Vec<GemVersion>>,
// }

// impl GemDependencyProvider {
//     pub fn new(client: CompactIndexClient) -> Self {
//         Self {
//             client,
//             cache: HashMap::new(),
//         }
//     }

//     // gemのバージョンを取得（キャッシュ利用）
//     fn get_versions(&mut self, gem_name: &str) -> Result<Vec<GemVersion>> {
//         if let Some(versions) = self.cache.get(gem_name) {
//             return Ok(versions.to_vec());
//         }

//         let versions = self.client.info(gem_name)?;
//         self.cache.insert(gem_name.to_string(), versions.clone());

//         Ok(versions)
//     }

//     // バージョン文字列をSemanticVersionに変換
//     fn parse_version(&self, version_str: &str) -> Result<SemanticVersion> {
//         // セマンティックバージョンに変換可能な形式に正規化
//         let normalized = if version_str.split('.').count() < 3 {
//             format!("{}.0", version_str)
//         } else {
//             version_str.to_string()
//         };

//         let version = Version::parse(&normalized)?;
//         Ok(SemanticVersion::new(
//             version.major as u32,
//             version.minor as u32,
//             version.patch as u32,
//             // &version.pre.as_str(),
//         ))
//     }

//     // バージョン制約文字列をRangeに変換
//     fn parse_requirement(&self, req_str: &str) -> Result<Range<SemanticVersion>> {
//         let req = VersionReq::parse(req_str)?;

//         // 制約からRangeを構築
//         let mut range = Range::any();

//         for comparator in req.comparators {
//             let version = SemanticVersion::new(
//                 comparator.major as u32,
//                 comparator.minor.unwrap_or(0) as u32,
//                 comparator.patch.unwrap_or(0) as u32,
//                 // &comparator.pre.as_str(),
//             );

//             match comparator.op {
//                 semver::Op::Exact => {
//                     range = range.intersection(&Range::exact(version));
//                 }
//                 semver::Op::Greater => {
//                     range = range.intersection(&Range::strictly_higher_than(version));
//                 }
//                 semver::Op::GreaterEq => {
//                     range = range.intersection(&Range::higher_than(version));
//                 }
//                 semver::Op::Less => {
//                     range = range.intersection(&Range::strictly_lower_than(version));
//                 }
//                 semver::Op::LessEq => {
//                     range = range.intersection(&Range::lower_than(version));
//                 }
//                 semver::Op::Tilde | semver::Op::Caret => {
//                     // ~> と ^ 演算子用の特別処理
//                     let next_major = version.bump_major();

//                     range = range.intersection(&Range::between(
//                         version.clone(),
//                         next_major,
//                         // true,  // includeMin
//                         // false, // includeMax
//                     ));
//                 }
//                 _ => {
//                     return Err(ResolverError::Other(format!(
//                         "Unsupported version operator in: {}",
//                         req_str
//                     )));
//                 }
//             }
//         }

//         Ok(range)
//     }
// }

// impl DependencyProvider<GemPackage, SemanticVersion> for GemDependencyProvider {
//     fn get_dependencies(
//         &self,
//         package: &GemPackage,
//         version: &SemanticVersion,
//     ) -> std::result::Result<
//         Dependencies<GemPackage, pubgrub::Ranges<SemanticVersion>, Self::M>,
//         Box<dyn std::error::Error>,
//     > {
//         let gem_name = &package.0;
//         let version_str = version.to_string();

//         // キャッシュされていないケースを考慮
//         let versions = match self.get_versions(gem_name) {
//             Ok(v) => v,
//             Err(e) => {
//                 return Err(pubgrub::solver::Error::PackageNotFound(format!(
//                     "Failed to fetch info for {}: {}",
//                     gem_name, e
//                 )));
//             }
//         };

//         // バージョンを見つける
//         let gem_version = versions
//             .iter()
//             .find(|v| v.version == version_str)
//             .ok_or_else(|| {
//                 pubgrub::solver::Error::PackageNotFound(format!(
//                     "Version {} of gem {} not found",
//                     version_str, gem_name
//                 ))
//             })?;

//         // 依存関係を変換
//         let mut dependencies = Dependencies::empty();

//         for dep in &gem_version.dependencies {
//             let package = GemPackage(dep.name.clone());

//             match self.parse_requirement(&dep.requirement) {
//                 Ok(range) => {
//                     dependencies.insert(package, range);
//                 }
//                 Err(e) => {
//                     return Err(pubgrub::solver::Error::ErrorInDependencyRequirement(
//                         format!(
//                             "Error parsing requirement {} for {}: {}",
//                             dep.requirement, dep.name, e
//                         ),
//                     ));
//                 }
//             }
//         }

//         Ok(dependencies)
//     }

//     // fn get_versions(&self, package: &GemPackage) -> pubgrub::solver::Result<Vec<SemanticVersion>> {
//     //     let gem_name = &package.0;

//     //     let versions = match self.get_versions(gem_name) {
//     //         Ok(v) => v,
//     //         Err(e) => {
//     //             return Err(pubgrub::solver::Error::PackageNotFound(format!(
//     //                 "Failed to fetch versions for {}: {}",
//     //                 gem_name, e
//     //             )));
//     //         }
//     //     };

//     //     let semantic_versions = versions
//     //         .iter()
//     //         .filter_map(|v| self.parse_version(&v.version).ok())
//     //         .collect::<Vec<_>>();

//     //     if semantic_versions.is_empty() {
//     //         Err(pubgrub::solver::Error::PackageNotFound(format!(
//     //             "No valid versions found for {}",
//     //             gem_name
//     //         )))
//     //     } else {
//     //         Ok(semantic_versions)
//     //     }
//     // }
// }

// pub struct Resolver {
//     provider: GemDependencyProvider,
// }

// impl Resolver {
//     pub fn new(client: CompactIndexClient) -> Self {
//         Self {
//             provider: GemDependencyProvider::new(client),
//         }
//     }

//     pub fn resolve(
//         &mut self,
//         dependencies: &[GemfileDependency],
//     ) -> Result<HashMap<String, GemVersion>> {
//         let root = GemPackage("_root_".to_string());

//         // ルート依存関係を準備
//         let mut root_deps = Dependencies::empty();

//         for dep in dependencies {
//             let package = GemPackage(dep.name.clone());
//             let constraint = dep
//                 .version_constraint
//                 .clone()
//                 .unwrap_or_else(|| ">=0".to_string());

//             match self.provider.parse_requirement(&constraint) {
//                 Ok(range) => {
//                     root_deps.insert(package, range);
//                 }
//                 Err(e) => {
//                     return Err(ResolverError::Other(format!(
//                         "Failed to parse requirement '{}' for {}: {}",
//                         constraint, dep.name, e
//                     )));
//                 }
//             }
//         }

//         // オフラインプロバイダを準備
//         // let offline_provider =
//         // OfflineDependencyProvider::new(&self.provider, root.clone(), root_deps);

//         // PubGrubソルバを実行
//         let solution = match pubgrub::resolve(&self.provider, root.clone(), 1u32) {
//             Ok(solution) => solution,
//             Err(e) => {
//                 let report = match e {
//                     PubGrubError::NoSolution(tree) => DefaultStringReporter::report(&tree),
//                     _ => format!("{}", e),
//                 };

//                 return Err(ResolverError::PubGrub(report));
//             }
//         };

//         // 結果をGemVersionに変換
//         let mut result = HashMap::new();

//         for (package, version) in solution.iter() {
//             if package == &root {
//                 continue; // ルートパッケージはスキップ
//             }

//             let gem_name = package.0.clone();
//             let version_str = format!(
//                 "{}.{}.{}",
//                 version.major(),
//                 version.minor(),
//                 version.patch()
//             );

//             // 実際のGemVersionオブジェクトを探す
//             if let Ok(versions) = self.provider.get_versions(&gem_name) {
//                 if let Some(gem_version) = versions.iter().find(|v| v.version == version_str) {
//                     result.insert(gem_name, gem_version.clone());
//                 } else {
//                     return Err(ResolverError::Other(format!(
//                         "Resolved version {} of gem {} not found in available versions",
//                         version_str, gem_name
//                     )));
//                 }
//             } else {
//                 return Err(ResolverError::Other(format!(
//                     "Failed to fetch info for resolved gem {}",
//                     gem_name
//                 )));
//             }
//         }

//         Ok(result)
//     }
// }
