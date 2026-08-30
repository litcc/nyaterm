use std::sync::Arc;

use nyaterm_remote_desktop::RdpSessionManager;

#[cfg(target_os = "windows")]
mod platform {
    use std::collections::HashSet;
    use std::ptr::null_mut;
    use std::sync::{Arc, Mutex, OnceLock, Weak, mpsc};
    use std::thread;

    use nyaterm_remote_desktop::{RdpInputEvent, RdpSessionManager};
    use windows_sys::Win32::Foundation::{HINSTANCE, LPARAM, LRESULT, WPARAM};
    use windows_sys::Win32::System::Threading::GetCurrentThreadId;
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        MAPVK_VK_TO_VSC_EX, MapVirtualKeyW, VK_LWIN, VK_RWIN,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CallNextHookEx, DispatchMessageW, GetMessageW, HHOOK, KBDLLHOOKSTRUCT, LLKHF_EXTENDED,
        LLKHF_UP, MSG, PM_NOREMOVE, PeekMessageW, PostThreadMessageW, SetWindowsHookExW,
        TranslateMessage, UnhookWindowsHookEx, WH_KEYBOARD_LL, WM_KEYDOWN, WM_KEYUP, WM_QUIT,
        WM_SYSKEYDOWN, WM_SYSKEYUP,
    };

    static CAPTURE: OnceLock<Arc<CaptureState>> = OnceLock::new();
    static HOOK_THREAD: OnceLock<Mutex<Option<HookThread>>> = OnceLock::new();

    struct HookThread {
        thread_id: u32,
        worker: thread::JoinHandle<()>,
    }

    struct CaptureState {
        manager: Mutex<Weak<RdpSessionManager>>,
        session_id: Mutex<Option<String>>,
        win_key_down: Mutex<bool>,
        captured_keys: Mutex<HashSet<(u16, bool)>>,
    }

    #[derive(Clone, Debug, PartialEq, Eq)]
    struct CapturedKeyEvent {
        scan_code: u16,
        extended: bool,
        pressed: bool,
    }

    pub(super) fn set_keyboard_capture(
        manager: Arc<RdpSessionManager>,
        session_id: Option<String>,
    ) {
        let state = CAPTURE.get_or_init(|| {
            Arc::new(CaptureState {
                manager: Mutex::new(Weak::new()),
                session_id: Mutex::new(None),
                win_key_down: Mutex::new(false),
                captured_keys: Mutex::new(HashSet::new()),
            })
        });
        if let Ok(mut current_manager) = state.manager.lock() {
            *current_manager = Arc::downgrade(&manager);
        }
        let previous = state.session_id.lock().ok().and_then(|mut current| {
            let previous = current.clone();
            *current = session_id.clone();
            previous
        });
        if previous != session_id {
            let had_pressed_keys = reset_pressed_state(state);
            if had_pressed_keys && let Some(previous) = previous {
                let _ = manager.send_input(&previous, vec![RdpInputEvent::ReleaseAllKeys]);
            }
        }
        if session_id.is_some() {
            ensure_hook_thread();
        } else {
            stop_hook_thread();
        }
    }

    pub(super) fn shutdown_keyboard_capture() {
        if let Some(state) = CAPTURE.get() {
            reset_pressed_state(state);
            if let Ok(mut session_id) = state.session_id.lock() {
                *session_id = None;
            }
        }
        stop_hook_thread();
    }

    fn ensure_hook_thread() {
        let runtime = HOOK_THREAD.get_or_init(|| Mutex::new(None));
        let Ok(mut runtime) = runtime.lock() else {
            return;
        };
        if runtime.is_some() {
            return;
        }
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        let worker = match thread::Builder::new()
            .name("rdp-keyboard-capture".to_string())
            .spawn(move || hook_thread(ready_tx))
        {
            Ok(worker) => worker,
            Err(error) => {
                tracing::warn!(%error, "failed to start RDP keyboard capture thread");
                return;
            }
        };
        match ready_rx.recv() {
            Ok(Some(thread_id)) => {
                *runtime = Some(HookThread { thread_id, worker });
            }
            Ok(None) | Err(_) => {
                let _ = worker.join();
            }
        }
    }

    fn stop_hook_thread() {
        let Some(runtime) = HOOK_THREAD.get() else {
            return;
        };
        let hook = runtime.lock().ok().and_then(|mut runtime| runtime.take());
        let Some(hook) = hook else {
            return;
        };
        let posted = unsafe { PostThreadMessageW(hook.thread_id, WM_QUIT, 0, 0) };
        if posted == 0 {
            tracing::warn!("failed to post shutdown to RDP keyboard capture thread");
        }
        if hook.worker.join().is_err() {
            tracing::warn!("RDP keyboard capture thread panicked during shutdown");
        }
    }

    fn reset_pressed_state(state: &CaptureState) -> bool {
        let Ok(mut win_key_down) = state.win_key_down.lock() else {
            return false;
        };
        let Ok(mut captured_keys) = state.captured_keys.lock() else {
            return false;
        };
        let had_pressed_keys = *win_key_down || !captured_keys.is_empty();
        *win_key_down = false;
        captured_keys.clear();
        had_pressed_keys
    }

    fn hook_thread(ready_tx: mpsc::SyncSender<Option<u32>>) {
        let hook = unsafe {
            SetWindowsHookExW(
                WH_KEYBOARD_LL,
                Some(keyboard_hook_proc),
                null_mut::<std::ffi::c_void>() as HINSTANCE,
                0,
            )
        };
        if hook.is_null() {
            tracing::warn!("failed to install RDP keyboard capture hook");
            let _ = ready_tx.send(None);
            return;
        }
        let mut message = MSG::default();
        unsafe {
            PeekMessageW(&mut message, null_mut(), 0, 0, PM_NOREMOVE);
        }
        let _ = ready_tx.send(Some(unsafe { GetCurrentThreadId() }));
        while unsafe { GetMessageW(&mut message, null_mut(), 0, 0) } > 0 {
            unsafe {
                TranslateMessage(&message);
                DispatchMessageW(&message);
            }
        }
        unsafe {
            UnhookWindowsHookEx(hook);
        }
    }

    unsafe extern "system" fn keyboard_hook_proc(
        code: i32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        if code >= 0
            && let Some(state) = CAPTURE.get()
            && let Some(raw) = unsafe { (lparam as *const KBDLLHOOKSTRUCT).as_ref() }
            && let Some(event) =
                captured_key_event(wparam as u32, raw.vkCode, raw.scanCode, raw.flags)
            && update_capture_state(state, &event)
            && let Some((manager, session_id)) = capture_target(state)
        {
            let input = if event.pressed {
                RdpInputEvent::KeyDown {
                    scan_code: event.scan_code,
                    extended: event.extended,
                    repeat: false,
                }
            } else {
                RdpInputEvent::KeyUp {
                    scan_code: event.scan_code,
                    extended: event.extended,
                    repeat: false,
                }
            };
            let _ = manager.send_input(&session_id, vec![input]);
            return 1;
        }
        unsafe {
            CallNextHookEx(
                null_mut::<std::ffi::c_void>() as HHOOK,
                code,
                wparam,
                lparam,
            )
        }
    }

    fn captured_key_event(
        message: u32,
        vk_code: u32,
        raw_scan_code: u32,
        flags: u32,
    ) -> Option<CapturedKeyEvent> {
        let pressed = match message {
            WM_KEYDOWN | WM_SYSKEYDOWN => true,
            WM_KEYUP | WM_SYSKEYUP => false,
            _ => return None,
        } && flags & LLKHF_UP == 0;
        let mapped = unsafe { MapVirtualKeyW(vk_code, MAPVK_VK_TO_VSC_EX) };
        let scan_code = if raw_scan_code == 0 {
            mapped
        } else {
            raw_scan_code
        } & 0xff;
        if scan_code == 0 {
            return None;
        }
        Some(CapturedKeyEvent {
            scan_code: scan_code as u16,
            extended: flags & LLKHF_EXTENDED != 0 || matches!(vk_code as u16, VK_LWIN | VK_RWIN),
            pressed,
        })
    }

    fn update_capture_state(state: &CaptureState, event: &CapturedKeyEvent) -> bool {
        let Ok(mut win_key_down) = state.win_key_down.lock() else {
            return false;
        };
        let Ok(mut captured_keys) = state.captured_keys.lock() else {
            return false;
        };
        let is_win_key = event.extended && matches!(event.scan_code, 0x5b | 0x5c);
        if is_win_key {
            *win_key_down = event.pressed;
            return true;
        }
        let key = (event.scan_code, event.extended);
        if event.pressed && *win_key_down {
            captured_keys.insert(key);
            return true;
        }
        !event.pressed && captured_keys.remove(&key)
    }

    fn capture_target(state: &CaptureState) -> Option<(Arc<RdpSessionManager>, String)> {
        let session_id = state.session_id.lock().ok()?.clone()?;
        let manager = state.manager.lock().ok()?.upgrade()?;
        Some((manager, session_id))
    }

    #[cfg(test)]
    mod tests {
        use windows_sys::Win32::UI::Input::KeyboardAndMouse::VK_LWIN;
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            LLKHF_EXTENDED, LLKHF_UP, WM_KEYDOWN, WM_KEYUP,
        };

        use super::{CapturedKeyEvent, captured_key_event};

        #[test]
        fn windows_key_and_combo_scan_codes_are_preserved() {
            assert_eq!(
                captured_key_event(WM_KEYDOWN, u32::from(VK_LWIN), 0x5b, LLKHF_EXTENDED),
                Some(CapturedKeyEvent {
                    scan_code: 0x5b,
                    extended: true,
                    pressed: true,
                })
            );
            assert_eq!(
                captured_key_event(WM_KEYUP, u32::from(b'R'), 0x13, LLKHF_UP),
                Some(CapturedKeyEvent {
                    scan_code: 0x13,
                    extended: false,
                    pressed: false,
                })
            );
        }
    }
}

pub(super) fn set_keyboard_capture(manager: Arc<RdpSessionManager>, session_id: Option<String>) {
    #[cfg(target_os = "windows")]
    platform::set_keyboard_capture(manager, session_id);
    #[cfg(not(target_os = "windows"))]
    let _ = (manager, session_id);
}

pub(super) fn shutdown_keyboard_capture() {
    #[cfg(target_os = "windows")]
    platform::shutdown_keyboard_capture();
}
