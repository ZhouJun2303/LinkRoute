#[cfg(windows)]
mod platform {
    use std::ffi::c_void;
    use std::mem::size_of;
    use std::ptr;
    use std::thread;
    use std::time::{Duration, Instant};

    use linkroute_core::{
        request_from_json, response_to_json, IpcError, IpcRequest, IpcResponse, IPC_PIPE_NAME,
    };

    const BUFFER_SIZE: usize = 64 * 1024;
    const INVALID_HANDLE_VALUE: isize = -1;
    const PIPE_ACCESS_DUPLEX: u32 = 0x00000003;
    const PIPE_TYPE_MESSAGE: u32 = 0x00000004;
    const PIPE_READMODE_MESSAGE: u32 = 0x00000002;
    const PIPE_NOWAIT: u32 = 0x00000001;
    const PIPE_UNLIMITED_INSTANCES: u32 = 255;
    const ERROR_PIPE_CONNECTED: u32 = 535;
    const ERROR_PIPE_LISTENING: u32 = 536;
    const ERROR_NO_DATA: u32 = 232;
    const ERROR_MORE_DATA: u32 = 234;
    const SDDL_REVISION_1: u32 = 1;

    #[repr(C)]
    struct SecurityAttributes {
        length: u32,
        security_descriptor: *mut c_void,
        inherit_handle: i32,
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        #[link_name = "CreateNamedPipeW"]
        fn create_named_pipe_w(
            name: *const u16,
            open_mode: u32,
            pipe_mode: u32,
            max_instances: u32,
            out_buffer_size: u32,
            in_buffer_size: u32,
            default_timeout: u32,
            security_attributes: *const SecurityAttributes,
        ) -> isize;
        #[link_name = "ConnectNamedPipe"]
        fn connect_named_pipe(handle: isize, overlapped: *mut c_void) -> i32;
        #[link_name = "DisconnectNamedPipe"]
        fn disconnect_named_pipe(handle: isize) -> i32;
        #[link_name = "ReadFile"]
        fn read_file(
            handle: isize,
            buffer: *mut c_void,
            bytes_to_read: u32,
            bytes_read: *mut u32,
            overlapped: *mut c_void,
        ) -> i32;
        #[link_name = "WriteFile"]
        fn write_file(
            handle: isize,
            buffer: *const c_void,
            bytes_to_write: u32,
            bytes_written: *mut u32,
            overlapped: *mut c_void,
        ) -> i32;
        #[link_name = "CloseHandle"]
        fn close_handle(handle: isize) -> i32;
        #[link_name = "GetLastError"]
        fn get_last_error() -> u32;
        #[link_name = "LocalFree"]
        fn local_free(memory: *mut c_void) -> *mut c_void;
    }

    #[link(name = "advapi32")]
    unsafe extern "system" {
        #[link_name = "ConvertStringSecurityDescriptorToSecurityDescriptorW"]
        fn convert_string_security_descriptor_to_security_descriptor_w(
            string_security_descriptor: *const u16,
            string_sddl_revision: u32,
            security_descriptor: *mut *mut c_void,
            security_descriptor_size: *mut u32,
        ) -> i32;
    }

    pub fn serve_once(
        timeout: Duration,
        handler: impl Fn(IpcRequest) -> IpcResponse,
    ) -> Result<bool, String> {
        let security_descriptor = SecurityDescriptor::new()?;
        let security_attributes = SecurityAttributes {
            length: size_of::<SecurityAttributes>() as u32,
            security_descriptor: security_descriptor.as_ptr(),
            inherit_handle: 0,
        };
        let pipe_name = wide(IPC_PIPE_NAME);
        let handle = unsafe {
            create_named_pipe_w(
                pipe_name.as_ptr(),
                PIPE_ACCESS_DUPLEX,
                PIPE_TYPE_MESSAGE | PIPE_READMODE_MESSAGE | PIPE_NOWAIT,
                PIPE_UNLIMITED_INSTANCES,
                BUFFER_SIZE as u32,
                BUFFER_SIZE as u32,
                100,
                &security_attributes,
            )
        };
        if handle == INVALID_HANDLE_VALUE {
            return Err(last_os_error("create control named pipe"));
        }
        let pipe = PipeHandle(handle);

        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            if try_connect(pipe.0)? {
                handle_client(pipe.0, handler)?;
                return Ok(true);
            }
            thread::sleep(Duration::from_millis(20));
        }

