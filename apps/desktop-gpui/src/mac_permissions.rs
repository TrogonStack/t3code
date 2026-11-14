//! The two macOS permissions SnapShots needs, read and requested through the system frameworks
//! (`DesktopSnapShot.ts`: `currentMacPermissions`, `requestMacScreenCapturePermission`).

pub const SCREEN_RECORDING_URL: &str =
    "x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture";
pub const ACCESSIBILITY_URL: &str =
    "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MacPermissions {
    pub screen_recording: bool,
    pub accessibility: bool,
}

#[cfg(target_os = "macos")]
mod ffi {
    #[link(name = "CoreGraphics", kind = "framework")]
    unsafe extern "C" {
        pub fn CGPreflightScreenCaptureAccess() -> bool;
        pub fn CGRequestScreenCaptureAccess() -> bool;
    }

    #[link(name = "ApplicationServices", kind = "framework")]
    unsafe extern "C" {
        pub fn AXIsProcessTrusted() -> bool;
    }
}

/// `None` where the platform has no such permissions.
pub fn current() -> Option<MacPermissions> {
    #[cfg(target_os = "macos")]
    {
        // SAFETY: both calls take no arguments and only read the process's permission state.
        let (screen_recording, accessibility) = unsafe {
            (
                ffi::CGPreflightScreenCaptureAccess(),
                ffi::AXIsProcessTrusted(),
            )
        };
        Some(MacPermissions {
            screen_recording,
            accessibility,
        })
    }
    #[cfg(not(target_os = "macos"))]
    None
}

/// Shows the system's Screen Recording prompt the first time; true when already allowed.
pub fn request_screen_recording() -> bool {
    #[cfg(target_os = "macos")]
    {
        // SAFETY: no arguments; the call may show the system prompt.
        unsafe { ffi::CGRequestScreenCaptureAccess() }
    }
    #[cfg(not(target_os = "macos"))]
    false
}
