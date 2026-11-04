//! Public repositories on GitHub, read without an account: the REST API for commits
//! and trees, raw.githubusercontent.com for the files.

use std::time::Duration;

use bm_core::skills::TreeEntry;
use reqwest::{Client, Response, StatusCode, Url, header};
use serde::Deserialize;

use crate::skill_import::{ImportError, Source};

const API: &str = "https://api.github.com";
const RAW: &str = "https://raw.githubusercontent.com";

pub struct Github {
    client: Client,
}

#[derive(Deserialize)]
struct Repository {
    default_branch: String,
}

#[derive(Deserialize)]
struct Tree {
    tree: Vec<TreeEntry>,
    #[serde(default)]
    truncated: bool,
}

/// An entry of a folder listing from the contents API.
#[derive(Deserialize)]
struct Content {
    name: String,
    sha: String,
    #[serde(rename = "type")]
    kind: String,
}

impl Github {
    pub fn new() -> Result<Self, ImportError> {
        // The updater installs the same provider; whoever comes first does.
        if rustls::crypto::CryptoProvider::get_default().is_none() {
            let _ = rustls::crypto::ring::default_provider().install_default();
        }
        let client = Client::builder()
            .user_agent(concat!("Brainmatter/", env!("CARGO_PKG_VERSION")))
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|e| ImportError::Unexpected(e.to_string()))?;
        Ok(Self { client })
    }

    async fn get(&self, url: Url, accept: &str) -> Result<Response, ImportError> {
        let request = self
            .client
            .get(url)
            .header(header::ACCEPT, accept)
            .header("X-GitHub-Api-Version", "2022-11-28");
        let response = request.send().await.map_err(failed)?;
        if let Some(limited) = rate_limit(&response) {
            return Err(limited);
        }
        Ok(response)
    }

    async fn json<T: for<'de> Deserialize<'de>>(&self, url: Url) -> Result<T, ImportError> {
        let response = self.get(url, "application/vnd.github+json").await?;
        match response.status() {
            StatusCode::OK => {}
            StatusCode::NOT_FOUND => return Err(ImportError::PathNotFound),
            status => return Err(ImportError::Status(status.as_u16())),
        }
        let bytes = response.bytes().await.map_err(failed)?;
        serde_json::from_slice(&bytes).map_err(|e| ImportError::Unexpected(e.to_string()))
    }

    /// The SHA of the tree at `folder`, found in the listing of its parent.
    async fn folder_tree(
        &self,
        repo: &str,
        revision: &str,
        folder: &str,
    ) -> Result<String, ImportError> {
        let (parent, name) = folder.rsplit_once('/').unwrap_or(("", folder));
        let mut url = api(repo, &["contents"]);
        extend(&mut url, parent);
        url.query_pairs_mut().append_pair("ref", revision);
        // A file instead of a folder answers with an object, not a list.
        let listing: Vec<Content> = match self.json(url).await {
            Err(ImportError::Unexpected(_)) => return Err(ImportError::PathNotFound),
            other => other?,
        };
        listing
            .into_iter()
            .find(|c| c.name == name && c.kind == "dir")
            .map(|c| c.sha)
            .ok_or(ImportError::PathNotFound)
    }
}

fn failed(e: reqwest::Error) -> ImportError {
    if e.is_timeout() {
        ImportError::Timeout
    } else if e.is_connect() || e.is_request() {
        ImportError::Offline
    } else {
        ImportError::Unexpected(e.to_string())
    }
}

/// GitHub answers 403 or 429 once an address has used up its requests.
fn rate_limit(response: &Response) -> Option<ImportError> {
    let header = |name: &str| {
        let value = response.headers().get(name)?.to_str().ok()?;
        value.trim().parse::<i64>().ok()
    };
    let used_up = header("x-ratelimit-remaining") == Some(0);
    let limited = match response.status() {
        StatusCode::TOO_MANY_REQUESTS => true,
        StatusCode::FORBIDDEN => used_up,
        _ => false,
    };
    if !limited {
        return None;
    }
    let now = bm_core::time::now_ms() / 1000;
    let reset = header("x-ratelimit-reset").or_else(|| header("retry-after").map(|s| now + s));
    Some(ImportError::RateLimited(reset))
}

