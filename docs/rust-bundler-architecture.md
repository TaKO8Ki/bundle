# Rust Ruby Bundler Architecture

## Overview

This document outlines the proposed architecture for a Ruby bundler written in Rust, based on analysis of the RubyGems repository. The design emphasizes shared components, efficient dependency resolution, and clean separation of concerns while maintaining compatibility with existing Ruby gem ecosystem.

## Key Design Principles

1. **Shared State Management**: Use `Arc<BundlerContext>` to share expensive resources like caches and configuration
2. **Trait-Based Design**: Abstract core functionality through traits for flexibility and testability  
3. **Strategy Pattern**: Support multiple resolution algorithms (PubGrub, Molinillo)
4. **Unified Caching**: Single cache layer shared between resolver and installer
5. **Composable Sources**: Support for multiple gem sources (remote, local, git)

## Overall Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                        Bundler                                  │
├─────────────────────────────────────────────────────────────────┤
│  ┌─────────────┐    ┌─────────────┐    ┌─────────────┐          │
│  │  Resolver   │    │  Installer  │    │   Config    │          │
│  │             │    │             │    │             │          │
│  └─────────────┘    └─────────────┘    └─────────────┘          │
│         │                   │                   │               │
│         └───────────────────┼───────────────────┘               │
│                             │                                   │
│  ┌──────────────────────────▼──────────────────────────────┐    │
│  │                BundlerContext (Arc)                     │    │
│  │  ┌─────────────┐ ┌─────────────┐ ┌─────────────┐        │    │
│  │  │   Sources   │ │    Cache    │ │  Platform   │        │    │
│  │  └─────────────┘ └─────────────┘ └─────────────┘        │    │
│  └─────────────────────────────────────────────────────────┘    │
└─────────────────────────────────────────────────────────────────┘
```

## Core Components

### BundlerContext - Shared State

The `BundlerContext` serves as the central hub for shared resources:

```rust
pub struct BundlerContext {
    pub sources: Vec<Box<dyn SpecificationSource>>,
    pub cache: Arc<RwLock<SpecificationCache>>,
    pub config: BundlerConfig,
    pub platform: Platform,
}
```

**Benefits:**
- Single source of truth for configuration
- Shared caching between resolver and installer
- Thread-safe access to expensive resources
- Consistent platform and source handling

### Dependency Provider Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                  DependencyProvider Trait                       │
└─────────────────────┬───────────────────────────────────────────┘
                      │
      ┌───────────────┼───────────────┐
      │               │               │
┌─────▼─────┐  ┌──────▼──────┐  ┌─────▼─────┐
│  Remote   │  │    Local    │  │    Git    │
│ Provider  │  │  Provider   │  │ Provider  │
└───────────┘  └─────────────┘  └───────────┘
      │               │               │
      └───────────────┼───────────────┘
                      │
      ┌───────────────▼───────────────┐
      │   CompositeDependencyProvider │
      │                               │
      │  - Combines multiple sources  │
      │  - Prioritizes local over     │
      │    remote                     │
      │  - Shared by Resolver &       │
      │    Installer                  │
      └───────────────────────────────┘
```

The dependency provider abstraction allows both resolver and installer to access gem specifications consistently:

```rust
pub trait DependencyProvider {
    fn search_for(&self, dependency: &Dependency) -> Vec<Box<dyn Specification>>;
    fn name_for(&self, specification: &dyn Specification) -> String;
    fn dependencies_for(&self, specification: &dyn Specification) -> Vec<Dependency>;
}
```

### Resolution System

```rust
pub struct Resolver {
    context: Arc<BundlerContext>,
    strategy: Box<dyn ResolverStrategy>,
}

pub trait ResolverStrategy {
    fn resolve(&self, requirements: &[Dependency], provider: &dyn DependencyProvider) 
        -> Result<ResolutionResult>;
}
```

**Supported Strategies:**
- **PubGrubStrategy**: Modern conflict resolution algorithm
- **MolinilloStrategy**: Ruby-compatible resolution algorithm

### Installation System  

```rust
pub struct Installer {
    context: Arc<BundlerContext>,
    download_manager: DownloadManager,
    build_system: BuildSystem,
}
```

The installer uses the same dependency provider as the resolver, ensuring consistency between what gets resolved and what gets installed.

## Data Flow

### Complete Resolution and Installation Flow

