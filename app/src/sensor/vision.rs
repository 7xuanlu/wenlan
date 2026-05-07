// SPDX-License-Identifier: AGPL-3.0-only
use crate::error::AppError;

/// Metadata + image for a captured window (used by frame_compare for hash).
pub struct WindowCapture {
    pub image: image::DynamicImage,
    pub app_name: String,
    pub window_name: String,
    pub focused: bool,
}

/// A single text observation from Apple Vision OCR.
#[derive(Debug, Clone)]
pub struct TextObservation {
    pub text: String,
    pub confidence: f64,
}

/// OCR result for a single window / region.
#[derive(Debug, Clone)]
pub struct WindowOcrResult {
    pub window_name: String,
    pub app_name: String,
    pub text: String,
    pub observations: Vec<TextObservation>,
    pub focused: bool,
    #[allow(dead_code)]
    pub confidence: f64,
}

// ── Window listing via xcap (for metadata + hash images) ──────────────────

/// Capture all visible windows via xcap. Returns images for frame_compare hash
/// and window metadata (app names, focused state). OCR is done separately via
/// native CoreGraphics for maximum quality.
pub fn capture_windows(
    skip_apps: &[String],
    skip_title_patterns: &[String],
    private_browsing_detection: bool,
) -> Result<Vec<WindowCapture>, AppError> {
    use xcap::Window;

    let windows = Window::all().map_err(|e| AppError::Vision(format!("xcap list: {e}")))?;

    let mut captures = Vec::new();
    for win in &windows {
        if win.is_minimized().unwrap_or(false) {
            continue;
        }
        let width = win.width().unwrap_or(0);
        let height = win.height().unwrap_or(0);
        if width < 10 || height < 10 {
            continue;
        }

        let title = win.title().unwrap_or_default();
        let app = win.app_name().unwrap_or_default();

        if should_skip_window(
            &app,
            &title,
            skip_apps,
            skip_title_patterns,
            private_browsing_detection,
        ) {
            continue;
        }

        let img = match win.capture_image() {
            Ok(buf) => image::DynamicImage::ImageRgba8(
                image::ImageBuffer::from_raw(buf.width(), buf.height(), buf.into_raw())
                    .ok_or_else(|| AppError::Vision("Failed to create image buffer".to_string()))?,
            ),
            Err(e) => {
                log::debug!("[vision] capture failed for \"{}\": {}", title, e);
                continue;
            }
        };

        captures.push(WindowCapture {
            image: img,
            app_name: app,
            window_name: title,
            focused: win.is_focused().unwrap_or(false),
        });
    }

    Ok(captures)
}

// ── Per-Window OCR pipeline ───────────────────────────────────────────────
//
// Each window's DynamicImage → grayscale luma8 → CGImage via CGBitmapContext
// → VNRecognizeTextRequest → TextObservation (text + confidence).
// Screenpipe pattern: per-window attribution, but we fire on trigger events
// instead of 1/sec.

/// Run Apple Vision OCR on each window capture, returning per-window results
/// with individual text observations.
#[cfg(target_os = "macos")]
pub fn ocr_per_window(captures: &[WindowCapture]) -> Result<Vec<WindowOcrResult>, AppError> {
    let mut results = Vec::new();

    for capture in captures {
        let cg_image = unsafe { dynamic_image_to_gray_cgimage(&capture.image) };
        if cg_image.is_null() {
            log::debug!(
                "[vision] grayscale conversion failed for \"{}\"",
                capture.window_name
            );
            continue;
        }

        let observations = unsafe { run_vision_ocr(cg_image) };
        unsafe { cg::CFRelease(cg_image) };

        let observations = match observations {
            Some(obs) if !obs.is_empty() => obs,
            _ => continue,
        };

        let text: String = observations
            .iter()
            .map(|o| o.text.as_str())
            .collect::<Vec<_>>()
            .join("\n");

        if text.trim().len() < 10 {
            continue;
        }

        let total_confidence: f64 = observations.iter().map(|o| o.confidence).sum();
        let avg_confidence = total_confidence / observations.len() as f64;

        results.push(WindowOcrResult {
            window_name: capture.window_name.clone(),
            app_name: capture.app_name.clone(),
            text,
            observations,
            focused: capture.focused,
            confidence: avg_confidence,
        });
    }

    log::info!(
        "[vision] per-window OCR: {} windows with text",
        results.len()
    );

    Ok(results)
}

