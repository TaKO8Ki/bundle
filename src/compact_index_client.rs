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
use std::io::{self, BufRead, BufReader, Cursor, Read, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::Semaphore;
use tokio::task::JoinHandle;
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
    pub requirement_str: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct CompactIndexClient {
    base_url: Url,
    cache_dir: PathBuf,
    http_client: Client,
    limiter: Arc<Semaphore>,
}

pub enum InfoSource {
    File(File),                    // append された既存 or partial
    Mem(std::io::Cursor<Vec<u8>>), // fresh full body
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
            limiter: Arc::new(Semaphore::new(num_cpus::get())),
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

    #[instrument(level = Level::INFO, skip_all)]
    pub async fn resolve_dependencies(
        &self,
        root_gems: Vec<String>,
    ) -> Result<HashMap<String, Vec<GemVersion>>> {
        use futures::stream::StreamExt;
        // Ensure we have a fresh `/versions` file – *serial* (only once).
        self.ensure_versions_fresh().await?;

        // Work‑list algorithm, but each round is processed in parallel.
        let mut graph: HashMap<String, Vec<GemVersion>> = HashMap::new();
        let mut visited: HashSet<String> = HashSet::new();
        let mut queue: VecDeque<String> = root_gems.iter().cloned().collect();
        let mut tasks: FuturesUnordered<JoinHandle<Result<(String, Vec<GemVersion>)>>> =
            FuturesUnordered::new();
        // Shared Arc for all spawned tasks
        let shared_client = Arc::new(self.clone());

        // Function to spawn a fetch task; ONLY place where spawn happens
        let spawn_fetch = |client: Arc<CompactIndexClient>, gem: String| -> JoinHandle<_> {
            tokio::spawn(async move {
                let versions = client.info(&gem).await?;
                Ok::<_, CompactIndexError>((gem, versions))
            })
        };

        // initial fill (schedule unique downloads)
        let mut scheduled = HashSet::<String>::new();
        while let Some(name) = queue.pop_front() {
            if !visited.contains(&name) && !scheduled.contains(&name) {
                scheduled.insert(name.clone());
                tasks.push(spawn_fetch(Arc::clone(&shared_client), name));
            }
            if tasks.len() >= shared_client.limiter.available_permits() {
                break;
            }
        }

        // main loop
        while let Some(out) = tasks.next().await {
            let (gem, versions) = out.unwrap().unwrap();
            if visited.insert(gem.clone()) {
                graph.insert(gem, versions.clone());
            }
            for v in &versions {
                for d in &v.dependencies {
                    if !visited.contains(&d.name) {
                        queue.push_back(d.name.clone());
                    }
                }
            }

            // refill window
            while tasks.len() < shared_client.limiter.available_permits() {
                if let Some(n) = queue.pop_front() {
                    if !visited.contains(&n) && !scheduled.contains(&n) {
                        scheduled.insert(n.clone());
                        tasks.push(spawn_fetch(Arc::clone(&shared_client), n));
                    }
                } else {
                    break;
                }
            }
        }
        Ok(graph)
    }

    async fn ensure_versions_fresh(&self) -> Result<()> {
        let url = self.base_url.join("versions")?;
        let path = self.cache_dir.join("versions");
        self.update_cache(&url, &path, &path).await?;
        Ok(())
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

        // TODO: It's possible to return bytes or File from this function and reuse it in `CompactIndexClient::info`.
        // It can reduce overlapped I/O.
        let file = self
            .update_cache(&info_url, &info_path, &info_etag_path)
            .await?;

        // Check if the info file exists
        // info file is sometimes empty like https://rubygems.org/info/active_support.
        // If it is empty, we don't create a new file.
        // We just return an empty vector.
        if !info_path.exists() {
            return Ok(vec![]);
        }

        // This line would be unnecessary if update_cache returns a file or bytes, which info APi returns.
        // let Some(file) = file else {
        //     return Ok(vec![]);
        // };
        let mut result = Vec::new();

        debug!("Reading info file for gem: {}", gem_name);

        let file: Box<dyn BufRead> = match file {
            Some(InfoSource::File(f)) => Box::new(BufReader::new(f)),
            Some(InfoSource::Mem(c)) => Box::new(BufReader::new(c)),
            None => {
                return Ok(vec![]);
            }
        };

        for raw in BufReader::new(file).lines().flatten() {
            if raw.starts_with("---") {
                continue;
            }

            let line = raw.split('|').next().unwrap_or(&raw);

            let mut parts = line.splitn(2, ' ');
            let ver_str = parts.next().unwrap();
            let deps_str = parts.next().unwrap_or("");
            let rv = RubyVersion::parse(ver_str);
            // if rv.is_prerelease() {
            //     println!("pre: {rv}")
            // }

            let mut dependencies = Vec::new();

            for dep_entry in deps_str.split(',') {
                let dep_entry = dep_entry.trim();
                if dep_entry.is_empty() {
                    continue;
                }
                if let Some(idx) = dep_entry.find(':') {
                    let name = dep_entry[..idx].to_string();

                    let req_str = dep_entry[idx + 1..].trim();
                    if req_str == "~>" {
                        println!("raw: {raw}, gem_name: {gem_name}");
                    }
                    let (req, req_str) = parse_req(req_str, "&");
                    dependencies.push(GemDependency {
                        name: name.to_string(),
                        requirement: req,
                        requirement_str: req_str,
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
    async fn update_cache(
        &self,
        url: &Url,
        cache_path: &Path,
        etag_path: &Path,
    ) -> Result<Option<InfoSource>> {
        let mut headers = HeaderMap::new();

        if etag_path.exists() {
            if let Some(etag) = self.read_etag(etag_path)? {
                headers.insert(IF_NONE_MATCH, HeaderValue::from_str(&etag).unwrap());
            }

            if let Ok(metadata) = fs::metadata(etag_path) {
                if metadata.len() > 0 {
                    let range = format!("bytes={}-", metadata.len() - 1);
                    headers.insert(RANGE, HeaderValue::from_str(&range).unwrap());
                }
            }
        }

        let response = self
            .http_client
            .get(url.clone())
            .headers(headers)
            .send()
            .await?;

        if response.status() == reqwest::StatusCode::NOT_MODIFIED {
            return Ok(None);
        }

        if response.status().is_success() {
            return Ok(self
                .process_response(response, cache_path, etag_path)
                .await?);
        } else {
            return Err(CompactIndexError::Other(format!(
                "HTTP error: {} for URL: {}",
                response.status(),
                url
            )));
        }
    }

    async fn process_response(
        &self,
        response: Response,
        cache_path: &Path,
        etag_path: &Path,
    ) -> Result<Option<InfoSource>> {
        let is_partial = response.status() == reqwest::StatusCode::PARTIAL_CONTENT;

        if let Some(etag) = response.headers().get(ETAG) {
            self.write_etag(etag_path, etag.to_str().unwrap())?;
        }

        let body = response.bytes().await?;

        let file = if is_partial && cache_path.exists() {
            let mut file = fs::OpenOptions::new()
                .append(true)
                .read(true)
                .open(cache_path)?;

            file.write_all(&body[1..])?;
            InfoSource::File(file)
        } else {
            // If the body is empty, we don't create a new file.
            if let Ok(text) = std::str::from_utf8(&body) {
                let mut lines = text.lines();
                if lines.next() == Some("---") && lines.next().is_none() {
                    return Ok(None);
                }
            }
            let mut file = File::create(cache_path)?;
            file.write_all(&body)?;
            InfoSource::Mem(Cursor::new(body.to_vec()))
        };

        Ok(Some(file))
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

#[cfg(test)]
mod tests {
    use std::{
        fs::File,
        io::{BufRead, BufReader},
        path::PathBuf,
    };

    #[test]
    fn test_open_file() {
        // let file = File::open(PathBuf::from(
        //     ".newbundle/cache/compact_index/rubygems.org.443.63ce7be7/info/aws-sdk-emrserverlesswebservice",
        // ))
        // .expect("Failed to open info file");
        // let mut lines = BufReader::new(file).lines().flatten();
        // let line = lines.next().unwrap();
        // assert_eq!(line, "---");
        // let line = lines.next().unwrap();
        // assert_eq!(line, "\n");
        // let line = lines.next().unwrap();
    }

    // #[test]
    // fn test_parse_version() {
    //     let mut file = fs::OpenOptions::new().append(true).open(cache_path)?;
    // }
}