        Ok(false)
    }

    fn try_connect(handle: isize) -> Result<bool, String> {
        if unsafe { connect_named_pipe(handle, ptr::null_mut()) } != 0 {
            return Ok(true);
        }

        match unsafe { get_last_error() } {
            ERROR_PIPE_CONNECTED => Ok(true),
            ERROR_PIPE_LISTENING | ERROR_NO_DATA => Ok(false),
            error => Err(format!("could not connect control named pipe: Windows error {error}")),
        }
    }

    fn handle_client(
        handle: isize,
        handler: impl Fn(IpcRequest) -> IpcResponse,
    ) -> Result<(), String> {
        let request_text = read_message(handle)?;
        let response = match request_from_json(&request_text) {
            Ok(request) => handler(request),
            Err(error) => IpcResponse::Error {
                error: IpcError::new("invalid-request-json", error.to_string()),
            },
        };
        let response_text = response_to_json(&response).map_err(|error| error.to_string())?;
        write_message(handle, response_text.as_bytes())?;
        unsafe {
            disconnect_named_pipe(handle);
        }
        Ok(())
    }

    fn read_message(handle: isize) -> Result<String, String> {
        let deadline = Instant::now() + Duration::from_secs(2);
        let mut data = Vec::new();
        loop {
            let mut buffer = [0_u8; BUFFER_SIZE];
            let mut read = 0_u32;
            if unsafe {
                read_file(
                    handle,
                    buffer.as_mut_ptr().cast(),
                    buffer.len() as u32,
                    &mut read,
                    ptr::null_mut(),
                )
            } != 0
            {
                data.extend_from_slice(&buffer[..read as usize]);
                break;
            }

            match unsafe { get_last_error() } {
                ERROR_MORE_DATA => data.extend_from_slice(&buffer[..read as usize]),
                ERROR_NO_DATA if Instant::now() < deadline => {
                    thread::sleep(Duration::from_millis(10));
                }
                ERROR_NO_DATA => return Err(String::from("timed out waiting for pipe request")),
                error => return Err(format!("could not read control pipe: Windows error {error}")),
            }
        }

        String::from_utf8(data).map_err(|error| error.to_string())
    }

    fn write_message(handle: isize, message: &[u8]) -> Result<(), String> {
        let mut written = 0_u32;
        if unsafe {
            write_file(
                handle,
                message.as_ptr().cast(),
                message.len() as u32,
                &mut written,
                ptr::null_mut(),
            )
        } == 0
        {
            return Err(last_os_error("write control pipe response"));
        }
        if written as usize != message.len() {
            return Err(format!("short control pipe write: {written} of {} bytes", message.len()));
        }
        Ok(())
    }

    struct PipeHandle(isize);

    impl Drop for PipeHandle {
        fn drop(&mut self) {
            unsafe {
                close_handle(self.0);
            }
        }
    }

    struct SecurityDescriptor(*mut c_void);

    impl SecurityDescriptor {
        fn new() -> Result<Self, String> {
            let sddl = wide("D:P(A;;GA;;;SY)(A;;GA;;;BA)(A;;GRGW;;;IU)");
            let mut descriptor = ptr::null_mut();
            if unsafe {
                convert_string_security_descriptor_to_security_descriptor_w(
                    sddl.as_ptr(),
                    SDDL_REVISION_1,
                    &mut descriptor,
                    ptr::null_mut(),
                )
            } == 0
            {
                return Err(last_os_error("build control pipe security descriptor"));
            }
            Ok(Self(descriptor))
        }

        fn as_ptr(&self) -> *mut c_void {
            self.0
        }
    }

    impl Drop for SecurityDescriptor {
        fn drop(&mut self) {
            if !self.0.is_null() {
                unsafe {
                    local_free(self.0);
                }
            }
        }
    }

    fn wide(text: &str) -> Vec<u16> {
        text.encode_utf16().chain(std::iter::once(0)).collect()
    }

    fn last_os_error(action: &str) -> String {
        format!("could not {action}: {}", std::io::Error::last_os_error())
    }
}

#[cfg(not(windows))]
mod platform {
    use std::time::Duration;

    use linkroute_core::{IpcRequest, IpcResponse};

    pub fn serve_once(
        _timeout: Duration,
        _handler: impl Fn(IpcRequest) -> IpcResponse,
    ) -> Result<bool, String> {
        Ok(false)
    }
}

pub fn serve_once(
    timeout: std::time::Duration,
    handler: impl Fn(linkroute_core::IpcRequest) -> linkroute_core::IpcResponse,
) -> Result<bool, String> {
    platform::serve_once(timeout, handler)
}
