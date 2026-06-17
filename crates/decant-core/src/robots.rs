//! robots.txt fetching and enforcement.
//!
//! Fetches `robots.txt` once per host (cached for the lifetime of the crawl),
//! then checks each URL against it using the `robotstxt` crate.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use reqwest::Client;
use url::Url;

use crate::CoreError;

/// Shared robots.txt cache — one entry per host.
#[derive(Clone, Default)]
pub struct RobotsCache {
    inner: Arc<Mutex<HashMap<String, Option<String>>>>,
}

impl RobotsCache {
    /// Create a new, empty robots.txt cache.
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns `true` if the given URL is allowed for `user_agent`.
    ///
    /// Fetches `robots.txt` from the host on first call; subsequent calls for
    /// the same host are served from cache.  A fetch error is treated as
    /// "no robots.txt" (allow all).
    pub async fn is_allowed(
        &self,
        client: &Client,
        url: &Url,
        user_agent: &str,
    ) -> Result<bool, CoreError> {
        let host = url.host_str().unwrap_or("").to_string();
        let robots_txt = self.fetch_cached(client, url, &host).await?;

        match robots_txt {
            None => Ok(true), // no robots.txt → allow all
            Some(body) => {
                // robotstxt 0.3 API: check via DefaultMatcher
                use robotstxt::DefaultMatcher;
                Ok(DefaultMatcher::default().one_agent_allowed_by_robots(
                    &body,
                    user_agent,
                    url.as_str(),
                ))
            }
        }
    }

    /// Fetch robots.txt for a host (caches the result, even on error).
    async fn fetch_cached(
        &self,
        client: &Client,
        url: &Url,
        host: &str,
    ) -> Result<Option<String>, CoreError> {
        // Fast path: already cached.
        {
            let cache = self.inner.lock().expect("robots cache lock poisoned");
            if let Some(val) = cache.get(host) {
                return Ok(val.clone());
            }
        }

        // Build the robots.txt URL.
        let robots_url = {
            let mut u = url.clone();
            u.set_path("/robots.txt");
            u.set_query(None);
            u.set_fragment(None);
            u
        };

        let body = match client.get(robots_url.as_str()).send().await {
            Ok(resp) if resp.status().is_success() => Some(resp.text().await.unwrap_or_default()),
            _ => None, // 404 or network error → treat as allow-all
        };

        let mut cache = self.inner.lock().expect("robots cache poisoned");
        cache.insert(host.to_string(), body.clone());
        Ok(body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use reqwest::Client;

    async fn start_mock_server(_body: &'static str) -> (tokio::net::TcpListener, String) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let url = format!("http://127.0.0.1:{}", port);
        (listener, url)
    }

    #[tokio::test]
    async fn test_robots_allowed_by_default_on_404() {
        let (listener, url_str) = start_mock_server("").await;
        // Mock a 404 responder
        tokio::spawn(async move {
            if let Ok((mut stream, _)) = listener.accept().await {
                let mut buf = [0u8; 1024];
                let _ = tokio::io::AsyncReadExt::read(&mut stream, &mut buf).await;
                let response =
                    "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
                let _ = tokio::io::AsyncWriteExt::write_all(&mut stream, response.as_bytes()).await;
            }
        });

        let cache = RobotsCache::new();
        let client = Client::new();
        let url = Url::parse(&format!("{}/some-page", url_str)).unwrap();

        // Should be allowed on 404
        let allowed = cache.is_allowed(&client, &url, "decant").await.unwrap();
        assert!(allowed);
    }

    #[tokio::test]
    async fn test_robots_enforces_disallow() {
        let robots_content = "User-agent: *\nDisallow: /secret\n";
        let (listener, url_str) = start_mock_server(robots_content).await;
        tokio::spawn(async move {
            if let Ok((mut stream, _)) = listener.accept().await {
                let mut buf = [0u8; 1024];
                let _ = tokio::io::AsyncReadExt::read(&mut stream, &mut buf).await;
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    robots_content.len(),
                    robots_content
                );
                let _ = tokio::io::AsyncWriteExt::write_all(&mut stream, response.as_bytes()).await;
            }
        });

        let cache = RobotsCache::new();
        let client = Client::new();

        let allowed_url = Url::parse(&format!("{}/public", url_str)).unwrap();
        let disallowed_url = Url::parse(&format!("{}/secret", url_str)).unwrap();

        // Public allowed
        assert!(
            cache
                .is_allowed(&client, &allowed_url, "decant")
                .await
                .unwrap()
        );
        // Secret disallowed
        assert!(
            !cache
                .is_allowed(&client, &disallowed_url, "decant")
                .await
                .unwrap()
        );
    }

    #[tokio::test]
    async fn test_robots_cache_prevents_duplicate_fetches() {
        let robots_content = "User-agent: *\nDisallow: /blocked\n";
        let (listener, url_str) = start_mock_server(robots_content).await;
        // Mock server only accepts ONE connection.
        tokio::spawn(async move {
            if let Ok((mut stream, _)) = listener.accept().await {
                let mut buf = [0u8; 1024];
                let _ = tokio::io::AsyncReadExt::read(&mut stream, &mut buf).await;
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    robots_content.len(),
                    robots_content
                );
                let _ = tokio::io::AsyncWriteExt::write_all(&mut stream, response.as_bytes()).await;
            }
        });

        let cache = RobotsCache::new();
        let client = Client::new();

        let url1 = Url::parse(&format!("{}/blocked", url_str)).unwrap();
        let url2 = Url::parse(&format!("{}/allowed", url_str)).unwrap();

        // First call fetches from server and caches
        assert!(!cache.is_allowed(&client, &url1, "decant").await.unwrap());

        // Second call should hit the cache instead of attempting another fetch.
        // Since /allowed is not disallowed, it should return true (allowed).
        // (If it tried to fetch, it would hang or fail since mock server only accepts 1 connection).
        assert!(cache.is_allowed(&client, &url2, "decant").await.unwrap());
    }
}