```
  Gemfile          Gemfile.lock
     │                  │
     ▼                  ▼
┌─────────────────────────────┐
│       Requirements          │
│    (Dependencies)           │
└─────────────┬───────────────┘
              │
              ▼
┌─────────────────────────────┐
│        Resolver             │
│                             │
│ ┌─────────────────────────┐ │    ┌─────────────────────────┐
│ │   Strategy Pattern      │ │    │   Dependency Provider   │
│ │  ┌─────────┐ ┌────────┐ │ │◀── │                         │
│ │  │PubGrub  │ │Molinillo││ │    │ ┌─────────┐ ┌─────────┐ │
│ │  │Strategy │ │Strategy ││ │    │ │ Remote  │ │  Local  │ │
│ │  └─────────┘ └────────┘ │ │    │ │Provider │ │Provider │ │
│ └─────────────────────────┘ │    │ └─────────┘ └─────────┘ │
└─────────────┬───────────────┘    └─────────────────────────┘
              │                                  ▲
              ▼                                  │
┌─────────────────────────────┐                  │
│    Resolution Result        │                  │
│  (Selected Specifications)  │                  │
└─────────────┬───────────────┘                  │
              │                                  │
              ▼                                  │
┌─────────────────────────────┐                  │
│       Installer             │                  │
│                             │                  │
│ ┌─────────────────────────┐ │──────────────────┘
│ │     Install Plan        │ │
│ │  (Ordered Dependencies) │ │
│ └─────────────────────────┘ │
│                             │
│ ┌─────────────────────────┐ │
│ │   Download Manager      │ │
│ └─────────────────────────┘ │
│                             │
│ ┌─────────────────────────┐ │
│ │    Build System         │ │
│ └─────────────────────────┘ │
└─────────────┬───────────────┘
              │
              ▼
     Updated Gemfile.lock
```

## Key Architectural Benefits

### 1. Shared Context Pattern
- **Problem**: Resolver and installer need access to same sources, cache, and configuration
- **Solution**: `Arc<BundlerContext>` provides shared access to expensive resources
- **Benefit**: No duplication of state, consistent behavior

### 2. Trait-Based Abstraction
- **Problem**: Need flexibility for different source types and resolution strategies  
- **Solution**: Core traits (`DependencyProvider`, `SpecificationSource`, `ResolverStrategy`)
- **Benefit**: Easy testing, pluggable implementations

### 3. Unified Caching
- **Problem**: Both resolver and installer need to cache specifications and downloads
- **Solution**: Single `SpecificationCache` in shared context
- **Benefit**: Reduced network calls, consistent cache behavior

### 4. Strategy Pattern for Resolution
- **Problem**: Need to support different resolution algorithms
- **Solution**: `ResolverStrategy` trait with multiple implementations
- **Benefit**: Algorithm flexibility, A/B testing capability

### 5. Composable Dependency Providers
- **Problem**: Need to search across multiple gem sources with priorities
- **Solution**: `CompositeDependencyProvider` combines multiple sources
- **Benefit**: Flexible source configuration, local override support

## Thread Safety and Performance

### Concurrent Operations
```rust
// Cache is thread-safe for concurrent reads
pub struct SpecificationCache {
    inner: Arc<RwLock<CacheInner>>,
}

// Context can be safely shared across threads
let context = Arc::new(BundlerContext::new(config)?);
let resolver = Resolver::new(context.clone());
let installer = Installer::new(context.clone());
```

### Performance Optimizations
- **Lazy Loading**: Specifications loaded on-demand
- **Parallel Downloads**: Multiple gems downloaded concurrently  
- **Incremental Resolution**: Build on existing lock files when possible
- **Smart Caching**: Cache invalidation based on timestamps and checksums

## Error Handling Strategy

```rust
// Structured error types for different failure modes
#[derive(Debug, thiserror::Error)]
pub enum BundlerError {
    #[error("Resolution failed: {0}")]
    Resolution(#[from] ResolutionError),
    
    #[error("Installation failed: {0}")]
    Installation(#[from] InstallationError),
    
    #[error("Network error: {0}")]
    Network(#[from] NetworkError),
    
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}
```

## Integration Points

### Ruby Compatibility
- **Gemfile Parsing**: Compatible with Ruby Gemfile syntax
- **Lock File Format**: Maintains Gemfile.lock compatibility
- **Extension Building**: Supports native extensions (C, Rust, etc.)
- **Platform Handling**: Ruby platform string compatibility

### Ecosystem Integration
- **RubyGems.org**: Full API compatibility
- **Git Sources**: Support for git-based gems
- **Local Sources**: Path-based gem sources
- **Private Registries**: Custom gem server support

This architecture provides a solid foundation for building a fast, reliable Ruby bundler in Rust while maintaining compatibility with the existing Ruby ecosystem.
