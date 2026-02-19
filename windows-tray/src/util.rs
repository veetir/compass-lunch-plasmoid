use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;

pub fn to_wstring(value: &str) -> Vec<u16> {
    OsStr::new(value).encode_wide().chain(Some(0)).collect()
}
