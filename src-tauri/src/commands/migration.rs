use std::path::PathBuf;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x08000000;

fn lolmanager_dir() -> Option<PathBuf> {
    dirs::data_local_dir().map(|d| d.join("LolManager"))
}

#[tauri::command]
pub fn check_lolmanager_installed() -> bool {
    if let Some(dir) = lolmanager_dir() {
        // Velopack installs Update.exe in the app root directory
        dir.join("Update.exe").exists()
    } else {
        false
    }
}

#[tauri::command]
pub fn uninstall_lolmanager() -> Result<(), String> {
    let dir = lolmanager_dir().ok_or("Cannot determine LocalAppData")?;
    let update_exe = dir.join("Update.exe");

    if !update_exe.exists() {
        return Err("LolManager не найден".to_string());
    }

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        use std::process::Command;

        Command::new(&update_exe)
            .arg("--uninstall")
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .map_err(|e| format!("Не удалось запустить деинсталляцию: {}", e))?;
    }

    Ok(())
}
