use std::sync::atomic::{AtomicBool, Ordering};

static SHUTDOWN_REQUESTED: AtomicBool = AtomicBool::new(false);

pub fn requested() -> bool {
    SHUTDOWN_REQUESTED.load(Ordering::SeqCst)
}

pub fn request() {
    SHUTDOWN_REQUESTED.store(true, Ordering::SeqCst);
}

pub fn install_handler() {
    #[cfg(unix)]
    unix_impl::install();

    #[cfg(windows)]
    windows_impl::install();
}

#[cfg(unix)]
mod unix_impl {
    use super::request;

    const SIGINT: i32 = 2;
    const SIGTERM: i32 = 15;

    extern "C" {
        fn signal(signum: i32, handler: extern "C" fn(i32)) -> usize;
    }

    extern "C" fn handle(_signum: i32) {
        request();
    }

    pub fn install() {
        unsafe {
            signal(SIGINT, handle);
            signal(SIGTERM, handle);
        }
    }
}

#[cfg(windows)]
mod windows_impl {
    use super::request;

    #[link(name = "kernel32")]
    extern "system" {
        fn SetConsoleCtrlHandler(handler: extern "system" fn(u32) -> i32, add: i32) -> i32;
    }

    extern "system" fn handle(_ctrl_type: u32) -> i32 {
        request();
        1
    }

    pub fn install() {
        unsafe {
            SetConsoleCtrlHandler(handle, 1);
        }
    }
}
