# Rust Ruby Bundler - Implementation Guide

## Overview

This guide provides step-by-step implementation details for building a Ruby bundler in Rust based on the proposed architecture. It includes code examples, project structure, dependencies, and best practices.

## Project Structure

```
rust-bundler/
├── Cargo.toml
├── src/
│   ├── lib.rs
│   ├── bundler.rs
│   ├── context.rs
│   ├── resolver/
│   │   ├── mod.rs
│   │   ├── strategy/
│   │   │   ├── mod.rs
│   │   │   ├── pubgrub.rs
│   │   │   └── molinillo.rs
│   │   └── result.rs
│   ├── installer/
│   │   ├── mod.rs
│   │   ├── download.rs
│   │   ├── build.rs
│   │   └── deploy.rs
│   ├── dependency/
│   │   ├── mod.rs
│   │   ├── provider.rs
│   │   └── specification.rs
│   ├── source/
│   │   ├── mod.rs
│   │   ├── remote.rs
│   │   ├── local.rs
│   │   └── git.rs
│   ├── cache/
│   │   ├── mod.rs
│   │   ├── memory.rs
│   │   ├── disk.rs
│   │   └── network.rs
│   ├── config/
│   │   ├── mod.rs
│   │   └── platform.rs
│   └── error.rs
├── tests/
│   ├── integration/
│   └── fixtures/
└── examples/
    └── basic_usage.rs
```

## Dependencies (Cargo.toml)

```toml
[package]
name = "rust-bundler"
version = "0.1.0"
edition = "2021"

[dependencies]
# Core dependencies
tokio = { version = "1.0", features = ["full"] }
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
thiserror = "1.0"
anyhow = "1.0"

# Version handling
semver = "1.0"

# HTTP client
reqwest = { version = "0.11", features = ["json", "stream"] }

# File operations
tempfile = "3.0"
tar = "0.4"
flate2 = "1.0"

# Git operations
git2 = "0.17"

# Platform detection
target-lexicon = "0.12"

# Caching and storage
sled = "0.34"  # Embedded database for disk cache
lru = "0.12"   # LRU cache for memory

# Process management
subprocess = "0.2"

# Configuration
toml = "0.8"

# Logging
tracing = "0.1"
tracing-subscriber = "0.3"

# Resolution algorithms
pubgrub = "0.2"

[dev-dependencies]
tokio-test = "0.4"
tempfile = "3.0"
```

## Core Implementation

### 1. Error Types (`src/error.rs`)

```rust
use thiserror::Error;

#[derive(Debug, Error)]
pub enum BundlerError {
    #[error("Resolution failed: {0}")]
    Resolution(#[from] ResolutionError),
    
    #[error("Installation failed: {0}")]
    Installation(#[from] InstallationError),
    
    #[error("Network error: {0}")]
    Network(#[from] NetworkError),
    
    #[error("Configuration error: {0}")]
    Config(#[from] ConfigError),
    
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    
    #[error("Parsing error: {0}")]
    Parsing(#[from] ParsingError),
}

#[derive(Debug, Error)]
pub enum ResolutionError {
    #[error("Conflicting dependencies: {conflicts:?}")]
    Conflicts { conflicts: Vec<String> },
    
    #[error("Missing dependency: {name} {version}")]
    MissingDependency { name: String, version: String },
    
    #[error("Version constraint unsatisfiable: {constraint}")]
    UnsatisfiableConstraint { constraint: String },
}

#[derive(Debug, Error)]
pub enum InstallationError {
    #[error("Build failed for {gem}: {reason}")]
    BuildFailed { gem: String, reason: String },
    
    #[error("Insufficient permissions: {path}")]
    Permission { path: String },
    
    #[error("Disk space insufficient")]
    DiskSpace,
    
    #[error("Dependency not found during installation: {name}")]
    DependencyNotFound { name: String },
}

#[derive(Debug, Error)]
pub enum NetworkError {
    #[error("Connection timeout")]
    Timeout,
    
    #[error("DNS resolution failed: {host}")]
    DnsResolution { host: String },
    
    #[error("HTTP error {status}: {message}")]
    Http { status: u16, message: String },
    
    #[error("SSL/TLS error: {message}")]
    Tls { message: String },
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("Invalid configuration: {field}")]
    InvalidField { field: String },
    
    #[error("Missing required configuration: {field}")]
    MissingField { field: String },
    
    #[error("Configuration file not found: {path}")]
    FileNotFound { path: String },
}

#[derive(Debug, Error)]
pub enum ParsingError {
    #[error("Gemfile syntax error at line {line}: {message}")]
    GemfileSyntax { line: usize, message: String },
    
    #[error("Version parsing error: {version}")]
    Version { version: String },
    
    #[error("Dependency specification error: {spec}")]
    DependencySpec { spec: String },
}

pub type Result<T> = std::result::Result<T, BundlerError>;
```

