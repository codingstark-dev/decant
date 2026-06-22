//! Unified browser instance that works for both Chrome and Lightpanda backends.

use crate::backend::BrowserBackend;
use crate::error::RenderError;
use chromiumoxide::{Browser as CBrowser, BrowserConfig};
use futures::StreamExt;

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::process::Command;

/// The unified browser instance.
pub struct Browser {
    inner: CBrowser,
    backend: BrowserBackend,
    _proc: Option<tokio::process::Child>,
    profile_dir: Option<PathBuf>,
}

impl Browser {
    /// Launch the browser using the specified backend.
    ///
    /// # Errors
    ///
    /// Returns a `RenderError` if browser execution or connection fails.
    pub async fn launch(backend: BrowserBackend) -> Result<Self, RenderError> {
        match backend {
            BrowserBackend::Chrome => {
                let chrome_bin = BrowserBackend::resolve_chrome().ok_or_else(|| {
                    RenderError::ChromeNotFound(
                        "Chrome binary not found on PATH or via CHROME_PATH".to_string(),
                    )
                })?;
                let _launch_lock = acquire_chrome_launch_lock().await?;

                let mut last_error = None;
                for attempt in 0_u64..5 {
                    let profile_dir = unique_chrome_profile_dir();
                    std::fs::create_dir_all(&profile_dir)
                        .map_err(|e| RenderError::BrowserLaunch(e.to_string()))?;

                    let config = BrowserConfig::builder()
                        .chrome_executable(chrome_bin.clone())
                        .user_data_dir(&profile_dir)
                        .arg("--headless=new")
                        .arg("--disable-gpu")
                        .arg("--no-sandbox")
                        .arg("--disable-setuid-sandbox")
                        .build()
                        .map_err(|e| RenderError::BrowserLaunch(e.to_string()))?;

                    match CBrowser::launch(config).await {
                        Ok((browser, mut handler)) => {
                            tokio::spawn(async move {
                                while let Some(res) = handler.next().await {
                                    if let Err(e) = res {
                                        tracing::debug!("Chrome handler error: {:?}", e);
                                    }
                                }
                            });

                            return Ok(Self {
                                inner: browser,
                                backend,
                                _proc: None,
                                profile_dir: Some(profile_dir),
                            });
                        }
                        Err(e) => {
                            last_error = Some(e);
                            let _ = std::fs::remove_dir_all(&profile_dir);
                            tokio::time::sleep(Duration::from_millis(750 * (attempt + 1))).await;
                        }
                    }
                }

                Err(RenderError::BrowserLaunch(last_error.map_or_else(
                    || "Chrome launch failed".to_string(),
                    |e| e.to_string(),
                )))
            }
            BrowserBackend::Lightpanda => {
                let lp_bin = BrowserBackend::resolve_lightpanda().ok_or_else(|| {
                    RenderError::LightpandaNotFound(
                        "Lightpanda binary not found on PATH or via LIGHTPANDA_BIN".to_string(),
                    )
                })?;

                // Spawn lightpanda process: lightpanda serve --host 127.0.0.1 --port 9222
                let child = Command::new(lp_bin)
                    .arg("serve")
                    .arg("--host")
                    .arg("127.0.0.1")
                    .arg("--port")
                    .arg("9222")
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .spawn()
                    .map_err(|e| {
                        RenderError::BrowserLaunch(format!("Failed to spawn Lightpanda: {e}"))
                    })?;

                // Wait a moment for Lightpanda to startup and bind port
                tokio::time::sleep(Duration::from_millis(500)).await;

                // Connect via websocket
                let ws_url = "ws://127.0.0.1:9222";
                let (browser, mut handler) = CBrowser::connect(ws_url)
                    .await
                    .map_err(|e| RenderError::CdpConnection(e.to_string()))?;

                tokio::spawn(async move {
                    while let Some(res) = handler.next().await {
                        if let Err(e) = res {
                            tracing::debug!("Lightpanda handler error: {:?}", e);
                        }
                    }
                });

                Ok(Self {
                    inner: browser,
                    backend,
                    _proc: Some(child),
                    profile_dir: None,
                })
            }
        }
    }

    /// Shutdown the browser and cleanup associated resources.
    pub async fn shutdown(mut self) {
        let _ = self.inner.close().await;
        if tokio::time::timeout(Duration::from_secs(5), self.inner.wait())
            .await
            .is_err()
        {
            let _ = self.inner.kill().await;
        }

        if let Some(mut child) = self._proc.take() {
            let _ = child.kill().await;
        }

        if let Some(profile_dir) = self.profile_dir.take() {
            let _ = std::fs::remove_dir_all(profile_dir);
        }
    }

    /// Get a reference to the underlying chromiumoxide Browser.
    pub fn inner(&self) -> &CBrowser {
        &self.inner
    }

    /// Get the backend type of this browser.
    pub fn backend(&self) -> BrowserBackend {
        self.backend
    }
}

fn unique_chrome_profile_dir() -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    std::env::temp_dir().join(format!("decant-chrome-{}-{nanos}", std::process::id()))
}

struct ChromeLaunchLock {
    path: PathBuf,
}

impl Drop for ChromeLaunchLock {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir(&self.path);
    }
}

async fn acquire_chrome_launch_lock() -> Result<ChromeLaunchLock, RenderError> {
    let path = std::env::temp_dir().join("decant-chrome-launch.lock");
    for _ in 0..1_200 {
        match std::fs::create_dir(&path) {
            Ok(()) => return Ok(ChromeLaunchLock { path }),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                remove_stale_chrome_launch_lock(&path);
                tokio::time::sleep(Duration::from_millis(250)).await;
            }
            Err(e) => return Err(RenderError::BrowserLaunch(e.to_string())),
        }
    }

    Err(RenderError::BrowserLaunch(
        "timed out waiting for Chrome launch lock".to_string(),
    ))
}

fn remove_stale_chrome_launch_lock(path: &Path) {
    let Ok(metadata) = std::fs::metadata(path) else {
        return;
    };
    let Ok(modified) = metadata.modified() else {
        return;
    };
    let Ok(age) = modified.elapsed() else {
        return;
    };
    if age > Duration::from_secs(300) {
        let _ = std::fs::remove_dir(path);
    }
}
