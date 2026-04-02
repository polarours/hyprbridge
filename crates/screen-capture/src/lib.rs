//! Screen capture for HyprBridge.

use std::sync::Arc;

use anyhow::Result;
use serde::{Deserialize, Serialize};

/// Capture resolution in pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Resolution {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels,
    pub height: u32,
}

impl Default for Resolution {
    fn default() -> Self {
        Self {
            width: 1920,
            height: 1080,
        }
    }
}

/// Configuration for screen capture.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaptureConfig {
    /// Target frames per second.
    pub fps: u32,
    /// Capture resolution.
    pub resolution: Resolution,
    /// Quality level (0–100).
    pub quality: u32,
    /// Whether to encode captured video.
    pub encode_video: bool,
    /// Whether to capture system audio.
    pub capture_audio: bool,
}

impl Default for CaptureConfig {
    fn default() -> Self {
        Self {
            fps: 30,
            resolution: Resolution::default(),
            quality: 80,
            encode_video: true,
            capture_audio: false,
        }
    }
}

/// A single captured frame.
#[derive(Debug, Clone)]
pub struct CaptureFrame {
    /// Timestamp in microseconds since capture start.
    pub timestamp: u64,
    /// Raw video frame data (RGBA or encoded if encode_video is true).
    pub video_data: Vec<u8>,
    /// Raw audio data (may be empty if audio capture is disabled).
    pub audio_data: Vec<u8>,
    /// Frame width in pixels.
    pub width: u32,
    /// Frame height in pixels.
    pub height: u32,
}

/// Platform-agnostic screen capture interface.
#[async_trait::async_trait]
pub trait ScreenCapture: Send {
    /// Begin capturing the screen with the current configuration.
    async fn start(&mut self) -> Result<()>;

    /// Stop capturing and release resources.
    async fn stop(&mut self) -> Result<()>;

    /// Capture a single frame. Returns `None` if inactive.
    async fn capture_frame(&mut self) -> Result<Option<CaptureFrame>>;

    /// Update the capture configuration.
    fn set_config(&mut self, config: CaptureConfig);

    /// Returns `true` if capture is currently active.
    fn is_active(&self) -> bool;
}

/// Creates a platform-specific screen capture implementation.
pub fn create_capture() -> Arc<dyn ScreenCapture> {
    create_platform_capture()
}

#[cfg(target_os = "linux")]
fn create_platform_capture() -> Arc<dyn ScreenCapture> {
    if std::env::var("WAYLAND_DISPLAY").is_ok() {
        return Arc::new(LinuxWaylandCapture::new());
    }
    if std::env::var("DISPLAY").is_ok() {
        return Arc::new(LinuxX11Capture::new());
    }
    Arc::new(PlatformCapture::new())
}

#[cfg(target_os = "macos")]
fn create_platform_capture() -> Arc<dyn ScreenCapture> {
    Arc::new(MacCapture::new())
}

#[cfg(target_os = "windows")]
fn create_platform_capture() -> Arc<dyn ScreenCapture> {
    Arc::new(WindowsCapture::new())
}

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
fn create_platform_capture() -> Arc<dyn ScreenCapture> {
    Arc::new(PlatformCapture::new())
}

/// Default placeholder screen capture implementation.
pub struct PlatformCapture {
    config: CaptureConfig,
    active: bool,
}

impl PlatformCapture {
    #[must_use]
    pub fn new() -> Self {
        Self {
            config: CaptureConfig::default(),
            active: false,
        }
    }

    #[must_use]
    pub fn with_config(config: CaptureConfig) -> Self {
        Self { config, active: false }
    }
}

impl Default for PlatformCapture {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl ScreenCapture for PlatformCapture {
    async fn start(&mut self) -> Result<()> {
        tracing::info!("Screen capture started (placeholder)");
        self.active = true;
        Ok(())
    }