### 2. Specification and Dependency Core (`src/dependency/mod.rs`)

```rust
use semver::Version;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub mod provider;
pub mod specification;

pub use provider::*;
pub use specification::*;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Dependency {
    pub name: String,
    pub version_req: String,
    pub dep_type: DependencyType,
    pub platforms: Vec<String>,
    pub source: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DependencyType {
    Runtime,
    Development,
    Optional,
}

impl Dependency {
    pub fn new(name: impl Into<String>, version_req: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            version_req: version_req.into(),
            dep_type: DependencyType::Runtime,
            platforms: vec![],
            source: None,
        }
    }
    
    pub fn development(mut self) -> Self {
        self.dep_type = DependencyType::Development;
        self
    }
    
    pub fn platform(mut self, platform: impl Into<String>) -> Self {
        self.platforms.push(platform.into());
        self
    }
    
    pub fn source(mut self, source: impl Into<String>) -> Self {
        self.source = Some(source.into());
        self
    }
    
    pub fn matches_version(&self, version: &Version) -> bool {
        semver::VersionReq::parse(&self.version_req)
            .map(|req| req.matches(version))
            .unwrap_or(false)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpecificationInfo {
    pub name: String,
    pub version: Version,
    pub dependencies: Vec<Dependency>,
    pub platform: String,
    pub checksum: Option<String>,
    pub download_url: Option<String>,
    pub metadata: HashMap<String, serde_json::Value>,
}

impl SpecificationInfo {
    pub fn platform_compatible(&self, target_platform: &str) -> bool {
        self.platform == "ruby" || self.platform == target_platform
    }
}
```

### 3. Specification Trait (`src/dependency/specification.rs`)

```rust
use super::{Dependency, SpecificationInfo};
use crate::config::Platform;
use semver::Version;

pub trait Specification {
    fn name(&self) -> &str;
    fn version(&self) -> &Version;
    fn dependencies(&self) -> &[Dependency];
    fn platform_compatible(&self, platform: &Platform) -> bool;
    fn checksum(&self) -> Option<&str>;
    fn download_url(&self) -> Option<&str>;
}

#[derive(Debug, Clone)]
pub struct GemSpecification {
    info: SpecificationInfo,
}

impl GemSpecification {
    pub fn new(info: SpecificationInfo) -> Self {
        Self { info }
    }
    
    pub fn info(&self) -> &SpecificationInfo {
        &self.info
    }
}

impl Specification for GemSpecification {
    fn name(&self) -> &str {
        &self.info.name
    }
    
    fn version(&self) -> &Version {
        &self.info.version
    }
    
    fn dependencies(&self) -> &[Dependency] {
        &self.info.dependencies
    }
    
    fn platform_compatible(&self, platform: &Platform) -> bool {
        self.info.platform_compatible(&platform.to_string())
    }
    
    fn checksum(&self) -> Option<&str> {
        self.info.checksum.as_deref()
    }
    
    fn download_url(&self) -> Option<&str> {
        self.info.download_url.as_deref()
    }
}
```

### 4. Dependency Provider (`src/dependency/provider.rs`)

```rust
use super::{Dependency, Specification};
use crate::error::Result;
use std::sync::Arc;

pub trait DependencyProvider: Send + Sync {
    fn search_for(&self, dependency: &Dependency) -> Result<Vec<Box<dyn Specification>>>;
    fn name_for(&self, specification: &dyn Specification) -> String;
    fn dependencies_for(&self, specification: &dyn Specification) -> Vec<Dependency>;
    fn supports_platform(&self, platform: &str) -> bool;
}

pub struct CompositeDependencyProvider {
    providers: Vec<Box<dyn DependencyProvider>>,
}

impl CompositeDependencyProvider {
    pub fn new(providers: Vec<Box<dyn DependencyProvider>>) -> Self {
        Self { providers }
    }
    
    pub fn add_provider(&mut self, provider: Box<dyn DependencyProvider>) {
        self.providers.push(provider);
    }
}

impl DependencyProvider for CompositeDependencyProvider {
    fn search_for(&self, dependency: &Dependency) -> Result<Vec<Box<dyn Specification>>> {
        let mut results = Vec::new();
        
        // Try providers in priority order (local -> git -> remote)
        for provider in &self.providers {
            match provider.search_for(dependency) {
                Ok(mut specs) => {
                    results.append(&mut specs);
                    // If we found specs from a higher priority provider, prefer those
                    if !results.is_empty() {
                        break;
                    }
                }
                Err(_) => continue, // Try next provider
            }
        }
        
        Ok(results)
    }
    
    fn name_for(&self, specification: &dyn Specification) -> String {
        specification.name().to_string()
    }
    
    fn dependencies_for(&self, specification: &dyn Specification) -> Vec<Dependency> {
        specification.dependencies().to_vec()
    }
    
    fn supports_platform(&self, platform: &str) -> bool {
        self.providers.iter().any(|p| p.supports_platform(platform))
    }
}
```

