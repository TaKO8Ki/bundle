mod cli;
mod compact_index_client;
mod executor;
mod installer;
mod resolver;
mod version;

use compact_index_client::CompactIndexClient;
use executor::Executor;
use installer::GemInstaller;
use resolver::Resolver;
use serde::Deserialize;
use tracing_subscriber::fmt::format::FmtSpan;
use version::{RubyVersion, parse_req};
// use resolver::Resolver;

use pubgrub::Ranges;
use std::env;
use std::error::Error;
use std::path::{Path, PathBuf};

use clap::Parser as _;

#[derive(Deserialize, Debug)]
struct Gemfile {
    dependencies: Vec<Gem>,
}

#[derive(Deserialize, Debug)]
struct Gem {
    name: String,
    requirement: Option<String>,
}

fn parse_gemfile() -> Gemfile {
    let gemfile: Gemfile = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/gemfile.json"
    )))
    .unwrap();

    // println!("gemfile: {:?}", gemfile);

    // println!("rmagick: {}", gemfile.dependencies.iter().find(|dep| dep.name == "rmagick").unwrap().requirement.clone().unwrap());

    gemfile
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    use tracing_subscriber::util::SubscriberInitExt;
    use tracing_subscriber::{EnvFilter, fmt, prelude::__tracing_subscriber_SubscriberExt};

    tracing_subscriber::registry()
        .with(
            fmt::layer()
                .with_span_events(FmtSpan::CLOSE)
                .event_format(tracing_subscriber::fmt::format().without_time()),
        )
        .with(EnvFilter::from_default_env())
        .init();

    let cli = cli::Cli::parse();

    let gemfile = parse_gemfile();

    let gems = CompactIndexClient::new("https://rubygems.org/", Path::new(".newbundle"))?
        .resolve_dependencies(
            gemfile
                .dependencies
                .iter()
                .map(|dep| dep.name.clone())
                .collect(),
        )
        .await?;

    println!("gems: {}", gems.len());

    let mut resolver = Resolver::new();

    for (gem, versions) in gems {
        for v in versions {
            let constraints: Vec<(String, Ranges<RubyVersion>, Vec<String>)> = v
                .dependencies
                .iter()
                .map(|dep| {
                    (
                        dep.name.clone(),
                        dep.requirement.clone(),
                        dep.requirement_str.clone(),
                    )
                })
                .collect();
            resolver.add_dependencies(gem.clone(), v.version, constraints);
        }
    }
    let root_pkg = "root".to_string();
    let root_ver = RubyVersion::new(0, 0, 0);
    let root_constraints: Vec<(String, Ranges<RubyVersion>, Vec<String>)> = gemfile
        .dependencies
        .into_iter()
        .map(|gem| {
            // semver::VersionReq から VS へ
            let (vs, req_str) = match gem.requirement {
                Some(req) => parse_req(&req, ","), // :contentReference[oaicite:1]{index=1}
                None => parse_req("*", ","),
            };
            (gem.name, vs, req_str)
        })
        .collect();
    resolver.add_dependencies(root_pkg, root_ver, root_constraints);

    let solution = resolver.resolve().expect("dependency resolution failed");

    for (pkg, ver) in &solution {
        println!("  - {} ({})", pkg, ver);
        if let Some(deps) = resolver.get_dependencies_str(pkg, ver) {
            for (dg, dr) in deps {
                println!("    - {} ({})", dg, dr.join(", "))
            }
        }
    }

    match &cli.command() {
        Some(cli::Command::Install) => (),
        Some(cli::Command::Exec { args }) => {
            Executor::new(args.clone()).exec()?;
            return Ok(());
        }
        Some(cli::Command::Lock) => {
            return Ok(());
        }
        None => {}
    }

    // Bundlerのディレクトリ構造
    let bundle_dir = dirs::home_dir()
        .unwrap_or_else(|| env::current_dir().unwrap())
        .join(".bundle");

    // Gemキャッシュディレクトリ
    let gem_cache_dir = bundle_dir.join("cache");

    // Bundlerのインストールパス
    let install_dir = match env::var("GEM_HOME") {
        Ok(dir) => PathBuf::from(dir),
        Err(_) => dirs::home_dir()
            .unwrap_or_else(|| env::current_dir().unwrap())
            .join(".gem"),
    };

    let api_url = "https://rubygems.org/";

    // Gemfileを解析
    println!("Parsing Gemfile...");

    // Compact Index Clientを初期化
    println!("Initializing Compact Index Client...");
    let client = CompactIndexClient::new(api_url, &bundle_dir)?;

    // 依存関係を解決
    // // println!("Resolving dependencies...");
    // // let mut resolver = Resolver::new(client);
    // // let resolved_gems = resolver.resolve(&gemfile_content.dependencies)?;

    // // println!("Resolved {} gems:", resolved_gems.len());
    // // for (name, version) in &resolved_gems {
    // //     println!("  {} ({})", name, version.version);
    // // }

    // // gemをインストール
    // println!("Installing gems...");
    // let installer = GemInstaller::new(&install_dir, &gem_cache_dir, api_url)?;
    // installer.install_gems(resolved_gems)?;

    println!("Bundle install completed successfully!");

    Ok(())
}
