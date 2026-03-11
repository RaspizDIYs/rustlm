use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use windows::core::*;
use windows::Win32::Foundation::*;
use windows::Win32::System::Com::*;
use windows::Win32::System::Variant::*;
use windows::Win32::UI::Accessibility::*;
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::WindowsAndMessaging::*;

/// Attempt to log in to Riot Client by automating the login form via UI Automation.
/// This must be called from a COM STA thread (use `spawn_blocking`).
/// Pass `cancel` to allow early abort from the UI.
pub fn login_to_riot_client(username: &str, password: &str, timeout_secs: u64, cancel: Option<&Arc<AtomicBool>>) -> Result<()> {
    unsafe {
        CoInitializeEx(Some(std::ptr::null()), COINIT_APARTMENTTHREADED).ok()?;
    }

    let result = unsafe { do_login(username, password, timeout_secs, cancel) };

    unsafe {
        CoUninitialize();
    }

    result
}

fn is_cancelled(cancel: Option<&Arc<AtomicBool>>) -> bool {
    cancel.map_or(false, |c| c.load(Ordering::Relaxed))
}

unsafe fn do_login(username: &str, password: &str, timeout_secs: u64, cancel: Option<&Arc<AtomicBool>>) -> Result<()> {
    let automation: IUIAutomation =
        CoCreateInstance(&CUIAutomation8, None, CLSCTX_INPROC_SERVER)?;

    let deadline = std::time::Instant::now() + Duration::from_secs(timeout_secs);

    // Find Riot Client window
    let hwnd = wait_for_riot_client_window(deadline, cancel)?;

    // Activate window
    activate_window(hwnd);
    std::thread::sleep(Duration::from_millis(80));

    let element = automation.ElementFromHandle(hwnd)?;

    if is_cancelled(cancel) { return Err(Error::new(E_ABORT, "Login cancelled")); }

    // Find login fields with retry
    let (username_el, password_el, sign_in_el) =
        find_login_elements(&automation, &element, hwnd, deadline, cancel)?;

    // Input username
    set_element_value(&username_el, username)?;
    std::thread::sleep(Duration::from_millis(20));

    // Verify username was set
    verify_value(&username_el, username);

    // Input password
    set_element_value(&password_el, password)?;
    std::thread::sleep(Duration::from_millis(20));

    // Try to check "Remember Me" checkbox
    let _ = try_check_remember_me(&automation, &element);

    // Click sign in
    std::thread::sleep(Duration::from_millis(30));
    if let Some(btn) = sign_in_el {
        invoke_button(&btn);
    } else {
        // Fallback: send Enter key
        let fg = GetForegroundWindow();
        if fg == hwnd {
            focus_element(&password_el);
            std::thread::sleep(Duration::from_millis(30));
            send_virtual_key(VK_RETURN);
        }
    }

    Ok(())
}

fn wait_for_riot_client_window(deadline: std::time::Instant, cancel: Option<&Arc<AtomicBool>>) -> Result<HWND> {
    let process_names = ["RiotClientUx", "RiotClientUxRender", "Riot Client"];

    loop {
        if is_cancelled(cancel) {
            return Err(Error::new(E_ABORT, "Login cancelled"));
        }
        if std::time::Instant::now() > deadline {
            return Err(Error::new(E_FAIL, "Riot Client window not found within timeout"));
        }

        for name in &process_names {
            if let Some(hwnd) = find_process_main_window(name) {
                return Ok(hwnd);
            }
        }

        std::thread::sleep(Duration::from_millis(100));
    }
}

fn find_process_main_window(process_name: &str) -> Option<HWND> {
    // Use native Windows API (CreateToolhelp32Snapshot) to find process PIDs
    // This handles names with spaces like "Riot Client" without shell quoting issues
    let exe_name = format!("{}.exe", process_name);

    use windows::Win32::System::Diagnostics::ToolHelp::*;
    use windows::Win32::Foundation::CloseHandle;

    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0).ok()?;
        let mut entry = PROCESSENTRY32W::default();
        entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;

        if Process32FirstW(snapshot, &mut entry).is_ok() {
            loop {
                let name = String::from_utf16_lossy(
                    &entry.szExeFile[..entry.szExeFile.iter().position(|&c| c == 0).unwrap_or(entry.szExeFile.len())]
                );
                if name.eq_ignore_ascii_case(&exe_name) {
                    let pid = entry.th32ProcessID;
                    let _ = CloseHandle(snapshot);
                    return find_main_window_for_pid(pid);
                }
                if Process32NextW(snapshot, &mut entry).is_err() {
                    break;
                }
            }
        }
        let _ = CloseHandle(snapshot);
    }
    None
}