    async fn stop(&mut self) -> Result<()> {
        tracing::info!("Screen capture stopped (placeholder)");
        self.active = false;
        Ok(())
    }

    async fn capture_frame(&mut self) -> Result<Option<CaptureFrame>> {
        if !self.active {
            return Ok(None);
        }
        Ok(Some(CaptureFrame {
            timestamp: 0,
            video_data: vec![],
            audio_data: vec![],
            width: self.config.resolution.width,
            height: self.config.resolution.height,
        }))
    }

    fn set_config(&mut self, config: CaptureConfig) {
        self.config = config;
    }

    fn is_active(&self) -> bool {
        self.active
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use anyhow::Result;
    use core_graphics::display::{CGDisplay, CGWindowListCopyWindowInfo, CGRect, kCGNullWindowID};
    use std::time::Instant;

    use super::{CaptureConfig, CaptureFrame, ScreenCapture};

    pub struct MacCapture {
        config: CaptureConfig,
        active: bool,
    }

    impl MacCapture {
        #[must_use]
        pub fn new() -> Self {
            Self {
                config: CaptureConfig::default(),
                active: false,
            }
        }
    }

    impl Default for MacCapture {
        fn default() -> Self {
            Self::new()
        }
    }

    #[async_trait::async_trait]
    impl ScreenCapture for MacCapture {
        async fn start(&mut self) -> Result<()> {
            self.active = true;
            Ok(())
        }

        async fn stop(&mut self) -> Result<()> {
            self.active = false;
            Ok(())
        }

        async fn capture_frame(&mut self) -> Result<Option<CaptureFrame>> {
            if !self.active {
                return Ok(None);
            }
            let display = CGDisplay::main();
            let img = display
                .image(0, 0, display.pixels_wide(), display.pixels_high())
                .map_err(|e| anyhow::anyhow!("CGDisplay image capture failed: {e}"))?;
            let width = img.width();
            let height = img.height();
            let data = img.data().to_vec();
            Ok(Some(CaptureFrame {
                timestamp: Instant::now()
                    .duration_since(Instant::now())
                    .as_micros() as u64,
                video_data: data,
                audio_data: vec![],
                width,
                height,
            }))
        }

        fn set_config(&mut self, config: CaptureConfig) {
            self.config = config;
        }

        fn is_active(&self) -> bool {
            self.active
        }
    }
}
#[cfg(target_os = "macos")]
pub use macos::MacCapture;

#[cfg(target_os = "windows")]
mod windows {
    use anyhow::Result;
    use windows::Win32::Foundation::{GetDesktopWindow, HWND};
    use windows::Win32::Graphics::Gdi::{
        BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, GetObjectW,
        GetWindowDC, ReleaseDC, BITMAP, BITMAPINFOHEADER, DC_BITMAP, HBITMAP, hdc,
    };
    use std::time::Instant;

    use super::{CaptureConfig, CaptureFrame, ScreenCapture};

    pub struct WindowsCapture {
        config: CaptureConfig,
        active: bool,
    }

    impl WindowsCapture {
        #[must_use]
        pub fn new() -> Self {
            Self {
                config: CaptureConfig::default(),
                active: false,
            }
        }
    }

    impl Default for WindowsCapture {
        fn default() -> Self {
            Self::new()
        }
    }

    #[async_trait::async_trait]
    impl ScreenCapture for WindowsCapture {
        async fn start(&mut self) -> Result<()> {
            self.active = true;
            Ok(())
        }

        async fn stop(&mut self) -> Result<()> {
            self.active = false;
            Ok(())
        }

