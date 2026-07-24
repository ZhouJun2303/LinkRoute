#[cfg(windows)]
mod platform {
    use std::ffi::c_void;
    use std::mem::size_of;
    use std::ptr;
    use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    const SERVICE_NAME: &str = "LinkRoute";

    const NO_ERROR: u32 = 0;
    const SERVICE_WIN32_OWN_PROCESS: u32 = 0x00000010;
    const SERVICE_STOPPED: u32 = 0x00000001;
    const SERVICE_START_PENDING: u32 = 0x00000002;
    const SERVICE_STOP_PENDING: u32 = 0x00000003;
    const SERVICE_RUNNING: u32 = 0x00000004;
    const SERVICE_ACCEPT_STOP: u32 = 0x00000001;
    const SERVICE_ACCEPT_SHUTDOWN: u32 = 0x00000004;
    const SERVICE_CONTROL_STOP: u32 = 0x00000001;
    const SERVICE_CONTROL_SHUTDOWN: u32 = 0x00000005;

    type ServiceStatusHandle = isize;

    static STOP_REQUESTED: AtomicBool = AtomicBool::new(false);
    static STATUS_HANDLE: AtomicIsize = AtomicIsize::new(0);

    #[repr(C)]
    struct ServiceTableEntryW {
        service_name: *mut u16,
        service_proc: Option<unsafe extern "system" fn(u32, *mut *mut u16)>,
    }

    #[repr(C)]
    struct ServiceStatus {
        service_type: u32,
        current_state: u32,
        controls_accepted: u32,
        win32_exit_code: u32,
        service_specific_exit_code: u32,
        check_point: u32,
        wait_hint: u32,
    }

    #[link(name = "advapi32")]
    unsafe extern "system" {
        #[link_name = "StartServiceCtrlDispatcherW"]
        fn start_service_ctrl_dispatcher_w(service_table: *const ServiceTableEntryW) -> i32;
        #[link_name = "RegisterServiceCtrlHandlerExW"]
        fn register_service_ctrl_handler_ex_w(
            service_name: *const u16,
            handler_proc: Option<
                unsafe extern "system" fn(u32, u32, *mut c_void, *mut c_void) -> u32,
            >,
            context: *mut c_void,
        ) -> ServiceStatusHandle;
        #[link_name = "SetServiceStatus"]
        fn set_service_status(
            service_status_handle: ServiceStatusHandle,
            service_status: *mut ServiceStatus,
        ) -> i32;
    }

    pub fn run_service_dispatcher() -> Result<(), String> {
        let mut service_name = wide(SERVICE_NAME);
        let service_table = [
            ServiceTableEntryW {
                service_name: service_name.as_mut_ptr(),
                service_proc: Some(service_main),
            },
            ServiceTableEntryW { service_name: ptr::null_mut(), service_proc: None },
        ];

        if unsafe { start_service_ctrl_dispatcher_w(service_table.as_ptr()) } == 0 {
            return Err(last_os_error("start service control dispatcher"));
        }
        Ok(())
    }

    unsafe extern "system" fn service_main(_argc: u32, _argv: *mut *mut u16) {
        STOP_REQUESTED.store(false, Ordering::SeqCst);
        let service_name = wide(SERVICE_NAME);
        let handle = unsafe {
            register_service_ctrl_handler_ex_w(
                service_name.as_ptr(),
                Some(service_control_handler),
                ptr::null_mut(),
            )
        };
        if handle == 0 {
            return;
        }
        STATUS_HANDLE.store(handle, Ordering::SeqCst);

        set_status(SERVICE_START_PENDING, 0, 1, 3000);
        let result = run_service_body();
        match result {
            Ok(()) => set_status(SERVICE_STOPPED, 0, 0, 0),
            Err(_) => set_status(SERVICE_STOPPED, 1, 0, 0),
        }
    }

    fn run_service_body() -> Result<(), String> {
        let controller = Arc::new(Mutex::new(crate::start_service_controller()?));
        set_status(SERVICE_RUNNING, 0, 0, 0);

        while !STOP_REQUESTED.load(Ordering::SeqCst) {
            let _ = crate::serve_control_pipe_once_shared(&controller, Duration::from_millis(250));
        }

        set_status(SERVICE_STOP_PENDING, 0, 1, 3000);
        let stop_result = controller
            .lock()
            .map_err(|_| String::from("service controller lock is poisoned"))?
            .stop();
        stop_result
    }

    unsafe extern "system" fn service_control_handler(
        control: u32,
        _event_type: u32,
        _event_data: *mut c_void,
        _context: *mut c_void,
    ) -> u32 {
        match control {
            SERVICE_CONTROL_STOP | SERVICE_CONTROL_SHUTDOWN => {
                STOP_REQUESTED.store(true, Ordering::SeqCst);
                set_status(SERVICE_STOP_PENDING, 0, 1, 3000);
            }
            _ => {}
        }
        NO_ERROR
    }

    fn set_status(current_state: u32, win32_exit_code: u32, check_point: u32, wait_hint: u32) {
        let handle = STATUS_HANDLE.load(Ordering::SeqCst);
        if handle == 0 {
            return;
        }

        let controls_accepted = if current_state == SERVICE_RUNNING {
            SERVICE_ACCEPT_STOP | SERVICE_ACCEPT_SHUTDOWN
        } else {
            0
        };
        let mut status = ServiceStatus {
            service_type: SERVICE_WIN32_OWN_PROCESS,
            current_state,
            controls_accepted,
            win32_exit_code,
            service_specific_exit_code: 0,
            check_point,
            wait_hint,
        };

        unsafe {
            set_service_status(handle, &mut status);
        }
    }

    fn wide(text: &str) -> Vec<u16> {
        text.encode_utf16().chain(std::iter::once(0)).collect()
    }

    fn last_os_error(action: &str) -> String {
        format!("could not {action}: {}", std::io::Error::last_os_error())
    }

    const _: () = {
        assert!(size_of::<ServiceStatus>() == 28);
    };
}

#[cfg(not(windows))]
mod platform {
    pub fn run_service_dispatcher() -> Result<(), String> {
        Err(String::from("Windows services are only supported on Windows"))
    }
}

pub fn run_service_dispatcher() -> Result<(), String> {
    platform::run_service_dispatcher()
}
