use crate::service_bridge::ServiceBridge;

#[cfg(windows)]
mod platform {
    use std::ffi::c_void;
    use std::mem::{size_of, zeroed};
    use std::ptr;
    use std::sync::{Mutex, OnceLock};

    use crate::service_bridge::{status_text, ServiceBridge, ServiceBridgeError};

    type Hwnd = isize;
    type Hicon = isize;
    type Hinstance = isize;
    type Hmenu = isize;
    type Hbrush = isize;
    type Hcursor = isize;
    type Lresult = isize;
    type Wparam = usize;
    type Lparam = isize;

    const WM_DESTROY: u32 = 0x0002;
    const WM_COMMAND: u32 = 0x0111;
    const WM_CONTEXTMENU: u32 = 0x007B;
    const WM_LBUTTONDBLCLK: u32 = 0x0203;
    const WM_RBUTTONUP: u32 = 0x0205;
    const WM_HOTKEY: u32 = 0x0312;
    const WM_USER: u32 = 0x0400;
    const TRAY_CALLBACK_MESSAGE: u32 = WM_USER + 1;

    const NIM_ADD: u32 = 0x00000000;
    const NIM_DELETE: u32 = 0x00000002;
    const NIF_MESSAGE: u32 = 0x00000001;
    const NIF_ICON: u32 = 0x00000002;
    const NIF_TIP: u32 = 0x00000004;

    const IDI_APPLICATION: u16 = 32512;
    const MF_STRING: u32 = 0x00000000;
    const MF_SEPARATOR: u32 = 0x00000800;
    const TPM_RIGHTBUTTON: u32 = 0x00000002;
    const TPM_RETURNCMD: u32 = 0x00000100;

    const MB_OK: u32 = 0x00000000;
    const MB_ICONERROR: u32 = 0x00000010;
    const MB_ICONINFORMATION: u32 = 0x00000040;

    const MOD_ALT: u32 = 0x0001;
    const MOD_CONTROL: u32 = 0x0002;
    const VK_L: u32 = 0x4C;
    const VK_S: u32 = 0x53;

    const TRAY_ID: u32 = 1;
    const HOTKEY_TOGGLE: i32 = 1;
    const HOTKEY_SWAP: i32 = 2;
    const MENU_TOGGLE: u32 = 1001;
    const MENU_SWAP: u32 = 1002;
    const MENU_STATUS: u32 = 1003;
    const MENU_DIAGNOSTICS: u32 = 1004;
    const MENU_EXIT: u32 = 1005;

    static TRAY_STATE: OnceLock<Mutex<TrayState>> = OnceLock::new();

    struct TrayState {
        bridge: ServiceBridge,
    }

    impl TrayState {
        fn new(bridge: ServiceBridge) -> Self {
            Self { bridge }
        }

        fn status_text(&self) -> Result<String, ServiceBridgeError> {
            let status = self.bridge.status()?;
            Ok(status_text(&status))
        }

        fn toggle_enabled(&self) -> Result<String, ServiceBridgeError> {
            let enabled = self.bridge.toggle_enabled()?;
            Ok(if enabled { "Routing enabled." } else { "Routing paused." }.to_string())
        }

        fn swap_outlets(&self) -> Result<String, ServiceBridgeError> {
            self.bridge.swap_outlets()?;
            Ok("Ethernet and Wi-Fi rule targets swapped.".to_string())
        }

        fn diagnostics(&self) -> Result<String, ServiceBridgeError> {
            self.bridge.diagnostics()
        }
    }

    #[repr(C)]
    struct Point {
        x: i32,
        y: i32,
    }

    #[repr(C)]
    struct Msg {
        hwnd: Hwnd,
        message: u32,
        wparam: Wparam,
        lparam: Lparam,
        time: u32,
        point: Point,
    }

    #[repr(C)]
    struct WndClassExW {
        cb_size: u32,
        style: u32,
        wnd_proc: Option<unsafe extern "system" fn(Hwnd, u32, Wparam, Lparam) -> Lresult>,
        cls_extra: i32,
        wnd_extra: i32,
        instance: Hinstance,
        icon: Hicon,
        cursor: Hcursor,
        background: Hbrush,
        menu_name: *const u16,
        class_name: *const u16,
        icon_small: Hicon,
    }

    #[repr(C)]
    struct Guid {
        data1: u32,
        data2: u16,
        data3: u16,
        data4: [u8; 8],
    }

