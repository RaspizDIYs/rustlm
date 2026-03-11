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
pub fn login_to_riot_client(username: &str, password: &str, timeout_secs: u64) -> Result<()> {
    unsafe {
        CoInitializeEx(Some(std::ptr::null()), COINIT_APARTMENTTHREADED).ok()?;
    }

    let result = unsafe { do_login(username, password, timeout_secs) };

    unsafe {
        CoUninitialize();
    }

    result
}

unsafe fn do_login(username: &str, password: &str, timeout_secs: u64) -> Result<()> {
    let automation: IUIAutomation =
        CoCreateInstance(&CUIAutomation8, None, CLSCTX_INPROC_SERVER)?;

    let deadline = std::time::Instant::now() + Duration::from_secs(timeout_secs);

    // Find Riot Client window
    let hwnd = wait_for_riot_client_window(deadline)?;

    // Activate window
    activate_window(hwnd);
    std::thread::sleep(Duration::from_millis(200));

    let element = automation.ElementFromHandle(hwnd)?;

    // Find login fields with retry
    let (username_el, password_el, sign_in_el) =
        find_login_elements(&automation, &element, hwnd, deadline)?;

    // Input username
    set_element_value(&username_el, username)?;
    std::thread::sleep(Duration::from_millis(50));

    // Verify username was set
    verify_value(&username_el, username);

    // Input password
    set_element_value(&password_el, password)?;
    std::thread::sleep(Duration::from_millis(50));

    // Try to check "Remember Me" checkbox
    let _ = try_check_remember_me(&automation, &element);

    // Click sign in
    std::thread::sleep(Duration::from_millis(100));
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

fn wait_for_riot_client_window(deadline: std::time::Instant) -> Result<HWND> {
    let process_names = ["RiotClientUx", "RiotClientUxRender", "Riot Client"];

    loop {
        if std::time::Instant::now() > deadline {
            return Err(Error::new(E_FAIL, "Riot Client window not found within timeout"));
        }

        for name in &process_names {
            if let Some(hwnd) = find_process_main_window(name) {
                return Ok(hwnd);
            }
        }

        std::thread::sleep(Duration::from_millis(250));
    }
}

fn find_process_main_window(process_name: &str) -> Option<HWND> {
    use std::process::Command;

    let output = Command::new("tasklist")
        .args(["/fi", &format!("imagename eq {}.exe", process_name), "/fo", "csv", "/nh"])
        .output()
        .ok()?;

    let text = String::from_utf8_lossy(&output.stdout);
    if text.trim().is_empty() || text.contains("INFO: No tasks") {
        return None;
    }

    // Parse PID from CSV: "name","pid",...
    let pid: u32 = text.lines()
        .next()?
        .split(',')
        .nth(1)?
        .trim_matches('"')
        .parse()
        .ok()?;

    // Enumerate windows for this PID
    find_main_window_for_pid(pid)
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
) -> Result<(IUIAutomationElement, IUIAutomationElement, Option<IUIAutomationElement>)> {
    let mut scan_cycles = 0u32;

    loop {
        if std::time::Instant::now() > deadline {
            return Err(Error::new(E_FAIL, "Login fields not found within timeout"));
        }

        scan_cycles += 1;

        // Periodic window activation
        if scan_cycles % 10 == 0 {
            activate_window(hwnd);
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
                    std::thread::sleep(Duration::from_millis(80));
                    continue;
                }
            }
        };

        // Find sign-in button
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