/// Appends the parts of a `/`-separated path, each percent-encoded.
fn extend(url: &mut Url, path: &str) {
    if let Ok(mut segments) = url.path_segments_mut() {
        segments.extend(path.split('/').filter(|s| !s.is_empty()));
    }
}

fn api(repo: &str, rest: &[&str]) -> Url {
    let mut url = Url::parse(API).expect("a valid base");
    extend(&mut url, "repos");
    extend(&mut url, repo);
    for part in rest {
        extend(&mut url, part);
    }
    url
}

impl Source for Github {
    async fn default_branch(&self, repo: &str) -> Result<String, ImportError> {
        match self.json::<Repository>(api(repo, &[])).await {
            Err(ImportError::PathNotFound) => Err(ImportError::RepoNotFound),
            other => Ok(other?.default_branch),
        }
    }

    async fn commit(&self, repo: &str, reference: &str) -> Result<Option<String>, ImportError> {
        let url = api(repo, &["commits", reference]);
        let response = self.get(url, "application/vnd.github.sha").await?;
        match response.status() {
            StatusCode::OK => {}
            // 422 for a ref the repository does not know.
            StatusCode::NOT_FOUND | StatusCode::UNPROCESSABLE_ENTITY => return Ok(None),
            status => return Err(ImportError::Status(status.as_u16())),
        }
        let sha = response.text().await.map_err(failed)?.trim().to_string();
        if sha.len() != 40 || !sha.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(ImportError::Unexpected(format!("„{sha}“ is no commit")));
        }
        Ok(Some(sha))
    }

    async fn tree(
        &self,
        repo: &str,
        revision: &str,
        folder: &str,
    ) -> Result<Vec<TreeEntry>, ImportError> {
        let sha = match folder {
            "" => revision.to_string(),
            folder => self.folder_tree(repo, revision, folder).await?,
        };
        let mut url = api(repo, &["git", "trees", &sha]);
        url.query_pairs_mut().append_pair("recursive", "1");
        let tree: Tree = self.json(url).await?;
        if tree.truncated {
            return Err(ImportError::Truncated);
        }
        let entries = tree.tree.into_iter().map(|mut entry| {
            if !folder.is_empty() {
                entry.path = format!("{folder}/{}", entry.path);
            }
            entry
        });
        Ok(entries.collect())
    }

    async fn file(
        &self,
        repo: &str,
        revision: &str,
        path: &str,
        limit: u64,
    ) -> Result<Vec<u8>, ImportError> {
        let mut url = Url::parse(RAW).expect("a valid base");
        extend(&mut url, repo);
        extend(&mut url, revision);
        extend(&mut url, path);
        let mut response = self.get(url, "*/*").await?;
        match response.status() {
            StatusCode::OK => {}
            StatusCode::NOT_FOUND => return Err(ImportError::PathNotFound),
            status => return Err(ImportError::Status(status.as_u16())),
        }
        if response.content_length().is_some_and(|n| n > limit) {
            return Err(ImportError::TooLarge);
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(failed)? {
            if (bytes.len() + chunk.len()) as u64 > limit {
                return Err(ImportError::TooLarge);
            }
            bytes.extend_from_slice(&chunk);
        }
        Ok(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_are_encoded_part_by_part() {
        assert_eq!(api("o/r", &[]).as_str(), "https://api.github.com/repos/o/r");
        let commit = api("o/r", &["commits", "feature/x y"]);
        assert_eq!(commit.as_str(), "https://api.github.com/repos/o/r/commits/feature/x%20y");
        let mut contents = api("o/r", &["contents"]);
        extend(&mut contents, "");
        assert_eq!(contents.as_str(), "https://api.github.com/repos/o/r/contents");
        let mut raw = Url::parse(RAW).unwrap();
        extend(&mut raw, "o/r");
        extend(&mut raw, "skills/a#b?/SKILL.md");
        assert_eq!(raw.as_str(), "https://raw.githubusercontent.com/o/r/skills/a%23b%3F/SKILL.md");
    }
}