        async fn capture_frame(&mut self) -> Result<Option<CaptureFrame>> {
            if !self.active {
                return Ok(None);
            }
            unsafe {
                let desktop = GetDesktopWindow();
                let hdc_src = GetWindowDC(desktop);
                if hdc_src.is_invalid() {
                    return Ok(None);
                }

                let width = windows::Win32::UI::WindowsAndMessaging::GetSystemMetrics(
                    windows::Win32::UI::WindowsAndMessaging::SM_CXSCREEN,
                );
                let height = windows::Win32::UI::WindowsAndMessaging::GetSystemMetrics(
                    windows::Win32::UI::WindowsAndMessaging::SM_CYSCREEN,
                );

                let hdc_mem = CreateCompatibleDC(hdc_src);
                if hdc_mem.is_invalid() {
                    let _ = ReleaseDC(desktop, hdc_src);
                    return Ok(None);
                }

                let hbm = CreateCompatibleBitmap(hdc_src, width, height);
                if hbm.is_invalid() {
                    DeleteDC(hdc_mem);
                    let _ = ReleaseDC(desktop, hdc_src);
                    return Ok(None);
                }

                let hbm_old = windows::core::pointer::Pointer::null();
                let _ = windows::core::interface::Interface::cast::<windows::Win32::Graphics::Gdi::IGdiObject>(&hbm);

                let _ = BitBlt(
                    hdc_mem,
                    0,
                    0,
                    width,
                    height,
                    hdc_src,
                    0,
                    0,
                    windows::Win32::Graphics::Gdi::SRCCOPY,
                );

                let mut bm = BITMAP::default();
                let _ = GetObjectW(hbm.0, std::mem::size_of::<BITMAP>() as i32, &mut bm as *mut _ as *mut core::ffi::c_void);

                let bytes_per_pixel = 4u32;
                let stride = bm.bmWidth * bytes_per_pixel;
                let mut bits = vec![0u8; (bm.bmHeight as usize) * stride as usize];
                let _ = BitBlt(
                    hdc_mem,
                    0,
                    0,
                    bm.bmWidth,
                    bm.bmHeight,
                    hdc_src,
                    0,
                    0,
                    windows::Win32::Graphics::Gdi::SRCCOPY,
                );

                DeleteObject(hbm.0);
                DeleteDC(hdc_mem);
                let _ = ReleaseDC(desktop, hdc_src);

                Ok(Some(CaptureFrame {
                    timestamp: Instant::now()
                        .duration_since(Instant::now())
                        .as_micros() as u64,
                    video_data: bits,
                    audio_data: vec![],
                    width: bm.bmWidth as u32,
                    height: bm.bmHeight as u32,
                }))
            }
        }

        fn set_config(&mut self, config: CaptureConfig) {
            self.config = config;
        }

        fn is_active(&self) -> bool {
            self.active
        }
    }
}
#[cfg(target_os = "windows")]
pub use windows::WindowsCapture;

#[cfg(target_os = "linux")]
mod linux {
    use anyhow::Result;
    use std::time::Instant;

    use super::{CaptureConfig, CaptureFrame, ScreenCapture};

    pub struct LinuxX11Capture {
        config: CaptureConfig,
        active: bool,
    }

    impl LinuxX11Capture {
        #[must_use]
        pub fn new() -> Self {
            Self {
                config: CaptureConfig::default(),
                active: false,
            }
        }
    }

    impl Default for LinuxX11Capture {
        fn default() -> Self {
            Self::new()
        }
    }

    #[async_trait::async_trait]
    impl ScreenCapture for LinuxX11Capture {
        async fn start(&mut self) -> Result<()> {
            self.active = true;
            Ok(())
        }

        async fn stop(&mut self) -> Result<()> {
            self.active = false;
            Ok(())
        }

