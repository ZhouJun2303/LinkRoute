#[cfg(windows)]
mod platform {
    use std::ffi::c_void;
    use std::ptr;

    use linkroute_core::IPC_PIPE_NAME;

    const BUFFER_SIZE: usize = 64 * 1024;
    const INVALID_HANDLE_VALUE: isize = -1;
    const GENERIC_READ: u32 = 0x80000000;
    const GENERIC_WRITE: u32 = 0x40000000;
    const OPEN_EXISTING: u32 = 3;
    const PIPE_READMODE_MESSAGE: u32 = 0x00000002;
    const ERROR_MORE_DATA: u32 = 234;

    #[link(name = "kernel32")]
    unsafe extern "system" {
        #[link_name = "WaitNamedPipeW"]
        fn wait_named_pipe_w(name: *const u16, timeout: u32) -> i32;
        #[link_name = "CreateFileW"]
        fn create_file_w(
            file_name: *const u16,
            desired_access: u32,
            share_mode: u32,
            security_attributes: *mut c_void,
            creation_disposition: u32,
            flags_and_attributes: u32,
            template_file: isize,
        ) -> isize;
        #[link_name = "SetNamedPipeHandleState"]
        fn set_named_pipe_handle_state(
            named_pipe: isize,
            mode: *mut u32,
            max_collection_count: *mut u32,
            collect_data_timeout: *mut u32,
        ) -> i32;
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
    }

    pub fn send_request_json(json: &str) -> Result<Option<String>, String> {
        let pipe_name = wide(IPC_PIPE_NAME);
        if unsafe { wait_named_pipe_w(pipe_name.as_ptr(), 100) } == 0 {
            return Ok(None);
        }

        let handle = unsafe {
            create_file_w(
                pipe_name.as_ptr(),
                GENERIC_READ | GENERIC_WRITE,
                0,
                ptr::null_mut(),
                OPEN_EXISTING,
                0,
                0,
            )
        };
        if handle == INVALID_HANDLE_VALUE {
            return Ok(None);
        }
        let pipe = PipeHandle(handle);
        set_message_read_mode(pipe.0)?;
        write_message(pipe.0, json.as_bytes())?;
        read_message(pipe.0).map(Some)
    }

    fn set_message_read_mode(handle: isize) -> Result<(), String> {
        let mut mode = PIPE_READMODE_MESSAGE;
        if unsafe {
            set_named_pipe_handle_state(handle, &mut mode, ptr::null_mut(), ptr::null_mut())
        } == 0
        {
            return Err(last_os_error("set control pipe read mode"));
        }
        Ok(())
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
            return Err(last_os_error("write control pipe request"));
        }
        if written as usize != message.len() {
            return Err(format!("short control pipe write: {written} of {} bytes", message.len()));
        }
        Ok(())
    }

    fn read_message(handle: isize) -> Result<String, String> {
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
                error => {
                    return Err(format!(
                        "could not read control pipe response: Windows error {error}"
                    ))
                }
            }
        }

        String::from_utf8(data).map_err(|error| error.to_string())
    }

    struct PipeHandle(isize);

    impl Drop for PipeHandle {
        fn drop(&mut self) {
            unsafe {
                close_handle(self.0);
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
    pub fn send_request_json(_json: &str) -> Result<Option<String>, String> {
        Ok(None)
    }
}

pub fn send_request_json(json: &str) -> Result<Option<String>, String> {
    platform::send_request_json(json)
}
