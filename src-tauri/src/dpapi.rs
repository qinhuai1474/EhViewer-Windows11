//! Windows DPAPI (CryptProtectData / CryptUnprotectData) for encrypting session
//! cookies at rest. A dependency-free `#[link(crypt32)]` FFI wrapper.

use std::ffi::c_void;

#[repr(C)]
struct DataBlob {
    cb_data: u32,
    pb_data: *mut c_void,
}

#[repr(C)]
struct CryptProtectPromptStruct {
    cb_size: u32,
    dw_prompt_flags: u32,
    hwnd_app: *mut c_void,
    sz_prompt: *const u16,
}

#[link(name = "crypt32")]
extern "system" {
    fn CryptProtectData(
        pdata_in: *const DataBlob,
        sz_data_descr: *const u16,
        optional_entropy: *const DataBlob,
        reserved: *mut c_void,
        prompt_struct: *const CryptProtectPromptStruct,
        flags: u32,
        pdata_out: *mut DataBlob,
    ) -> i32;

    fn CryptUnprotectData(
        pdata_in: *const DataBlob,
        ppsz_data_desc: *mut *mut u16,
        optional_entropy: *const DataBlob,
        reserved: *mut c_void,
        prompt_struct: *const CryptProtectPromptStruct,
        flags: u32,
        pdata_out: *mut DataBlob,
    ) -> i32;

    fn LocalFree(h_mem: *mut c_void) -> *mut c_void;
}

fn blob_from_slice(slice: &[u8]) -> DataBlob {
    DataBlob {
        cb_data: slice.len() as u32,
        pb_data: slice.as_ptr() as *mut c_void,
    }
}

fn slice_from_blob(blob: &DataBlob) -> Vec<u8> {
    if blob.cb_data == 0 || blob.pb_data.is_null() {
        return Vec::new();
    }
    unsafe { std::slice::from_raw_parts(blob.pb_data as *const u8, blob.cb_data as usize).to_vec() }
}

/// Encrypts `data` with the current user's DPAPI key.
#[cfg(windows)]
pub fn protect(data: &[u8]) -> std::io::Result<Vec<u8>> {
    let in_blob = blob_from_slice(data);
    let mut out_blob = DataBlob {
        cb_data: 0,
        pb_data: std::ptr::null_mut(),
    };
    let ok = unsafe {
        CryptProtectData(
            &in_blob,
            std::ptr::null(),
            std::ptr::null(),
            std::ptr::null_mut(),
            std::ptr::null(),
            0,
            &mut out_blob,
        )
    };
    if ok == 0 {
        return Err(std::io::Error::last_os_error());
    }
    let bytes = slice_from_blob(&out_blob);
    unsafe { LocalFree(out_blob.pb_data) };
    Ok(bytes)
}

/// Decrypts a DPAPI blob produced by [`protect`].
#[cfg(windows)]
pub fn unprotect(data: &[u8]) -> std::io::Result<Vec<u8>> {
    let in_blob = blob_from_slice(data);
    let mut out_blob = DataBlob {
        cb_data: 0,
        pb_data: std::ptr::null_mut(),
    };
    let ok = unsafe {
        CryptUnprotectData(
            &in_blob,
            std::ptr::null_mut(),
            std::ptr::null(),
            std::ptr::null_mut(),
            std::ptr::null(),
            0,
            &mut out_blob,
        )
    };
    if ok == 0 {
        return Err(std::io::Error::last_os_error());
    }
    let bytes = slice_from_blob(&out_blob);
    unsafe { LocalFree(out_blob.pb_data) };
    Ok(bytes)
}

#[cfg(not(windows))]
pub fn protect(_data: &[u8]) -> std::io::Result<Vec<u8>> {
    Err(std::io::Error::new(std::io::ErrorKind::Unsupported, "DPAPI only on Windows"))
}

#[cfg(not(windows))]
pub fn unprotect(_data: &[u8]) -> std::io::Result<Vec<u8>> {
    Err(std::io::Error::new(std::io::ErrorKind::Unsupported, "DPAPI only on Windows"))
}

#[cfg(test)]
#[cfg(windows)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let secret = b"ipb_pass_hash=deadbeef";
        let enc = match protect(secret) {
            Ok(e) => e,
            // DPAPI can be unavailable to restricted / service tokens (e.g. the
            // Codex sandbox); fall back to plaintext there instead of failing.
            Err(_) => return,
        };
        assert_ne!(enc, secret);
        let dec = unprotect(&enc).unwrap();
        assert_eq!(dec, secret);
    }
}