### 5. Bundler Context (`src/context.rs`)

```rust
use crate::{
    cache::SpecificationCache,
    config::{BundlerConfig, Platform},
    dependency::CompositeDependencyProvider,
    source::SpecificationSource,
    error::Result,
};
use std::sync::{Arc, RwLock};

pub struct BundlerContext {
    pub sources: Vec<Box<dyn SpecificationSource>>,
    pub cache: Arc<RwLock<SpecificationCache>>,
    pub config: BundlerConfig,
    pub platform: Platform,
}

impl BundlerContext {
    pub fn new(config: BundlerConfig) -> Result<Self> {
        let platform = Platform::detect()?;
        let cache = Arc::new(RwLock::new(SpecificationCache::new(&config.cache_dir)?));
        let sources = Self::initialize_sources(&config)?;
        
        Ok(Self {
            sources,
            cache,
            config,
            platform,
        })
    }
    
    pub fn dependency_provider(&self) -> CompositeDependencyProvider {
        let providers: Vec<Box<dyn crate::dependency::DependencyProvider>> = self
            .sources
            .iter()
            .map(|source| source.as_dependency_provider())
            .collect();
            
        CompositeDependencyProvider::new(providers)
    }
    
    fn initialize_sources(config: &BundlerConfig) -> Result<Vec<Box<dyn SpecificationSource>>> {
        let mut sources: Vec<Box<dyn SpecificationSource>> = Vec::new();
        
        // Add local source (highest priority)
        if let Some(local_path) = &config.local_gem_path {
            sources.push(Box::new(crate::source::LocalSource::new(local_path.clone())?));
        }
        
        // Add git sources
        for git_config in &config.git_sources {
            sources.push(Box::new(crate::source::GitSource::new(git_config.clone())?));
        }
        
        // Add remote sources (lowest priority)
        for remote_url in &config.remote_sources {
            sources.push(Box::new(crate::source::RemoteSource::new(remote_url.clone())?));
        }
        
        Ok(sources)
    }
}
```

### 6. Cache Implementation (`src/cache/mod.rs`)

```rust
use crate::{dependency::SpecificationInfo, error::Result};
use lru::LruCache;
use sled::Db;
use std::{
    collections::HashMap,
    path::Path,
    time::{Duration, Instant},
};

pub mod memory;
pub mod disk;
pub mod network;

pub struct SpecificationCache {
    memory: LruCache<String, Vec<SpecificationInfo>>,
    disk: Db,
    network_cache: HashMap<String, (Vec<SpecificationInfo>, Instant)>,
    cache_ttl: Duration,
}

impl SpecificationCache {
    pub fn new(cache_dir: &Path) -> Result<Self> {
        std::fs::create_dir_all(cache_dir)?;
        let disk_path = cache_dir.join("specs.db");
        
        Ok(Self {
            memory: LruCache::new(std::num::NonZeroUsize::new(1000).unwrap()),
            disk: sled::open(&disk_path)?,
            network_cache: HashMap::new(),
            cache_ttl: Duration::from_secs(3600), // 1 hour TTL
        })
    }
    
    pub fn get(&mut self, key: &str) -> Option<Vec<SpecificationInfo>> {
        // Check memory cache first
        if let Some(specs) = self.memory.get(key) {
            return Some(specs.clone());
        }
        
        // Check network cache with TTL
        if let Some((specs, timestamp)) = self.network_cache.get(key) {
            if timestamp.elapsed() < self.cache_ttl {
                // Update memory cache
                self.memory.put(key.to_string(), specs.clone());
                return Some(specs.clone());
            } else {
                // Remove expired entry
                self.network_cache.remove(key);
            }
        }
        
        // Check disk cache
        if let Ok(Some(data)) = self.disk.get(key) {
            if let Ok(specs) = bincode::deserialize::<Vec<SpecificationInfo>>(&data) {
                // Update memory cache
                self.memory.put(key.to_string(), specs.clone());
                return Some(specs);
            }
        }
        
        None
    }
    
    pub fn put(&mut self, key: String, specs: Vec<SpecificationInfo>) -> Result<()> {
        // Update memory cache
        self.memory.put(key.clone(), specs.clone());
        
        // Update network cache with timestamp
        self.network_cache.insert(key.clone(), (specs.clone(), Instant::now()));
        
        // Update disk cache
        let data = bincode::serialize(&specs)?;
        self.disk.insert(key, data)?;
        self.disk.flush()?;
        
        Ok(())
    }
    
    pub fn invalidate(&mut self, key: &str) {
        self.memory.pop(key);
        self.network_cache.remove(key);
        let _ = self.disk.remove(key);
    }
    
    pub fn clear(&mut self) -> Result<()> {
        self.memory.clear();
        self.network_cache.clear();
        self.disk.clear()?;
        Ok(())
    }
}
```

