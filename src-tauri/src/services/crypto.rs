use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;

use crate::error::AppError;

// --- DPAPI (Windows only) ---

#[cfg(windows)]
unsafe fn local_free(ptr: *mut u8) {
    // LocalFree was removed from the `windows` crate in v0.61.
    // Call it via raw FFI since DPAPI allocates output with LocalAlloc.
    extern "system" {
        fn LocalFree(hmem: *mut u8) -> *mut u8;
    }
    LocalFree(ptr);
}

#[cfg(windows)]
pub fn dpapi_protect(data: &[u8]) -> Result<String, AppError> {
    use windows::Win32::Security::Cryptography::*;

    unsafe {
        let input_blob = CRYPT_INTEGER_BLOB {
            cbData: data.len() as u32,
            pbData: data.as_ptr() as *mut u8,
        };
        let mut output_blob = CRYPT_INTEGER_BLOB::default();

        CryptProtectData(
            &input_blob,
            None,
            None,
            None,
            None,
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output_blob,
        )
        .map_err(|e| AppError::Custom(format!("DPAPI Protect failed: {}", e)))?;

        let protected = std::slice::from_raw_parts(
            output_blob.pbData,
            output_blob.cbData as usize,
        )
        .to_vec();

        local_free(output_blob.pbData);

        Ok(BASE64.encode(&protected))
    }
}

#[cfg(not(windows))]
pub fn dpapi_protect(data: &[u8]) -> Result<String, AppError> {
    Ok(BASE64.encode(data))
}

#[cfg(windows)]
pub fn dpapi_unprotect(encrypted_b64: &str) -> Result<String, AppError> {
    use windows::Win32::Security::Cryptography::*;

    let protected_bytes = BASE64
        .decode(encrypted_b64)
        .map_err(|e| AppError::Custom(format!("Base64 decode failed: {}", e)))?;

    unsafe {
        let input_blob = CRYPT_INTEGER_BLOB {
            cbData: protected_bytes.len() as u32,
            pbData: protected_bytes.as_ptr() as *mut u8,
        };
        let mut output_blob = CRYPT_INTEGER_BLOB::default();

        CryptUnprotectData(
            &input_blob,
            None,
            None,
            None,
            None,
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output_blob,
        )
        .map_err(|e| AppError::Custom(format!("DPAPI Unprotect failed: {}", e)))?;

        let decrypted = std::slice::from_raw_parts(
            output_blob.pbData,
            output_blob.cbData as usize,
        )
        .to_vec();

        local_free(output_blob.pbData);

        String::from_utf8(decrypted)
            .map_err(|e| AppError::Custom(format!("UTF-8 decode failed: {}", e)))
    }
}

#[cfg(not(windows))]
pub fn dpapi_unprotect(encrypted_b64: &str) -> Result<String, AppError> {
    let bytes = BASE64
        .decode(encrypted_b64)
        .map_err(|e| AppError::Custom(format!("Base64 decode failed: {}", e)))?;
    String::from_utf8(bytes).map_err(|e| AppError::Custom(format!("UTF-8 decode failed: {}", e)))
}
