use crate::domain::AppError;

#[cfg(windows)]
fn transform(input: &[u8], encrypt: bool) -> Result<Vec<u8>, AppError> {
    use std::ptr::{null, null_mut};
    use windows_sys::Win32::{Foundation::LocalFree, Security::Cryptography::{CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN, CryptProtectData, CryptUnprotectData}};
    if input.is_empty() || input.len() > u32::MAX as usize { return Err(secret_error()); }
    let source = CRYPT_INTEGER_BLOB { cbData: input.len() as u32, pbData: input.as_ptr() as *mut u8 };
    let mut output = CRYPT_INTEGER_BLOB { cbData: 0, pbData: null_mut() };
    // DPAPI borrows source for this call; output is allocated by Windows and freed below.
    // User scope (no LOCAL_MACHINE flag). No credential material is logged or returned to JS.
    unsafe {
        let ok = if encrypt {
            CryptProtectData(&source, null(), null(), null(), null(), CRYPTPROTECT_UI_FORBIDDEN, &mut output)
        } else {
            CryptUnprotectData(&source, null_mut(), null(), null(), null(), CRYPTPROTECT_UI_FORBIDDEN, &mut output)
        };
        if ok == 0 { return Err(secret_error()); }
        let result = std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec();
        if !encrypt {
            use zeroize::Zeroize;
            std::slice::from_raw_parts_mut(output.pbData, output.cbData as usize).zeroize();
        }
        LocalFree(output.pbData.cast());
        Ok(result)
    }
}

#[cfg(not(windows))]
fn transform(_: &[u8], _: bool) -> Result<Vec<u8>, AppError> { Err(secret_error()) }

fn secret_error() -> AppError { AppError::new("credential", "无法使用 Windows 用户加密保护密钥，请在当前 Windows 账户重新配置。") }
pub fn protect(input: &[u8]) -> Result<Vec<u8>, AppError> { transform(input, true) }
pub fn unprotect(input: &[u8]) -> Result<zeroize::Zeroizing<String>, AppError> {
    let bytes = zeroize::Zeroizing::new(transform(input, false)?);
    let value = std::str::from_utf8(&bytes).map_err(|_| secret_error())?;
    Ok(zeroize::Zeroizing::new(value.to_owned()))
}

#[cfg(all(test, windows))]
mod tests {
    #[test]
    fn user_scoped_roundtrip_and_corrupt_blob() {
        let sample = b"test-only-not-an-api-key";
        let encrypted = super::protect(sample).unwrap();
        assert_ne!(encrypted, sample);
        assert_eq!(super::unprotect(&encrypted).unwrap().as_bytes(), sample);
        assert!(super::unprotect(b"invalid").is_err());
    }
}