### 7. Resolver Implementation (`src/resolver/mod.rs`)

```rust
use crate::{
    context::BundlerContext,
    dependency::{Dependency, DependencyProvider},
    error::{BundlerError, ResolutionError, Result},
};
use std::sync::Arc;

pub mod strategy;
pub mod result;

pub use result::ResolutionResult;
pub use strategy::{ResolverStrategy, PubGrubStrategy, MolinilloStrategy};

pub struct Resolver {
    context: Arc<BundlerContext>,
    strategy: Box<dyn ResolverStrategy>,
}

impl Resolver {
    pub fn new(context: Arc<BundlerContext>) -> Self {
        // Choose strategy based on configuration or complexity
        let strategy: Box<dyn ResolverStrategy> = if context.config.use_pubgrub {
            Box::new(PubGrubStrategy::new())
        } else {
            Box::new(MolinilloStrategy::new())
        };
        
        Self { context, strategy }
    }
    
    pub fn with_strategy(
        context: Arc<BundlerContext>,
        strategy: Box<dyn ResolverStrategy>,
    ) -> Self {
        Self { context, strategy }
    }
    
    pub fn resolve(&self, requirements: &[Dependency]) -> Result<ResolutionResult> {
        let provider = self.context.dependency_provider();
        
        // Pre-resolution validation
        self.validate_requirements(requirements)?;
        
        // Perform resolution
        let result = self.strategy.resolve(requirements, &provider)?;
        
        // Post-resolution validation
        self.validate_result(&result)?;
        
        Ok(result)
    }
    
    pub fn resolve_with_lock(
        &self,
        requirements: &[Dependency],
        _lock: &crate::lockfile::Lockfile,
    ) -> Result<ResolutionResult> {
        // TODO: Implement lock file constraint application
        self.resolve(requirements)
    }
    
    fn validate_requirements(&self, requirements: &[Dependency]) -> Result<()> {
        // Check for obviously conflicting requirements
        for req in requirements {
            if req.version_req.is_empty() {
                return Err(BundlerError::Resolution(
                    ResolutionError::UnsatisfiableConstraint {
                        constraint: format!("Empty version requirement for {}", req.name),
                    },
                ));
            }
        }
        
        Ok(())
    }
    
    fn validate_result(&self, result: &ResolutionResult) -> Result<()> {
        // Ensure all requirements are satisfied
        if result.resolved_specs.is_empty() {
            return Err(BundlerError::Resolution(
                ResolutionError::MissingDependency {
                    name: "No gems resolved".to_string(),
                    version: "any".to_string(),
                },
            ));
        }
        
        Ok(())
    }
}
```

### 8. Resolution Result (`src/resolver/result.rs`)