struct EnumData {
    pid: u32,
    hwnd: HWND,
}

unsafe extern "system" fn enum_windows_callback(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let data = &mut *(lparam.0 as *mut EnumData);
    let mut window_pid: u32 = 0;
    GetWindowThreadProcessId(hwnd, Some(&mut window_pid));

    if window_pid == data.pid && IsWindowVisible(hwnd).as_bool() {
        data.hwnd = hwnd;
        return FALSE; // Stop enumeration
    }
    TRUE
}

fn find_main_window_for_pid(pid: u32) -> Option<HWND> {
    let mut data = EnumData {
        pid,
        hwnd: HWND::default(),
    };

    unsafe {
        let _ = EnumWindows(
            Some(enum_windows_callback),
            LPARAM(&mut data as *mut EnumData as isize),
        );
    }

    if data.hwnd.0 != std::ptr::null_mut() {
        Some(data.hwnd)
    } else {
        None
    }
}

unsafe fn activate_window(hwnd: HWND) {
    let _ = ShowWindow(hwnd, SW_RESTORE);
    let _ = SetForegroundWindow(hwnd);
    std::thread::sleep(Duration::from_millis(50));
    let _ = ShowWindow(hwnd, SW_RESTORE);
    let _ = SetForegroundWindow(hwnd);
}

unsafe fn find_login_elements(
    automation: &IUIAutomation,
    root: &IUIAutomationElement,
    hwnd: HWND,
    deadline: std::time::Instant,
    cancel: Option<&Arc<AtomicBool>>,
) -> Result<(IUIAutomationElement, IUIAutomationElement, Option<IUIAutomationElement>)> {
    let mut scan_cycles = 0u32;
    let mut clicked_sign_in_landing = false;

    loop {
        if is_cancelled(cancel) {
            return Err(Error::new(E_ABORT, "Login cancelled"));
        }
        if std::time::Instant::now() > deadline {
            return Err(Error::new(E_FAIL, "Login fields not found within timeout"));
        }

        scan_cycles += 1;

        // Periodic window activation
        if scan_cycles % 10 == 0 {
            activate_window(hwnd);
        }

        // After some retries, try clicking "Sign in" landing button
        // (RC sometimes shows a landing page with a "Sign in" button before the actual form)
        if !clicked_sign_in_landing && scan_cycles >= 5 {
            if let Some(btn) = find_landing_sign_in_button(automation, root) {
                invoke_button(&btn);
                clicked_sign_in_landing = true;
                std::thread::sleep(Duration::from_millis(300));
                continue;
            }
        }

        // Try to find username field
        let username_el = find_element_by_ids(automation, root, &["username", "login"])
            .or_else(|| find_element_by_names(automation, root, &["username", "Login", "Email", "Адрес электронной почты", "Имя пользователя"]));

        // Try to find password field
        let password_el = find_element_by_ids(automation, root, &["password"])
            .or_else(|| find_element_by_names(automation, root, &["password", "Пароль", "Password"]));

        // If direct search failed, try edit controls fallback
        let (username_el, password_el) = match (username_el, password_el) {
            (Some(u), Some(p)) => (u, p),
            _ => {
                if let Some((u, p)) = find_edit_controls_fallback(automation, root) {
                    (u, p)
                } else {
                    std::thread::sleep(Duration::from_millis(50));
                    continue;
                }
            }
        };

        // Find sign-in button (the submit button on the login form)
        let sign_in = find_sign_in_button(automation, root);

        return Ok((username_el, password_el, sign_in));
    }
}

unsafe fn find_element_by_ids(
    automation: &IUIAutomation,
    root: &IUIAutomationElement,
    ids: &[&str],
) -> Option<IUIAutomationElement> {
    for id in ids {
        let condition = automation
            .CreatePropertyCondition(UIA_AutomationIdPropertyId, &VARIANT::from(BSTR::from(*id)))
            .ok()?;
        if let Ok(el) = root.FindFirst(TreeScope_Descendants, &condition) {
            return Some(el);
        }
    }
    None
}

