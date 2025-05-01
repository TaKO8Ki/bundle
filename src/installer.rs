// src/installer.rs
use crate::compact_index_client::GemVersion;
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum InstallerError {
    #[error("IO error: {0}")]
    Io(#[from] io::Error),

    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("External command error: {0}")]
    Command(String),

    #[error("Gem extraction error: {0}")]
    Extraction(String),

    #[error("Other error: {0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, InstallerError>;

pub struct GemInstaller {
    install_base_dir: PathBuf,
    cache_dir: PathBuf,
    base_url: String,
    // Ruby version for paths
    ruby_version: String,
}

impl GemInstaller {
    pub fn new(install_base_dir: &Path, cache_dir: &Path, base_url: &str) -> Result<Self> {
        // Ruby のバージョンを取得
        let ruby_version = Self::get_ruby_version()?;

        // ディレクトリ構造を作成
        let full_install_dir = install_base_dir.join("gems").join(&ruby_version);
        fs::create_dir_all(&full_install_dir.join("gems"))?;
        fs::create_dir_all(&full_install_dir.join("specifications"))?;
        fs::create_dir_all(&full_install_dir.join("extensions"))?;
        fs::create_dir_all(&full_install_dir.join("bin"))?;

        fs::create_dir_all(cache_dir)?;

        Ok(Self {
            install_base_dir: install_base_dir.to_path_buf(),
            cache_dir: cache_dir.to_path_buf(),
            base_url: base_url.to_string(),
            ruby_version,
        })
    }

    // Rubyのバージョンを取得
    fn get_ruby_version() -> Result<String> {
        let output = Command::new("ruby")
            .args(&["-e", "puts RUBY_VERSION"])
            .output()?;

        if !output.status.success() {
            return Err(InstallerError::Command(
                String::from_utf8_lossy(&output.stderr).to_string(),
            ));
        }

        let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
        Ok(version)
    }

    pub fn install_gems(&self, resolved_gems: HashMap<String, GemVersion>) -> Result<()> {
        for (name, version) in resolved_gems {
            self.install_gem(&name, &version.version.to_string())?;
        }

        Ok(())
    }

    fn install_gem(&self, name: &str, version: &str) -> Result<()> {
        let gem_filename = format!("{}-{}.gem", name, version);
        let cache_path = self.cache_dir.join(&gem_filename);

        // すでにインストールされているかチェック
        if self.is_gem_installed(name, version)? {
            println!("Gem {} ({}) is already installed", name, version);
            return Ok(());
        }

        // キャッシュになければダウンロード
        if !cache_path.exists() {
            self.download_gem(name, version, &cache_path)?;
        }

        // gemを解凍してインストール
        self.extract_and_install_gem(name, version, &cache_path)?;

        println!("Installed {} ({})", name, version);
        Ok(())
    }

    fn is_gem_installed(&self, name: &str, version: &str) -> Result<bool> {
        let gem_dir = self.get_gems_dir().join(format!("{}-{}", name, version));
        let gemspec_path = self
            .get_specifications_dir()
            .join(format!("{}-{}.gemspec", name, version));

        Ok(gem_dir.exists() && gemspec_path.exists())
    }

    fn download_gem(&self, name: &str, version: &str, output_path: &Path) -> Result<()> {
        let url = format!("{}/gems/{}-{}.gem", self.base_url, name, version);

        let client = reqwest::blocking::Client::new();
        let mut response = client.get(&url).send()?;

        if !response.status().is_success() {
            return Err(InstallerError::Other(format!(
                "Failed to download gem: HTTP status {}",
                response.status()
            )));
        }

        let mut file = File::create(output_path)?;
        let mut content = Vec::new();
        response.copy_to(&mut content)?;
        file.write_all(&content)?;

        Ok(())
    }

    fn extract_and_install_gem(&self, name: &str, version: &str, gem_path: &Path) -> Result<()> {
        let gem_full_name = format!("{}-{}", name, version);
        let gem_dir = self.get_gems_dir().join(&gem_full_name);
        let spec_dir = self.get_specifications_dir();

        // gemディレクトリを作成
        fs::create_dir_all(&gem_dir)?;

        // gemファイルを解凍
        self.extract_gem(gem_path, &gem_dir)?;

        // .gemspecファイルをspecificationsディレクトリにコピー
        let gemspec_source = gem_dir.join("metadata.gz");
        let gemspec_dest = spec_dir.join(format!("{}.gemspec", gem_full_name));

        // metadata.gzを解凍して.gemspecファイルを作成
        let mut source_file = File::open(&gemspec_source)?;
        let mut compressed_data = Vec::new();
        source_file.read_to_end(&mut compressed_data)?;

        // gem自体の実行ファイルをbinディレクトリに作成
        self.setup_bin_files(name, version, &gem_dir)?;

        // ネイティブ拡張があれば、extensionsディレクトリに展開
        self.build_extensions(name, version, &gem_dir)?;

        Ok(())
    }

    fn extract_gem(&self, gem_path: &Path, output_dir: &Path) -> Result<()> {
        // tar コマンドを使って.gemファイルを解凍
        // gemファイルはtar.gzファイルの一種です

        let output = Command::new("tar")
            .args(&[
                "xzf",
                gem_path.to_str().unwrap(),
                "-C",
                output_dir.to_str().unwrap(),
            ])
            .output()?;

        if !output.status.success() {
            return Err(InstallerError::Extraction(
                String::from_utf8_lossy(&output.stderr).to_string(),
            ));
        }

        Ok(())
    }

    fn setup_bin_files(&self, name: &str, version: &str, gem_dir: &Path) -> Result<()> {
        let bin_dir = gem_dir.join("bin");

        if !bin_dir.exists() {
            return Ok(()); // バイナリがないgem
        }

        let target_bin_dir = self.get_bin_dir();
        fs::create_dir_all(&target_bin_dir)?;

        // binディレクトリ内の各ファイルについて
        for entry in fs::read_dir(&bin_dir)? {
            let entry = entry?;
            let bin_name = entry.file_name();
            let source_path = entry.path();
            let target_path = target_bin_dir.join(&bin_name);

            // 実行ファイルのラッパースクリプトを作成
            let wrapper_content = format!(
                "#!/usr/bin/env ruby\n\
                 # This file was generated by bundle_rust\n\
                 ENV['GEM_HOME'] = '{}'\n\
                 ENV['GEM_PATH'] = '{}'\n\
                 $:.unshift File.expand_path('../../lib', __FILE__)\n\
                 load File.expand_path('../../gems/{}-{}/bin/{}', __FILE__)\n",
                self.get_gems_base_dir().display(),
                self.get_gems_base_dir().display(),
                name,
                version,
                bin_name.to_string_lossy()
            );

            let mut file = File::create(&target_path)?;
            file.write_all(wrapper_content.as_bytes())?;

            // 実行権限を付与
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let mut perms = fs::metadata(&target_path)?.permissions();
                perms.set_mode(0o755);
                fs::set_permissions(&target_path, perms)?;
            }
        }

        Ok(())
    }

    fn build_extensions(&self, name: &str, version: &str, gem_dir: &Path) -> Result<()> {
        let ext_dir = gem_dir.join("ext");

        if !ext_dir.exists() {
            return Ok(()); // 拡張機能がないgem
        }

        let extensions_dir = self.get_extensions_dir();
        let platform = Self::get_platform()?;
        let target_ext_dir = extensions_dir
            .join(&platform)
            .join(format!("{}-{}", name, version));

        fs::create_dir_all(&target_ext_dir)?;

        // 各拡張ディレクトリをビルド
        for entry in fs::read_dir(&ext_dir)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                let ext_name = entry.file_name();
                let ext_source_dir = entry.path();

                // extconf.rbを実行してMakefileを生成
                let output = Command::new("ruby")
                    .current_dir(&ext_source_dir)
                    .args(&["extconf.rb"])
                    .output()?;

                if !output.status.success() {
                    println!(
                        "Warning: Failed to run extconf.rb for {}: {}",
                        ext_name.to_string_lossy(),
                        String::from_utf8_lossy(&output.stderr)
                    );
                    continue;
                }

                // makeを実行してビルド
                let output = Command::new("make").current_dir(&ext_source_dir).output()?;

                if !output.status.success() {
                    println!(
                        "Warning: Failed to build extension {} for {}-{}: {}",
                        ext_name.to_string_lossy(),
                        name,
                        version,
                        String::from_utf8_lossy(&output.stderr)
                    );
                    continue;
                }

                // ビルドした.soファイルをコピー
                for entry in fs::read_dir(&ext_source_dir)? {
                    let entry = entry?;
                    let file_name = entry.file_name();
                    let path = entry.path();

                    if path.extension().map_or(false, |ext| ext == "so") {
                        let target_path = target_ext_dir.join(&file_name);
                        fs::copy(&path, &target_path)?;
                    }
                }
            }
        }

        Ok(())
    }

    fn get_platform() -> Result<String> {
        let output = Command::new("ruby")
            .args(&["-e", "puts RUBY_PLATFORM"])
            .output()?;

        if !output.status.success() {
            return Err(InstallerError::Command(
                String::from_utf8_lossy(&output.stderr).to_string(),
            ));
        }

        let platform = String::from_utf8_lossy(&output.stdout).trim().to_string();
        Ok(platform)
    }

    // ディレクトリ構造のヘルパーメソッド
    fn get_gems_base_dir(&self) -> PathBuf {
        self.install_base_dir.join("gems").join(&self.ruby_version)
    }

    fn get_gems_dir(&self) -> PathBuf {
        self.get_gems_base_dir().join("gems")
    }

    fn get_specifications_dir(&self) -> PathBuf {
        self.get_gems_base_dir().join("specifications")
    }

    fn get_extensions_dir(&self) -> PathBuf {
        self.get_gems_base_dir().join("extensions")
    }

    fn get_bin_dir(&self) -> PathBuf {
        self.get_gems_base_dir().join("bin")
    }
}
