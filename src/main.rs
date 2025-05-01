mod compact_index_client;
mod executor;
mod installer;
mod resolver;
mod version;

use compact_index_client::CompactIndexClient;
use executor::Executor;
use installer::GemInstaller;
use serde::Deserialize;
use tracing_subscriber::fmt::format::FmtSpan;
// use resolver::Resolver;

use std::env;
use std::error::Error;
use std::path::{Path, PathBuf};

use clap::Parser as _;

#[derive(clap::Parser)]
#[command(
    name = "Bundler",
    version = "0.1.0",
    about = "Example CLI",
    subcommand_required = true,
    arg_required_else_help = true
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(clap::Subcommand)]
enum Commands {
    Install,
    #[clap(trailing_var_arg = true, allow_hyphen_values = true)]
    Exec {
        args: Vec<String>,
    },
    Lock,
}

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

    // setup tracing
    tracing_subscriber::registry()
        .with(fmt::layer().with_span_events(FmtSpan::CLOSE).event_format(
            tracing_subscriber::fmt::format().without_time(), // タイムスタンプを表示しない :contentReference[oaicite:0]{index=0}
        ))
        .with(EnvFilter::from_default_env())
        // .with_span_events(FmtSpan::CLOSE)
        // ↓ 以下でタイムスタンプを消します
        // .event_format(
        //     tracing_subscriber::fmt::format().without_time(), // タイムスタンプを表示しない :contentReference[oaicite:0]{index=0}
        // )
        .init();

    let cli = Cli::parse();

    let gemfile = parse_gemfile();

    CompactIndexClient::new("https://rubygems.org/", Path::new(".newbundle"))?
        .resolve_dependencies(
            gemfile
                .dependencies
                .iter()
                .map(|dep| dep.name.clone())
                .collect(),
        )
        .await?;

    match &cli.command {
        Some(Commands::Install) => (),
        Some(Commands::Exec { args }) => {
            Executor::new(args.clone()).exec()?;
            return Ok(());
        }
        Some(Commands::Lock) => {
            return Ok(());
        }
        None => {}
    }

    // デフォルトのパス
    let gemfile_path = "Gemfile";

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