#[cfg(not(target_os = "macos"))]
pub fn ocr_per_window(_captures: &[WindowCapture]) -> Result<Vec<WindowOcrResult>, AppError> {
    Ok(vec![])
}

// ── macOS CoreGraphics FFI ─────────────────────────────────────────────────

#[cfg(target_os = "macos")]
mod cg {
    use std::ffi::c_void;

    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        pub fn CGColorSpaceCreateDeviceGray() -> *const c_void;
        pub fn CGBitmapContextCreate(
            data: *mut c_void,
            width: usize,
            height: usize,
            bits_per_component: usize,
            bytes_per_row: usize,
            space: *const c_void,
            bitmap_info: u32,
        ) -> *const c_void;
        pub fn CGBitmapContextCreateImage(ctx: *const c_void) -> *const c_void;
        pub fn CGContextRelease(ctx: *const c_void);
        pub fn CGColorSpaceRelease(space: *const c_void);

        pub fn CFRelease(cf: *const c_void);
    }
}

/// Convert a DynamicImage to a grayscale CGImage for Apple Vision OCR.
/// DynamicImage → grayscale luma8 → raw bytes → CGBitmapContext → CGImage.
/// Following Screenpipe's grayscale-first pattern for better OCR accuracy.
#[cfg(target_os = "macos")]
unsafe fn dynamic_image_to_gray_cgimage(img: &image::DynamicImage) -> *const std::ffi::c_void {
    let gray = img.grayscale().to_luma8();
    let (width, height) = gray.dimensions();

    if width == 0 || height == 0 {
        return std::ptr::null();
    }

    let raw = gray.into_raw();

    let gray_space = cg::CGColorSpaceCreateDeviceGray();
    // kCGImageAlphaNone = 0 for grayscale
    let ctx = cg::CGBitmapContextCreate(
        raw.as_ptr() as *mut std::ffi::c_void,
        width as usize,
        height as usize,
        8,
        width as usize, // 1 byte per pixel
        gray_space,
        0,
    );

    if ctx.is_null() {
        cg::CGColorSpaceRelease(gray_space);
        return std::ptr::null();
    }

    let cg_image = cg::CGBitmapContextCreateImage(ctx);

    cg::CGContextRelease(ctx);
    cg::CGColorSpaceRelease(gray_space);

    cg_image
}

// ── Apple Vision OCR (objc FFI) ────────────────────────────────────────────
//
// Runs VNRecognizeTextRequest on a CGImage.
//
// Following Screenpipe patterns:
// - Grayscale preprocessing for faster/better OCR
// - No confidence filtering at OCR level (return everything)
// - Proper autorelease pool management
// - Language correction enabled for better accuracy

