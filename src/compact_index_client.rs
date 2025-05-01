use futures::stream::FuturesUnordered;
use lazy_static::lazy_static;
use md5::{Digest as Md5Digest, Md5};
use pubgrub::Ranges;
use regex::Regex;
use reqwest::header::{ETAG, HeaderMap, HeaderValue, IF_NONE_MATCH, RANGE};
use reqwest::{Client, Response};
use sha2::{Digest as Sha2Digest, Sha256};
use std::collections::{HashMap, HashSet, VecDeque};
use std::fs::{self, File, read_dir};
use std::io::{self, BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use thiserror::Error;
use tracing::{Level, debug, instrument};
use url::Url;

use crate::version::{RubyVersion, Segment, parse_req};

#[derive(Error, Debug)]
pub enum CompactIndexError {
    #[error("IO error: {0}")]
    Io(#[from] io::Error),

    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("Checksum mismatch: expected {expected}, got {actual}")]
    ChecksumMismatch { expected: String, actual: String },

    #[error("URL parsing error: {0}")]
    UrlParse(#[from] url::ParseError),

    #[error("Other error: {0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, CompactIndexError>;

#[derive(Debug, Clone)]
pub struct GemVersion {
    pub name: String,
    pub version: RubyVersion,
    pub checksum: Option<String>,
    pub dependencies: Vec<GemDependency>,
}

#[derive(Debug, Clone)]
pub struct GemDependency {
    pub name: String,
    pub requirement: Ranges<RubyVersion>,
}

pub struct CompactIndexClient {
    base_url: Url,
    cache_dir: PathBuf,
    http_client: Client,
}

impl CompactIndexClient {
    pub fn new(base_url: &str, bundle_dir: &Path) -> Result<Self> {
        let url = Url::parse(base_url)?;

        let cache_slug = Self::cache_slug_for_url(&url)?;
        let cache_dir = bundle_dir
            .join("cache")
            .join("compact_index")
            .join(cache_slug);

        fs::create_dir_all(&cache_dir)?;
        fs::create_dir_all(&cache_dir.join("info"))?;
        fs::create_dir_all(&cache_dir.join("info-etags"))?;

        Ok(Self {
            base_url: url,
            cache_dir,
            http_client: Client::new(),
        })
    }

    fn cache_slug_for_url(url: &Url) -> Result<String> {
        lazy_static! {
            static ref UNSAFE_CHARS: Regex = Regex::new(r"[^A-Za-z0-9._-]").unwrap();
        }

        let host = url
            .host_str()
            .ok_or_else(|| CompactIndexError::Other("URL has no host".to_string()))?;

        let port = url
            .port()
            .map(|p| p.to_string())
            .unwrap_or_else(|| match url.scheme() {
                "http" => "80".to_string(),
                "https" => "443".to_string(),
                _ => "0".to_string(),
            });

        let url_str = url.as_str();
        let mut hasher = Md5::new();
        hasher.update(url_str.as_bytes());
        let hash_result = format!("{:x}", hasher.finalize());
        let hash = &hash_result[0..8];

        let sanitized_host = UNSAFE_CHARS.replace_all(host, "-");

        let slug = format!("{}.{}.{}", sanitized_host, port, hash);
        Ok(slug)
    }

    pub async fn resolve_dependencies(
        &self,
        root_gems: Vec<String>,
    ) -> Result<HashMap<String, Vec<GemVersion>>> {
        use futures::stream::StreamExt;

        // self.ensure_versions_fresh().await?;

        let mut graph: HashMap<String, Vec<GemVersion>> = HashMap::new();
        let mut visited: HashSet<String> = HashSet::new();
        let mut queue: VecDeque<String> = root_gems.iter().cloned().collect();
        let mut tasks: FuturesUnordered<_> = FuturesUnordered::new();
        let max_tasks = num_cpus::get();

        loop {
            // Fill the window – **single point where `tasks.push` happens**
            while tasks.len() < max_tasks {
                match queue.pop_front() {
                    Some(name) if !visited.contains(&name) => {
                        let client = &self;
                        let n = name.clone();
                        tasks.push(async move {
                            let vers = client.info(&n).await?;
                            Ok::<_, CompactIndexError>((n, vers))
                        });
                    }
                    _ => break,
                }
            }

            // Break if no active tasks and nothing queued.
            if tasks.is_empty() {
                break;
            }

            // Await one completed task.
            let (gem_name, versions) = tasks.next().await.expect("stream non-empty")?;
            visited.insert(gem_name.clone());
            for gv in &versions {
                for dep in &gv.dependencies {
                    if !visited.contains(&dep.name) {
                        queue.push_back(dep.name.clone());
                    }
                }
            }
            graph.insert(gem_name, versions);
        }
        Ok(graph)
    }

    pub async fn versions(&self, gems: Vec<String>) -> Result<HashMap<String, Vec<RubyVersion>>> {
        let versions_path = self.cache_dir.join("versions");
        let versions_url = self.base_url.join("versions")?;

        self.update_cache(&versions_url, &versions_path, &versions_path)
            .await?;

        let content_lines = BufReader::new(File::open(versions_path)?)
            .lines()
            .flatten()
            .skip_while(|line| *line != "---")
            .skip(1);
        Ok(parse_version(content_lines, gems))
    }

    #[instrument(level = Level::DEBUG, skip_all)]
    pub async fn info(&self, gem_name: &str) -> Result<Vec<GemVersion>> {
        let info_path = self.cache_dir.join("info").join(gem_name);
        let info_etag_path = self.cache_dir.join("info-etags").join(gem_name);
        let info_url = self.base_url.join(&format!("info/{}", gem_name))?;

        debug!("bbbbbbbbbbbbbbbbbbbb: {}", gem_name);

        self.update_cache(&info_url, &info_path, &info_etag_path)
            .await?;

        let file = File::open(info_path).expect("Failed to open info file");
        let mut result = Vec::new();

        debug!("Reading info file for gem: {}", gem_name);

        for raw in BufReader::new(file).lines().flatten() {
            if raw.starts_with("---") {
                continue;
            }

            let line = raw.split('|').next().unwrap_or(&raw);

            let mut parts = line.splitn(2, ' ');
            let ver_str = parts.next().unwrap();
            let deps_str = parts.next().unwrap_or("");
            let rv = RubyVersion::parse(ver_str);
            // let mut deps_vec = Vec::new();

            let mut dependencies = Vec::new();

            for dep_entry in deps_str.split(',') {
                let dep_entry = dep_entry.trim();
                if dep_entry.is_empty() {
                    continue;
                }
                if let Some(idx) = dep_entry.find(':') {
                    let name = dep_entry[..idx].to_string();

                    let req_str = dep_entry[idx + 1..].trim();
                    let req = parse_req(req_str, "&");
                    // deps_vec.push((name, req));
                    dependencies.push(GemDependency {
                        name: name.to_string(),
                        requirement: req,
                    });
                }
            }
            result.push(GemVersion {
                name: gem_name.to_string(),
                version: rv,
                checksum: None, // checksum is after the pipe; omitted here for brevity
                dependencies,
            });
        }
        Ok(result)
    }

    #[instrument(level = Level::DEBUG, skip_all)]
    async fn update_cache(&self, url: &Url, path: &Path, etag_cache_path: &Path) -> Result<()> {
        let mut headers = HeaderMap::new();
        if path.exists() {
            if let Some(etag) = self.read_etag(path)? {
                headers.insert(IF_NONE_MATCH, HeaderValue::from_str(&etag).unwrap());
            }
            if let Ok(meta) = fs::metadata(path) {
                if meta.len() > 0 {
                    headers.insert(
                        RANGE,
                        HeaderValue::from_str(&format!("bytes={}-", meta.len() - 1)).unwrap(),
                    );
                }
            }
        }
        let resp = self
            .http_client
            .get(url.clone())
            .headers(headers)
            .send()
            .await?;
        match resp.status() {
            reqwest::StatusCode::NOT_MODIFIED => Ok(()),
            s if s.is_success() => {
                self.process_response(resp, path, etag_cache_path).await?;
                Ok(())
            }
            s => Err(CompactIndexError::Other(format!("HTTP {} for {}", s, url))),
        }
    }

    async fn process_response(
        &self,
        response: Response,
        cache_path: &Path,
        etag_cache_path: &Path,
    ) -> Result<()> {
        let is_partial = response.status() == reqwest::StatusCode::PARTIAL_CONTENT;

        if let Some(etag) = response.headers().get(ETAG) {
            self.write_etag(etag_cache_path, etag.to_str().unwrap())?;
        }

        let body = response.bytes().await?;

        debug!("is_partial: {}", is_partial);

        if is_partial && cache_path.exists() {
            let mut file = fs::OpenOptions::new().append(true).open(cache_path)?;

            file.write_all(&body[1..])?;
        } else {
            let mut file = File::create(cache_path)?;
            file.write_all(&body)?;
        }

        Ok(())
    }

    fn read_etag(&self, file_path: &Path) -> Result<Option<String>> {
        let etag_path = file_path.with_extension("etag");

        if etag_path.exists() {
            let etag = fs::read_to_string(&etag_path)?;
            Ok(Some(etag))
        } else {
            Ok(None)
        }
    }

    fn write_etag(&self, file_path: &Path, etag: &str) -> Result<()> {
        let etag_path = file_path.with_extension("etag");
        fs::write(&etag_path, etag)?;
        Ok(())
    }

    fn md5_checksum(&self, file_path: &Path) -> Result<String> {
        let mut file = File::open(file_path)?;
        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer)?;

        let digest = Md5::digest(&buffer);
        let result = format!("{:x}", digest);

        Ok(result)
    }
}

#[instrument(skip_all)]
fn parse_version<'a>(
    lines: impl Iterator<Item = String>,
    gems: Vec<String>,
) -> HashMap<String, Vec<RubyVersion>> {
    let mut map: HashMap<String, Vec<RubyVersion>> = HashMap::new();

    let gems_set: HashSet<String> = gems.into_iter().collect();
    for line in lines {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 2 {
            continue;
        }
        if !gems_set.contains(parts[0]) {
            continue;
        }
        for ver_str in parts[1].split(',') {
            let rv = RubyVersion::parse(ver_str.trim());
            map.entry(parts[0].to_string()).or_default().push(rv);
        }
    }
    map
}
