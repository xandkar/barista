use std::ffi::CString;

use anyhow::{anyhow, bail};
use x11::xlib;

/// Ref: <https://xwindow.angelfire.com/page2.html>
/// Ref: <https://www.oreilly.com/library/view/xlib-reference-manual/9780937175262/14_appendix-f.html>
pub struct X11 {
    display_ptr: *mut xlib::Display,
}

unsafe impl Send for X11 {}

impl X11 {
    pub fn init() -> anyhow::Result<Self> {
        let display_ptr = unsafe { xlib::XOpenDisplay(std::ptr::null()) };
        if display_ptr.is_null() {
            Err(anyhow!("XOpenDisplay failed"))
        } else {
            Ok(Self { display_ptr })
        }
    }

    pub fn set_root_window_name(&self, name: &str) -> anyhow::Result<()> {
        let name = CString::new(name)?;
        let name = name.as_ptr();
        let window = unsafe { xlib::XDefaultRootWindow(self.display_ptr) };
        let ret = unsafe { xlib::XStoreName(self.display_ptr, window, name) };
        if ret < 0 {
            bail!("XStoreName failed: {}", ret);
        };
        let ret = unsafe { xlib::XFlush(self.display_ptr) };
        if ret < 0 {
            bail!("XFlush failed: {}", ret);
        };
        Ok(())
    }
}

impl Drop for X11 {
    fn drop(&mut self) {
        unsafe {
            xlib::XCloseDisplay(self.display_ptr);
        }
    }
}
