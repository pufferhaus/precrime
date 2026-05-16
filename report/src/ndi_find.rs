//! Minimal FFI wrapper around libndi's NDIlib_find_* API for source discovery.
//!
//! We don't wrap the full NDI SDK — `gst-plugin-rs`'s `ndisrc` handles streaming.
//! This module only enumerates sources visible on the LAN.

use anyhow::{anyhow, Result};
use std::ffi::CStr;
use std::os::raw::{c_char, c_void};
use std::time::Duration;

type NdiFindInstance = *mut c_void;

#[repr(C)]
struct NdiSource {
    p_ndi_name: *const c_char,
    p_url_address: *const c_char,
}

// Linked dynamically — libndi.so must be in the library path at runtime.
extern "C" {
    fn NDIlib_initialize() -> bool;
    fn NDIlib_find_create_v2(p_create: *const c_void) -> NdiFindInstance;
    fn NDIlib_find_destroy(p_instance: NdiFindInstance);
    fn NDIlib_find_wait_for_sources(p_instance: NdiFindInstance, timeout_ms: u32) -> bool;
    fn NDIlib_find_get_current_sources(
        p_instance: NdiFindInstance,
        p_no_sources: *mut u32,
    ) -> *const NdiSource;
}

pub struct Discovery {
    handle: NdiFindInstance,
}

// SAFETY: NDI Find instances are thread-safe per the SDK docs; we only use them
// from a single discovery thread anyway.
unsafe impl Send for Discovery {}

impl Discovery {
    pub fn new() -> Result<Self> {
        // SAFETY: NDIlib_initialize is the first call into the NDI library;
        // safe to invoke once at process start.
        let ok = unsafe { NDIlib_initialize() };
        if !ok {
            return Err(anyhow!("NDIlib_initialize failed (no system CPU support?)"));
        }
        // SAFETY: passing null for default settings is documented in the NDI SDK.
        let handle = unsafe { NDIlib_find_create_v2(std::ptr::null()) };
        if handle.is_null() {
            return Err(anyhow!("NDIlib_find_create_v2 returned null"));
        }
        Ok(Self { handle })
    }

    /// Block up to `timeout` waiting for the source list to change. Returns the
    /// current source names afterward (regardless of whether they changed).
    pub fn poll(&self, timeout: Duration) -> Vec<String> {
        let ms: u32 = timeout.as_millis().min(u32::MAX as u128) as u32;
        // SAFETY: handle is non-null (constructed in `new`), timeout is a primitive.
        unsafe {
            NDIlib_find_wait_for_sources(self.handle, ms);
        }
        self.current_sources()
    }

    fn current_sources(&self) -> Vec<String> {
        let mut count: u32 = 0;
        // SAFETY: handle is non-null; out-pointer points to local `count`.
        let ptr = unsafe { NDIlib_find_get_current_sources(self.handle, &mut count) };
        if ptr.is_null() || count == 0 {
            return Vec::new();
        }
        let mut out = Vec::with_capacity(count as usize);
        for i in 0..count as usize {
            // SAFETY: NDI guarantees the array is valid for `count` entries
            // until the next call into the find API.
            let src = unsafe { &*ptr.add(i) };
            if src.p_ndi_name.is_null() {
                continue;
            }
            // SAFETY: NDI provides null-terminated UTF-8.
            let cstr = unsafe { CStr::from_ptr(src.p_ndi_name) };
            if let Ok(s) = cstr.to_str() {
                out.push(s.to_owned());
            }
        }
        out
    }
}

impl Drop for Discovery {
    fn drop(&mut self) {
        if !self.handle.is_null() {
            // SAFETY: handle was created by NDIlib_find_create_v2 and not freed yet.
            unsafe { NDIlib_find_destroy(self.handle) };
        }
    }
}