```rust
use crate::dependency::SpecificationInfo;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolutionResult {
    pub resolved_specs: Vec<SpecificationInfo>,
    pub platform_specs: HashMap<String, Vec<SpecificationInfo>>,
    pub resolution_time: std::time::Duration,
    pub dependency_graph: DependencyGraph,
}

impl ResolutionResult {
    pub fn new(
        resolved_specs: Vec<SpecificationInfo>,
        resolution_time: std::time::Duration,
    ) -> Self {
        let dependency_graph = DependencyGraph::from_specs(&resolved_specs);
        
        Self {
            resolved_specs,
            platform_specs: HashMap::new(),
            resolution_time,
            dependency_graph,
        }
    }
    
    pub fn installation_order(&self) -> Vec<&SpecificationInfo> {
        self.dependency_graph.topological_sort(&self.resolved_specs)
    }
    
    pub fn find_spec(&self, name: &str) -> Option<&SpecificationInfo> {
        self.resolved_specs.iter().find(|spec| spec.name == name)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DependencyGraph {
    edges: HashMap<String, Vec<String>>,
}

impl DependencyGraph {
    pub fn from_specs(specs: &[SpecificationInfo]) -> Self {
        let mut edges = HashMap::new();
        
        for spec in specs {
            let deps: Vec<String> = spec
                .dependencies
                .iter()
                .map(|dep| dep.name.clone())
                .collect();
            edges.insert(spec.name.clone(), deps);
        }
        
        Self { edges }
    }
    
    pub fn topological_sort<'a>(
        &self,
        specs: &'a [SpecificationInfo],
    ) -> Vec<&'a SpecificationInfo> {
        // Simple topological sort implementation
        let mut result = Vec::new();
        let mut visited = std::collections::HashSet::new();
        let mut temp_visited = std::collections::HashSet::new();
        
        fn visit<'a>(
            spec_name: &str,
            edges: &HashMap<String, Vec<String>>,
            specs: &'a [SpecificationInfo],
            visited: &mut std::collections::HashSet<String>,
            temp_visited: &mut std::collections::HashSet<String>,
            result: &mut Vec<&'a SpecificationInfo>,
        ) {
            if temp_visited.contains(spec_name) || visited.contains(spec_name) {
                return;
            }
            
            temp_visited.insert(spec_name.to_string());
            
            if let Some(deps) = edges.get(spec_name) {
                for dep in deps {
                    visit(dep, edges, specs, visited, temp_visited, result);
                }
            }
            
            temp_visited.remove(spec_name);
            visited.insert(spec_name.to_string());
            
            if let Some(spec) = specs.iter().find(|s| s.name == spec_name) {
                result.push(spec);
            }
        }
        
        for spec in specs {
            if !visited.contains(&spec.name) {
                visit(
                    &spec.name,
                    &self.edges,
                    specs,
                    &mut visited,
                    &mut temp_visited,
                    &mut result,
                );
            }
        }
        
        result
    }
}
```

### 9. Main Bundler (`src/bundler.rs`)

```rust
use crate::{
    context::BundlerContext,
    config::BundlerConfig,
    dependency::Dependency,
    error::Result,
    installer::Installer,
    resolver::Resolver,
};
use std::sync::Arc;

pub struct Bundler {
    context: Arc<BundlerContext>,
    resolver: Resolver,
    installer: Installer,
}

impl Bundler {
    pub fn new(config: BundlerConfig) -> Result<Self> {
        let context = Arc::new(BundlerContext::new(config)?);
        let resolver = Resolver::new(context.clone());
        let installer = Installer::new(context.clone());
        
        Ok(Self {
            context,
            resolver,
            installer,
        })
    }
    
    pub async fn resolve_and_install(
        &self,
        requirements: Vec<Dependency>,
        lockfile: Option<&crate::lockfile::Lockfile>,
    ) -> Result<()> {
        // Resolution phase
        tracing::info!("Starting dependency resolution");
        let resolution = if let Some(lock) = lockfile {
            self.resolver.resolve_with_lock(&requirements, lock)?
        } else {
            self.resolver.resolve(&requirements)?
        };
        
        tracing::info!(
            "Resolution completed in {:?}, {} gems resolved",
            resolution.resolution_time,
            resolution.resolved_specs.len()
        );
        
        // Installation phase
        tracing::info!("Starting installation");
        let install_result = self.installer.install(&resolution).await?;
        
        tracing::info!(
            "Installation completed: {} gems installed, {} updated",
            install_result.installed_count,
            install_result.updated_count
        );
        
        Ok(())
    }
    
    pub fn context(&self) -> &Arc<BundlerContext> {
        &self.context
    }
}
```

## Usage Example

### Basic Usage (`examples/basic_usage.rs`)

