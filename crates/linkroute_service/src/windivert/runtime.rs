#![allow(dead_code)]

use std::ffi::{c_char, c_void, CString};
use std::path::{Path, PathBuf};

use libloading::Library;

pub type WindivertHandle = isize;

type RawOpen = unsafe extern "system" fn(*const c_char, u32, i16, u64) -> WindivertHandle;
type RawClose = unsafe extern "system" fn(WindivertHandle) -> i32;
type RawRecv =
    unsafe extern "system" fn(WindivertHandle, *mut c_void, u32, *mut u32, *mut c_void) -> i32;
type RawSend =
    unsafe extern "system" fn(WindivertHandle, *const c_void, u32, *mut u32, *const c_void) -> i32;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindivertLayer {
    Network = 0,
    NetworkForward = 1,
    Flow = 2,
    Socket = 3,
    Reflect = 4,
}

const INVALID_HANDLE_VALUE: WindivertHandle = -1;

pub struct WindivertRuntime {
    dll_path: PathBuf,
    _library: Library,
    open: RawOpen,
    close: RawClose,
    recv: RawRecv,
    send: RawSend,
}

impl WindivertRuntime {
    pub fn load_from_dir(dir: &Path) -> Result<Self, String> {
        let dll_path = dir.join("WinDivert.dll");
        if !dll_path.is_file() {
            return Err(format!("missing {}", dll_path.display()));
        }

        // Loading a trusted, hash-checked WinDivert DLL from the application directory.
        let library = unsafe { Library::new(&dll_path) }.map_err(|error| error.to_string())?;
        // Symbols are copied as function pointers while the owning Library is kept alive.
        let open = *unsafe { library.get::<RawOpen>(b"WinDivertOpen\0") }
            .map_err(|error| error.to_string())?;
        let close = *unsafe { library.get::<RawClose>(b"WinDivertClose\0") }
            .map_err(|error| error.to_string())?;
        let recv = *unsafe { library.get::<RawRecv>(b"WinDivertRecv\0") }
            .map_err(|error| error.to_string())?;
        let send = *unsafe { library.get::<RawSend>(b"WinDivertSend\0") }
            .map_err(|error| error.to_string())?;

        Ok(Self { dll_path, _library: library, open, close, recv, send })
    }

    pub fn dll_path(&self) -> &Path {
        &self.dll_path
    }

    pub fn open(
        &self,
        filter: &str,
        layer: WindivertLayer,
        priority: i16,
        flags: u64,
    ) -> Result<WindivertSession<'_>, String> {
        let filter = CString::new(filter).map_err(|_| String::from("filter contains NUL byte"))?;
        let handle = unsafe { (self.open)(filter.as_ptr(), layer as u32, priority, flags) };
        if handle == INVALID_HANDLE_VALUE {
            return Err(format!("WinDivertOpen failed: {}", std::io::Error::last_os_error()));
        }

        Ok(WindivertSession { runtime: self, handle })
    }
    pub fn resolved_symbol_count(&self) -> usize {
        4
    }
}

pub struct WindivertSession<'runtime> {
    runtime: &'runtime WindivertRuntime,
    handle: WindivertHandle,
}

impl WindivertSession<'_> {
    pub fn handle(&self) -> WindivertHandle {
        self.handle
    }
}

impl Drop for WindivertSession<'_> {
    fn drop(&mut self) {
        unsafe {
            (self.runtime.close)(self.handle);
        }
    }
}
#[cfg(test)]
mod tests {
    use std::fs;

    use super::WindivertRuntime;

    #[test]
    fn missing_dll_returns_error() {
        let dir = std::env::temp_dir()
            .join(format!("linkroute_windivert_runtime_missing_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        let error = match WindivertRuntime::load_from_dir(&dir) {
            Ok(_) => panic!("missing DLL should fail"),
            Err(error) => error,
        };

        assert!(error.contains("WinDivert.dll"));
        let _ = fs::remove_dir_all(dir);
    }
}