    #[repr(C)]
    struct NotifyIconDataW {
        cb_size: u32,
        hwnd: Hwnd,
        id: u32,
        flags: u32,
        callback_message: u32,
        icon: Hicon,
        tip: [u16; 128],
        state: u32,
        state_mask: u32,
        info: [u16; 256],
        version: u32,
        info_title: [u16; 64],
        info_flags: u32,
        guid_item: Guid,
        balloon_icon: Hicon,
    }

    #[link(name = "user32")]
    unsafe extern "system" {
        #[link_name = "RegisterClassExW"]
        fn register_class_ex_w(class: *const WndClassExW) -> u16;
        #[link_name = "CreateWindowExW"]
        fn create_window_ex_w(
            ex_style: u32,
            class_name: *const u16,
            window_name: *const u16,
            style: u32,
            x: i32,
            y: i32,
            width: i32,
            height: i32,
            parent: Hwnd,
            menu: Hmenu,
            instance: Hinstance,
            param: *const c_void,
        ) -> Hwnd;
        #[link_name = "DefWindowProcW"]
        fn def_window_proc_w(hwnd: Hwnd, message: u32, wparam: Wparam, lparam: Lparam) -> Lresult;
        #[link_name = "DestroyWindow"]
        fn destroy_window(hwnd: Hwnd) -> i32;
        #[link_name = "GetMessageW"]
        fn get_message_w(message: *mut Msg, hwnd: Hwnd, min_filter: u32, max_filter: u32) -> i32;
        #[link_name = "TranslateMessage"]
        fn translate_message(message: *const Msg) -> i32;
        #[link_name = "DispatchMessageW"]
        fn dispatch_message_w(message: *const Msg) -> Lresult;
        #[link_name = "PostQuitMessage"]
        fn post_quit_message(exit_code: i32);
        #[link_name = "LoadIconW"]
        fn load_icon_w(instance: Hinstance, icon_name: *const u16) -> Hicon;
        #[link_name = "RegisterHotKey"]
        fn register_hot_key(hwnd: Hwnd, id: i32, modifiers: u32, virtual_key: u32) -> i32;
        #[link_name = "UnregisterHotKey"]
        fn unregister_hot_key(hwnd: Hwnd, id: i32) -> i32;
        #[link_name = "CreatePopupMenu"]
        fn create_popup_menu() -> Hmenu;
        #[link_name = "AppendMenuW"]
        fn append_menu_w(menu: Hmenu, flags: u32, item_id: usize, item_text: *const u16) -> i32;
        #[link_name = "TrackPopupMenu"]
        fn track_popup_menu(
            menu: Hmenu,
            flags: u32,
            x: i32,
            y: i32,
            reserved: i32,
            hwnd: Hwnd,
            rect: *const c_void,
        ) -> i32;
        #[link_name = "DestroyMenu"]
        fn destroy_menu(menu: Hmenu) -> i32;
        #[link_name = "GetCursorPos"]
        fn get_cursor_pos(point: *mut Point) -> i32;
        #[link_name = "SetForegroundWindow"]
        fn set_foreground_window(hwnd: Hwnd) -> i32;
        #[link_name = "MessageBoxW"]
        fn message_box_w(hwnd: Hwnd, text: *const u16, caption: *const u16, flags: u32) -> i32;
    }

    #[link(name = "shell32")]
    unsafe extern "system" {
        #[link_name = "Shell_NotifyIconW"]
        fn shell_notify_icon_w(message: u32, data: *mut NotifyIconDataW) -> i32;
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        #[link_name = "GetModuleHandleW"]
        fn get_module_handle_w(module_name: *const u16) -> Hinstance;
    }

    pub fn run_tray(bridge: ServiceBridge) -> Result<(), String> {
        TRAY_STATE
            .set(Mutex::new(TrayState::new(bridge)))
            .map_err(|_| "tray state is already initialized".to_string())?;

        let class_name = wide("LinkRouteTrayWindow");
        let title = wide("LinkRoute");

        let instance = unsafe { get_module_handle_w(ptr::null()) };
        let icon = unsafe { load_icon_w(0, int_resource(IDI_APPLICATION)) };
        let class = WndClassExW {
            cb_size: size_of::<WndClassExW>() as u32,
            style: 0,
            wnd_proc: Some(window_proc),
            cls_extra: 0,
            wnd_extra: 0,
            instance,
            icon,
            cursor: 0,
            background: 0,
            menu_name: ptr::null(),
            class_name: class_name.as_ptr(),
            icon_small: icon,
        };

        if unsafe { register_class_ex_w(&class) } == 0 {
            return Err(last_os_error("register tray window class"));
        }

        let hwnd = unsafe {
            create_window_ex_w(
                0,
                class_name.as_ptr(),
                title.as_ptr(),
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                instance,
                ptr::null(),
            )
        };
        if hwnd == 0 {
            return Err(last_os_error("create tray window"));
        }

        add_tray_icon(hwnd)?;
        warn_if_hotkeys_failed(hwnd);
        message_loop()
    }