unsafe fn find_element_by_names(
    automation: &IUIAutomation,
    root: &IUIAutomationElement,
    names: &[&str],
) -> Option<IUIAutomationElement> {
    for name in names {
        let condition = automation
            .CreatePropertyCondition(UIA_NamePropertyId, &VARIANT::from(BSTR::from(*name)))
            .ok()?;
        if let Ok(el) = root.FindFirst(TreeScope_Descendants, &condition) {
            return Some(el);
        }
    }
    None
}

unsafe fn find_edit_controls_fallback(
    automation: &IUIAutomation,
    root: &IUIAutomationElement,
) -> Option<(IUIAutomationElement, IUIAutomationElement)> {
    let edit_condition = automation
        .CreatePropertyCondition(
            UIA_ControlTypePropertyId,
            &VARIANT::from(UIA_EditControlTypeId.0 as i32),
        )
        .ok()?;

    let edits = root
        .FindAll(TreeScope_Descendants, &edit_condition)
        .ok()?;

    let count = edits.Length().ok()?;
    if count >= 2 {
        let first = edits.GetElement(0).ok()?;
        let second = edits.GetElement(1).ok()?;
        Some((first, second))
    } else {
        None
    }
}

/// Find a "Sign in" button on the RC landing page (before the login form appears).
/// This is broader than find_sign_in_button — it looks for any clickable element
/// with sign-in text, not just buttons (could be a hyperlink or custom control).
unsafe fn find_landing_sign_in_button(
    automation: &IUIAutomation,
    root: &IUIAutomationElement,
) -> Option<IUIAutomationElement> {
    let names = ["Sign in", "Sign In", "Войти", "Log In", "LOG IN", "SIGN IN"];

    for name in &names {
        let condition = automation
            .CreatePropertyCondition(UIA_NamePropertyId, &VARIANT::from(BSTR::from(*name)))
            .ok()?;
        if let Ok(el) = root.FindFirst(TreeScope_Descendants, &condition) {
            return Some(el);
        }
    }
    None
}

unsafe fn find_sign_in_button(
    automation: &IUIAutomation,
    root: &IUIAutomationElement,
) -> Option<IUIAutomationElement> {
    let button_names = ["Sign in", "Sign In", "Log In", "Войти"];

    for name in &button_names {
        let name_cond = automation
            .CreatePropertyCondition(UIA_NamePropertyId, &VARIANT::from(BSTR::from(*name)))
            .ok()?;
        let type_cond = automation
            .CreatePropertyCondition(
                UIA_ControlTypePropertyId,
                &VARIANT::from(UIA_ButtonControlTypeId.0 as i32),
            )
            .ok()?;
        let combined = automation.CreateAndCondition(&name_cond, &type_cond).ok()?;

        if let Ok(el) = root.FindFirst(TreeScope_Descendants, &combined) {
            return Some(el);
        }
    }

    None
}

unsafe fn set_element_value(element: &IUIAutomationElement, value: &str) -> Result<()> {
    // Try ValuePattern first
    let pattern_obj = element.GetCurrentPattern(UIA_ValuePatternId);
    if let Ok(pattern_obj) = pattern_obj {
        let value_pattern: IUIAutomationValuePattern = pattern_obj.cast()?;
        // Clear first
        value_pattern.SetValue(&BSTR::from(""))?;
        std::thread::sleep(Duration::from_millis(30));
        value_pattern.SetValue(&BSTR::from(value))?;
        return Ok(());
    }

    // Fallback: focus and use keyboard
    focus_element(element);
    std::thread::sleep(Duration::from_millis(50));

    // Select all and type
    send_key_combo(VK_CONTROL, VK_A);
    std::thread::sleep(Duration::from_millis(30));

    for ch in value.chars() {
        send_char(ch);
        std::thread::sleep(Duration::from_millis(10));
    }

    Ok(())
}

unsafe fn verify_value(element: &IUIAutomationElement, expected: &str) {
    let pattern_obj = element.GetCurrentPattern(UIA_ValuePatternId);
    if let Ok(pattern_obj) = pattern_obj {
        if let Ok(value_pattern) = pattern_obj.cast::<IUIAutomationValuePattern>() {
            for _ in 0..2 {
                if let Ok(current) = value_pattern.CurrentValue() {
                    if current.to_string() == expected {
                        return;
                    }
                }
                // Retry
                let _ = value_pattern.SetValue(&BSTR::from(""));
                std::thread::sleep(Duration::from_millis(30));
                let _ = value_pattern.SetValue(&BSTR::from(expected));
                std::thread::sleep(Duration::from_millis(50));
            }
        }
    }
}