        async fn capture_frame(&mut self) -> Result<Option<CaptureFrame>> {
            if !self.active {
                return Ok(None);
            }
            #[cfg(feature = "x11")]
            {
                use x11rb::connection::Connection;
                use x11rb::protocol::xproto::ConnectionExt;

                let (conn, screen_num) = x11rb::rust_connection::RustConnection::connect(None)?;
                let screen = conn
                    .setup()
                    .roots
                    .get(screen_num)
                    .ok_or_else(|| anyhow::anyhow!("X11 root screen not available"))?;
                let width = screen.width_in_pixels as u32;
                let height = screen.height_in_pixels as u32;

                let reply = conn
                    .get_image(
                        x11rb::protocol::xproto::IMAGE_FORMAT_Z_PIXMAP,
                        screen.root,
                        0,
                        0,
                        width,
                        height,
                        u32::MAX,
                    )
                    .map_err(|e| anyhow::anyhow!("X11 get_image failed: {e}"))?
                    .reply()
                    .map_err(|e| anyhow::anyhow!("X11 get_image reply failed: {e}"))?;

                Ok(Some(CaptureFrame {
                    timestamp: Instant::now()
                        .duration_since(Instant::now())
                        .as_micros() as u64,
                    video_data: reply.data,
                    audio_data: vec![],
                    width,
                    height,
                }))
            }
            #[cfg(not(feature = "x11"))]
            {
                let _ = Instant::now();
                Ok(Some(CaptureFrame {
                    timestamp: 0,
                    video_data: vec![],
                    audio_data: vec![],
                    width: self.config.resolution.width,
                    height: self.config.resolution.height,
                }))
            }
        }

        fn set_config(&mut self, config: CaptureConfig) {
            self.config = config;
        }

        fn is_active(&self) -> bool {
            self.active
        }
    }

    pub struct LinuxWaylandCapture {
        config: CaptureConfig,
        active: bool,
    }

    impl LinuxWaylandCapture {
        #[must_use]
        pub fn new() -> Self {
            Self {
                config: CaptureConfig::default(),
                active: false,
            }
        }
    }

    impl Default for LinuxWaylandCapture {
        fn default() -> Self {
            Self::new()
        }
    }

    #[async_trait::async_trait]
    impl ScreenCapture for LinuxWaylandCapture {
        async fn start(&mut self) -> Result<()> {
            self.active = true;
            Ok(())
        }

        async fn stop(&mut self) -> Result<()> {
            self.active = false;
            Ok(())
        }

        async fn capture_frame(&mut self) -> Result<Option<CaptureFrame>> {
            if !self.active {
                return Ok(None);
            }
            Ok(Some(CaptureFrame {
                timestamp: 0,
                video_data: vec![],
                audio_data: vec![],
                width: self.config.resolution.width,
                height: self.config.resolution.height,
            }))
        }

        fn set_config(&mut self, config: CaptureConfig) {
            self.config = config;
        }

        fn is_active(&self) -> bool {
            self.active
        }
    }
}
#[cfg(target_os = "linux")]
pub use linux::{LinuxWaylandCapture, LinuxX11Capture};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_resolution() {
        let res = Resolution::default();
        assert_eq!(res.width, 1920);
        assert_eq!(res.height, 1080);
    }

    #[test]
    fn default_config() {
        let config = CaptureConfig::default();
        assert_eq!(config.fps, 30);
        assert_eq!(config.quality, 80);
        assert!(config.encode_video);
        assert!(!config.capture_audio);
    }

    #[tokio::test]
    async fn lifecycle() {
        let mut cap = PlatformCapture::new();
        assert!(!cap.is_active());

        cap.start().await.expect("start should succeed");
        assert!(cap.is_active());

        cap.stop().await.expect("stop should succeed");
        assert!(!cap.is_active());
    }

    #[tokio::test]
    async fn capture_returns_none_when_inactive() {
        let mut cap = PlatformCapture::new();
        let frame = cap.capture_frame().await.expect("capture should succeed");
        assert!(frame.is_none());
    }

    #[tokio::test]
    async fn capture_returns_frame_when_active() {
        let mut cap = PlatformCapture::new();
        cap.start().await.expect("start should succeed");

        let frame = cap
            .capture_frame()
            .await
            .expect("capture should succeed")
            .expect("frame should be Some when active");

        assert_eq!(frame.width, 1920);
        assert_eq!(frame.height, 1080);
    }
}
