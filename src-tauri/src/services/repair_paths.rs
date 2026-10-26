use std::path::PathBuf;

pub fn root() -> Result<PathBuf, String> {
    Ok(dirs::data_dir()
        .ok_or("Не найдена папка данных RustLM")?
        .join("com.rustlm.app")
        .join("Repair Tools"))
}