/// Run VNRecognizeTextRequest on a native CGImage.
/// Returns Vec<TextObservation> with text + confidence.
#[cfg(target_os = "macos")]
#[allow(deprecated)]
unsafe fn run_vision_ocr(cg_image: *const std::ffi::c_void) -> Option<Vec<TextObservation>> {
    use cocoa::base::{id, nil};

    let pool: id = msg_send![objc::runtime::Class::get("NSAutoreleasePool")?, new];

    let handler_cls = objc::runtime::Class::get("VNImageRequestHandler")?;
    let handler: id = msg_send![handler_cls, alloc];
    let empty_dict: id = msg_send![objc::runtime::Class::get("NSDictionary")?, dictionary];
    let handler: id = msg_send![handler, initWithCGImage: cg_image as id options: empty_dict];

    if handler == nil {
        let _: () = msg_send![pool, drain];
        return None;
    }

    let request_cls = objc::runtime::Class::get("VNRecognizeTextRequest")?;
    let request: id = msg_send![request_cls, alloc];
    let request: id = msg_send![request, init];
    // Screenpipe disables language correction for speed; we enable it for
    // accuracy since we fire much less frequently than their 1/sec rate.
    let _: () = msg_send![request, setUsesLanguageCorrection: true];
    let _: () = msg_send![request, setAutomaticallyDetectsLanguage: true];
    let _: () = msg_send![request, setMinimumTextHeight: 0.01_f32];

    let array_cls = objc::runtime::Class::get("NSArray")?;
    let requests: id = msg_send![array_cls, arrayWithObject: request];
    let mut error: id = nil;
    let success: bool = msg_send![handler, performRequests: requests error: &mut error];

    if !success {
        if error != nil {
            let desc: id = msg_send![error, localizedDescription];
            if let Some(msg) = ns_string_to_rust(desc) {
                log::warn!("[vision] OCR failed: {}", msg);
            }
        }
        let _: () = msg_send![pool, drain];
        return None;
    }

    let results: id = msg_send![request, results];
    if results == nil {
        let _: () = msg_send![pool, drain];
        return None;
    }
    let count: usize = msg_send![results, count];
    if count == 0 {
        let _: () = msg_send![pool, drain];
        return None;
    }

    let mut observations = Vec::with_capacity(count);

    for i in 0..count {
        let observation: id = msg_send![results, objectAtIndex: i];

        let candidates: id = msg_send![observation, topCandidates: 1_usize];
        let n: usize = msg_send![candidates, count];
        if n > 0 {
            let candidate: id = msg_send![candidates, objectAtIndex: 0_usize];
            let confidence: f32 = msg_send![candidate, confidence];

            let text_ns: id = msg_send![candidate, string];
            if let Some(text) = ns_string_to_rust(text_ns) {
                if !text.is_empty() {
                    observations.push(TextObservation {
                        text,
                        confidence: confidence as f64,
                    });
                }
            }
        }
    }

    let _: () = msg_send![pool, drain];

    if observations.is_empty() {
        return None;
    }

    log::info!(
        "[vision] OCR: {} observations extracted",
        observations.len()
    );

    Some(observations)
}

#[cfg(target_os = "macos")]
#[allow(deprecated)]
unsafe fn ns_string_to_rust(ns_str: cocoa::base::id) -> Option<String> {
    use cocoa::base::nil;
    use std::ffi::CStr;
    use std::os::raw::c_char;

    if ns_str == nil {
        return None;
    }
    let bytes: *const c_char = msg_send![ns_str, UTF8String];
    if bytes.is_null() {
        return None;
    }
    CStr::from_ptr(bytes).to_str().ok().map(|s| s.to_owned())
}

// ── Utilities ──────────────────────────────────────────────────────────────

/// Private browsing window title patterns.
const PRIVATE_BROWSING_PATTERNS: &[&str] = &[
    "Private Browsing", // Safari
    "Incognito",        // Chrome
    "[Private]",        // Firefox
    "[InPrivate]",      // Edge
];

/// Check if a window should be skipped based on app name, title, and config.
pub fn should_skip_window(
    app_name: &str,
    window_title: &str,
    skip_apps: &[String],
    skip_title_patterns: &[String],
    private_browsing_detection: bool,
) -> bool {
    // App name match (substring)
    if skip_apps.iter().any(|s| app_name.contains(s.as_str())) {
        return true;
    }

    let title_lower = window_title.to_lowercase();

    // Private browsing auto-detection
    if private_browsing_detection
        && PRIVATE_BROWSING_PATTERNS
            .iter()
            .any(|p| title_lower.contains(&p.to_lowercase()))
    {
        return true;
    }

    // User-defined title patterns (case-insensitive substring)
    if skip_title_patterns
        .iter()
        .any(|p| title_lower.contains(&p.to_lowercase()))
    {
        return true;
    }

    false
}