```rust
use rust_bundler::{
    Bundler,
    config::BundlerConfig,
    dependency::Dependency,
    error::Result,
};

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::init();
    
    // Create configuration
    let config = BundlerConfig::builder()
        .cache_dir("./cache".into())
        .add_remote_source("https://rubygems.org".to_string())
        .use_pubgrub(true)
        .build();
    
    // Create bundler
    let bundler = Bundler::new(config)?;
    
    // Define requirements
    let requirements = vec![
        Dependency::new("rails", "~> 7.0"),
        Dependency::new("pg", ">= 1.0").platform("ruby"),
        Dependency::new("puma", "~> 5.0"),
        Dependency::new("rspec", "~> 3.0").development(),
    ];
    
    // Resolve and install
    bundler.resolve_and_install(requirements, None).await?;
    
    println!("Bundler operation completed successfully!");
    Ok(())
}
```

## Configuration (`src/config/mod.rs`)

```rust
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub mod platform;
pub use platform::Platform;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BundlerConfig {
    pub cache_dir: PathBuf,
    pub remote_sources: Vec<String>,
    pub git_sources: Vec<GitSourceConfig>,
    pub local_gem_path: Option<PathBuf>,
    pub use_pubgrub: bool,
    pub parallel_downloads: usize,
    pub build_timeout: std::time::Duration,
    pub network_timeout: std::time::Duration,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitSourceConfig {
    pub url: String,
    pub branch: Option<String>,
    pub tag: Option<String>,
    pub rev: Option<String>,
}

impl Default for BundlerConfig {
    fn default() -> Self {
        Self {
            cache_dir: dirs::cache_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join("rust-bundler"),
            remote_sources: vec!["https://rubygems.org".to_string()],
            git_sources: vec![],
            local_gem_path: None,
            use_pubgrub: false,
            parallel_downloads: 8,
            build_timeout: std::time::Duration::from_secs(300),
            network_timeout: std::time::Duration::from_secs(30),
        }
    }
}

impl BundlerConfig {
    pub fn builder() -> BundlerConfigBuilder {
        BundlerConfigBuilder::default()
    }
}

pub struct BundlerConfigBuilder {
    config: BundlerConfig,
}

impl Default for BundlerConfigBuilder {
    fn default() -> Self {
        Self {
            config: BundlerConfig::default(),
        }
    }
}

impl BundlerConfigBuilder {
    pub fn cache_dir(mut self, path: PathBuf) -> Self {
        self.config.cache_dir = path;
        self
    }
    
    pub fn add_remote_source(mut self, url: String) -> Self {
        self.config.remote_sources.push(url);
        self
    }
    
    pub fn use_pubgrub(mut self, use_pubgrub: bool) -> Self {
        self.config.use_pubgrub = use_pubgrub;
        self
    }
    
    pub fn parallel_downloads(mut self, count: usize) -> Self {
        self.config.parallel_downloads = count;
        self
    }
    
    pub fn build(self) -> BundlerConfig {
        self.config
    }
}
```

## Testing Strategy

### Integration Tests (`tests/integration/basic.rs`)

```rust
use rust_bundler::{Bundler, config::BundlerConfig, dependency::Dependency};
use tempfile::TempDir;

#[tokio::test]
async fn test_basic_resolution() {
    let temp_dir = TempDir::new().unwrap();
    
    let config = BundlerConfig::builder()
        .cache_dir(temp_dir.path().to_path_buf())
        .build();
    
    let bundler = Bundler::new(config).unwrap();
    
    let requirements = vec![
        Dependency::new("json", "~> 2.0"),
    ];
    
    let result = bundler.resolve_and_install(requirements, None).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_conflict_resolution() {
    let temp_dir = TempDir::new().unwrap();
    
    let config = BundlerConfig::builder()
        .cache_dir(temp_dir.path().to_path_buf())
        .use_pubgrub(true)
        .build();
    
    let bundler = Bundler::new(config).unwrap();
    
    let requirements = vec![
        Dependency::new("rails", "= 6.0.0"),
        Dependency::new("some-gem-requiring-rails-7", ">= 1.0"),
    ];
    
    let result = bundler.resolve_and_install(requirements, None).await;
    // Should handle conflicts gracefully
    assert!(result.is_err());
}
```

## Next Steps

1. **Implement Source Providers**: Complete remote, local, and git source implementations
2. **Build System Integration**: Add support for compiling native extensions
3. **Lockfile Support**: Implement Gemfile.lock reading/writing
4. **CLI Interface**: Create command-line interface
5. **Performance Optimization**: Add benchmarking and optimization
6. **Ruby Integration**: Create Ruby bindings for compatibility

This implementation guide provides a solid foundation for building a robust Ruby bundler in Rust with proper architecture, error handling, and testing strategy.