unsafe fn try_check_remember_me(
    automation: &IUIAutomation,
    root: &IUIAutomationElement,
) -> Result<()> {
    let checkbox_cond = automation.CreatePropertyCondition(
        UIA_ControlTypePropertyId,
        &VARIANT::from(UIA_CheckBoxControlTypeId.0 as i32),
    )?;

    if let Ok(checkbox) = root.FindFirst(TreeScope_Descendants, &checkbox_cond) {
        let toggle_pattern = checkbox
            .GetCurrentPattern(UIA_TogglePatternId)?
            .cast::<IUIAutomationTogglePattern>()?;

        let state = toggle_pattern.CurrentToggleState()?;
        if state != ToggleState_On {
            toggle_pattern.Toggle()?;
        }
    }

    Ok(())
}

unsafe fn invoke_button(element: &IUIAutomationElement) {
    let pattern_obj = element.GetCurrentPattern(UIA_InvokePatternId);
    if let Ok(pattern_obj) = pattern_obj {
        if let Ok(invoke_pattern) = pattern_obj.cast::<IUIAutomationInvokePattern>() {
            let _ = invoke_pattern.Invoke();
            return;
        }
    }

    // Fallback: click via bounding rect
    if let Ok(rect) = element.CurrentBoundingRectangle() {
        let x = rect.left + (rect.right - rect.left) / 2;
        let y = rect.top + (rect.bottom - rect.top) / 2;
        let _ = SetCursorPos(x as i32, y as i32);
        std::thread::sleep(Duration::from_millis(30));

        let mut inputs = [
            INPUT {
                r#type: INPUT_MOUSE,
                Anonymous: INPUT_0 {
                    mi: MOUSEINPUT {
                        dwFlags: MOUSEEVENTF_LEFTDOWN,
                        ..Default::default()
                    },
                },
            },
            INPUT {
                r#type: INPUT_MOUSE,
                Anonymous: INPUT_0 {
                    mi: MOUSEINPUT {
                        dwFlags: MOUSEEVENTF_LEFTUP,
                        ..Default::default()
                    },
                },
            },
        ];
        SendInput(&mut inputs, std::mem::size_of::<INPUT>() as i32);
    }
}

unsafe fn focus_element(element: &IUIAutomationElement) {
    let _ = element.SetFocus();
}

unsafe fn send_virtual_key(vk: VIRTUAL_KEY) {
    let mut inputs = [
        INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: vk,
                    ..Default::default()
                },
            },
        },
        INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: vk,
                    dwFlags: KEYEVENTF_KEYUP,
                    ..Default::default()
                },
            },
        },
    ];
    SendInput(&mut inputs, std::mem::size_of::<INPUT>() as i32);
}

unsafe fn send_key_combo(modifier: VIRTUAL_KEY, key: VIRTUAL_KEY) {
    let mut inputs = [
        INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: modifier,
                    ..Default::default()
                },
            },
        },
        INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: key,
                    ..Default::default()
                },
            },
        },
        INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: key,
                    dwFlags: KEYEVENTF_KEYUP,
                    ..Default::default()
                },
            },
        },
        INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: modifier,
                    dwFlags: KEYEVENTF_KEYUP,
                    ..Default::default()
                },
            },
        },
    ];
    SendInput(&mut inputs, std::mem::size_of::<INPUT>() as i32);
}

unsafe fn send_char(ch: char) {
    let mut inputs = [
        INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: VIRTUAL_KEY(0),
                    wScan: ch as u16,
                    dwFlags: KEYEVENTF_UNICODE,
                    ..Default::default()
                },
            },
        },
        INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: VIRTUAL_KEY(0),
                    wScan: ch as u16,
                    dwFlags: KEYEVENTF_UNICODE | KEYEVENTF_KEYUP,
                    ..Default::default()
                },
            },
        },
    ];
    SendInput(&mut inputs, std::mem::size_of::<INPUT>() as i32);
}
