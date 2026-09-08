//! Inspect packaged Windows resources without launching any application window.
#[cfg(windows)]
fn main() {
    use std::ffi::c_void;
    use windows_sys::Win32::{
        Foundation::FreeLibrary,
        System::LibraryLoader::{
            FindResourceW, LoadLibraryExW, LoadResource, LockResource, SizeofResource,
        },
        UI::WindowsAndMessaging::DestroyIcon,
    };
    #[link(name = "shell32")]
    extern "system" {
        fn ExtractIconExW(
            path: *const u16,
            index: i32,
            large: *mut *mut c_void,
            small: *mut *mut c_void,
            count: u32,
        ) -> u32;
    }
    let path = std::env::args().nth(1).expect("executable path");
    let bytes = std::fs::read(&path).unwrap();
    let pe = u32::from_le_bytes(bytes[0x3c..0x40].try_into().unwrap()) as usize;
    assert_eq!(&bytes[pe..pe + 4], b"PE\0\0");
    let subsystem = u16::from_le_bytes(bytes[pe + 24 + 68..pe + 24 + 70].try_into().unwrap());
    assert_eq!(
        subsystem, 2,
        "GUI subsystem required: must not allocate a console"
    );
    let path: Vec<u16> = path.encode_utf16().chain(Some(0)).collect();
    unsafe {
        let module = LoadLibraryExW(path.as_ptr(), std::ptr::null_mut(), 2);
        assert!(!module.is_null());
        let group = FindResourceW(module, 1usize as *const u16, 14usize as *const u16);
        assert!(!group.is_null(), "embedded icon group");
        let size = SizeofResource(module, group) as usize;
        let data = LockResource(LoadResource(module, group));
        assert!(!data.is_null());
        let group = std::slice::from_raw_parts(data as *const u8, size);
        let count = u16::from_le_bytes(group[4..6].try_into().unwrap());
        assert_eq!(count, 7, "16,24,32,48,64,128,256 px icon images");
        assert_eq!(group[6], 16);
        assert_eq!(group[6 + 6 * 14], 0); // 256 encoded as zero.
        assert!(FreeLibrary(module) != 0);
        let mut large = std::ptr::null_mut();
        let mut small = std::ptr::null_mut();
        assert_eq!(
            ExtractIconExW(path.as_ptr(), 0, &mut large, &mut small, 1),
            2
        );
        assert!(
            !large.is_null() && !small.is_null(),
            "Windows shell must decode both icon sizes"
        );
        DestroyIcon(large);
        DestroyIcon(small);
    }
    println!("PASS: GUI subsystem (no console); seven embedded icon sizes; Windows Shell decodes large and small application icons.");
}
#[cfg(not(windows))]
fn main() {
    println!("Windows resource verification requires Windows.");
}
