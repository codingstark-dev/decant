//! Unified browser instance that works for both Chrome and Lightpanda backends.

use crate::backend::BrowserBackend;
use crate::error::RenderError;
use chromiumoxide::{Browser as CBrowser, BrowserConfig};
use futures::StreamExt;

use std::time::Duration;
use tokio::process::Command;

/// The unified browser instance.
pub struct Browser {
    inner: CBrowser,
    backend: BrowserBackend,
    _proc: Option<tokio::process::Child>,
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

                let config = BrowserConfig::builder()
                    .chrome_executable(chrome_bin)
                    .arg("--headless=new")
                    .arg("--disable-gpu")
                    .arg("--no-sandbox")
                    .arg("--disable-setuid-sandbox")
                    .build()
                    .map_err(|e| RenderError::BrowserLaunch(e.to_string()))?;

                let (browser, mut handler) = CBrowser::launch(config)
                    .await
                    .map_err(|e| RenderError::BrowserLaunch(e.to_string()))?;

                tokio::spawn(async move {
                    while let Some(res) = handler.next().await {
                        if let Err(e) = res {
                            tracing::debug!("Chrome handler error: {:?}", e);
                        }
                    }
                });

                Ok(Self {
                    inner: browser,
                    backend,
                    _proc: None,
                })
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
                })
            }
        }
    }

    /// Shutdown the browser and cleanup associated resources.
    pub async fn shutdown(mut self) {
        // Inner browser close
        let _ = self.inner.close().await;

        // If we spawned Lightpanda, kill the process
        if let Some(mut child) = self._proc.take() {
            let _ = child.kill().await;
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