    unsafe extern "system" fn window_proc(
        hwnd: Hwnd,
        message: u32,
        wparam: Wparam,
        lparam: Lparam,
    ) -> Lresult {
        match message {
            TRAY_CALLBACK_MESSAGE => {
                match lparam as u32 {
                    WM_RBUTTONUP | WM_CONTEXTMENU => show_menu(hwnd),
                    WM_LBUTTONDBLCLK => show_status(hwnd),
                    _ => {}
                }
                0
            }
            WM_COMMAND => {
                handle_menu_command(hwnd, (wparam & 0xffff) as u32);
                0
            }
            WM_HOTKEY => {
                handle_hotkey(hwnd, wparam as i32);
                0
            }
            WM_DESTROY => {
                remove_tray_icon(hwnd);
                unregister_hotkeys(hwnd);
                unsafe { post_quit_message(0) };
                0
            }
            _ => unsafe { def_window_proc_w(hwnd, message, wparam, lparam) },
        }
    }

    fn add_tray_icon(hwnd: Hwnd) -> Result<(), String> {
        let mut data = tray_data(hwnd);
        if unsafe { shell_notify_icon_w(NIM_ADD, &mut data) } == 0 {
            return Err(last_os_error("add tray icon"));
        }
        Ok(())
    }

    fn remove_tray_icon(hwnd: Hwnd) {
        let mut data = tray_data(hwnd);
        unsafe {
            shell_notify_icon_w(NIM_DELETE, &mut data);
        }
    }

    fn tray_data(hwnd: Hwnd) -> NotifyIconDataW {
        let mut data: NotifyIconDataW = unsafe { zeroed() };
        data.cb_size = size_of::<NotifyIconDataW>() as u32;
        data.hwnd = hwnd;
        data.id = TRAY_ID;
        data.flags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
        data.callback_message = TRAY_CALLBACK_MESSAGE;
        data.icon = unsafe { load_icon_w(0, int_resource(IDI_APPLICATION)) };
        copy_tip("LinkRoute - Ctrl+Alt+L toggle, Ctrl+Alt+S swap", &mut data.tip);
        data
    }

    fn warn_if_hotkeys_failed(hwnd: Hwnd) {
        let toggle_ok =
            unsafe { register_hot_key(hwnd, HOTKEY_TOGGLE, MOD_ALT | MOD_CONTROL, VK_L) } != 0;
        let swap_ok =
            unsafe { register_hot_key(hwnd, HOTKEY_SWAP, MOD_ALT | MOD_CONTROL, VK_S) } != 0;

        if !toggle_ok || !swap_ok {
            show_error(
                hwnd,
                "Could not register one or more global hotkeys. Tray menu controls still work.",
            );
        }
    }

    fn unregister_hotkeys(hwnd: Hwnd) {
        unsafe {
            unregister_hot_key(hwnd, HOTKEY_TOGGLE);
            unregister_hot_key(hwnd, HOTKEY_SWAP);
        }
    }

    fn message_loop() -> Result<(), String> {
        let mut message: Msg = unsafe { zeroed() };
        loop {
            let result = unsafe { get_message_w(&mut message, 0, 0, 0) };
            if result == -1 {
                return Err(last_os_error("read tray message"));
            }
            if result == 0 {
                return Ok(());
            }
            unsafe {
                translate_message(&message);
                dispatch_message_w(&message);
            }
        }
    }

    fn show_menu(hwnd: Hwnd) {
        let menu = unsafe { create_popup_menu() };
        if menu == 0 {
            show_error(hwnd, &last_os_error("create tray menu"));
            return;
        }

        append_menu_item(menu, MENU_TOGGLE, "Enable/Pause");
        append_menu_item(menu, MENU_SWAP, "Swap Ethernet/Wi-Fi");
        append_menu_item(menu, MENU_STATUS, "Status");
        append_menu_item(menu, MENU_DIAGNOSTICS, "Diagnostics");
        unsafe {
            append_menu_w(menu, MF_SEPARATOR, 0, ptr::null());
        }
        append_menu_item(menu, MENU_EXIT, "Exit");

        let mut point = Point { x: 0, y: 0 };
        if unsafe { get_cursor_pos(&mut point) } == 0 {
            show_error(hwnd, &last_os_error("read cursor position"));
            unsafe {
                destroy_menu(menu);
            }
            return;
        }

        unsafe {
            set_foreground_window(hwnd);
        }
        let command = unsafe {
            track_popup_menu(
                menu,
                TPM_RETURNCMD | TPM_RIGHTBUTTON,
                point.x,
                point.y,
                0,
                hwnd,
                ptr::null(),
            )
        };
        unsafe {
            destroy_menu(menu);
        }

        if command != 0 {
            handle_menu_command(hwnd, command as u32);
        }
    }

    fn append_menu_item(menu: Hmenu, item_id: u32, text: &str) {
        let text = wide(text);
        unsafe {
            append_menu_w(menu, MF_STRING, item_id as usize, text.as_ptr());
        }
    }

    fn handle_menu_command(hwnd: Hwnd, command: u32) {
        match command {
            MENU_TOGGLE => handle_action(hwnd, with_state(|state| state.toggle_enabled())),
            MENU_SWAP => handle_action(hwnd, with_state(|state| state.swap_outlets())),
            MENU_STATUS => show_status(hwnd),
            MENU_DIAGNOSTICS => show_diagnostics(hwnd),
            MENU_EXIT => unsafe {
                destroy_window(hwnd);
            },
            _ => {}
        }
    }

    fn handle_hotkey(hwnd: Hwnd, hotkey_id: i32) {
        match hotkey_id {
            HOTKEY_TOGGLE => handle_action(hwnd, with_state(|state| state.toggle_enabled())),
            HOTKEY_SWAP => handle_action(hwnd, with_state(|state| state.swap_outlets())),
            _ => {}
        }
    }

    fn show_status(hwnd: Hwnd) {
        match with_state(|state| state.status_text()) {
            Ok(text) => show_message(hwnd, "LinkRoute Status", &text, MB_OK | MB_ICONINFORMATION),
            Err(error) => show_error(hwnd, &error),
        }
    }

    fn show_diagnostics(hwnd: Hwnd) {
        match with_state(|state| state.diagnostics()) {
            Ok(text) => {
                show_message(hwnd, "LinkRoute Diagnostics", &text, MB_OK | MB_ICONINFORMATION)
            }
            Err(error) => show_error(hwnd, &error),
        }
    }

    fn handle_action(hwnd: Hwnd, result: Result<String, String>) {
        match result {
            Ok(message) => show_message(hwnd, "LinkRoute", &message, MB_OK | MB_ICONINFORMATION),
            Err(error) => show_error(hwnd, &error),
        }
    }

    fn with_state<T>(
        operation: impl FnOnce(&TrayState) -> Result<T, ServiceBridgeError>,
    ) -> Result<T, String> {
        let state = TRAY_STATE.get().ok_or_else(|| "tray state is not initialized".to_string())?;
        let guard = state.lock().map_err(|_| "tray state lock is poisoned".to_string())?;
        operation(&guard).map_err(|error| error.to_string())
    }

    fn show_error(hwnd: Hwnd, message: &str) {
        show_message(hwnd, "LinkRoute Error", message, MB_OK | MB_ICONERROR);
    }

    fn show_message(hwnd: Hwnd, title: &str, text: &str, flags: u32) {
        let title = wide(title);
        let text = wide(text);
        unsafe {
            message_box_w(hwnd, text.as_ptr(), title.as_ptr(), flags);
        }
    }

    fn copy_tip(text: &str, buffer: &mut [u16; 128]) {
        for (index, value) in text.encode_utf16().take(buffer.len() - 1).enumerate() {
            buffer[index] = value;
        }
    }

    fn wide(text: &str) -> Vec<u16> {
        text.encode_utf16().chain(std::iter::once(0)).collect()
    }

    fn int_resource(id: u16) -> *const u16 {
        id as usize as *const u16
    }

    fn last_os_error(action: &str) -> String {
        format!("could not {action}: {}", std::io::Error::last_os_error())
    }
}

#[cfg(not(windows))]
mod platform {
    use crate::service_bridge::{status_text, ServiceBridge};

    pub fn run_tray(bridge: ServiceBridge) -> Result<(), String> {
        let status = bridge.status().map_err(|error| error.to_string())?;
        println!("{}", status_text(&status));
        Ok(())
    }
}

pub fn run_tray(bridge: ServiceBridge) -> Result<(), String> {
    platform::run_tray(bridge)
}
