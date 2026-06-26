use std::path::Path;

unsafe extern "C" {

    fn xmlInitParser();

    fn validate_model_description(
        model_description_path: *const i8,
        fmi_major_version: i32,
        messages: *mut *mut *const i8,
    ) -> i32;

    fn free_messages(len: i32, messages: *mut *const i8);

}

/// Call this once to ensure the linker pulls in libxml2 symbols
/// and the library is initialized.
pub fn init_libxml() {
    unsafe {
        xmlInitParser();
    }
}

/// Validates the modelDescription.xml against the XSD schema for the given FMI version.
pub fn validate_model_description_against_xsd(
    model_description_path: &Path,
    fmi_major_version: i32,
) -> Vec<String> {
    use std::ffi::CString;

    init_libxml();

    let path = model_description_path.to_str().unwrap();

    let path_cstring = CString::new(path).expect("Path contains null bytes");

    let mut messages: *mut *const i8 = std::ptr::null_mut();

    let n_messages = unsafe {
        validate_model_description(path_cstring.as_ptr(), fmi_major_version, &mut messages)
    };

    if n_messages > 0 && !messages.is_null() {
        let messages_slice = unsafe { std::slice::from_raw_parts(messages, n_messages as usize) };
        let messages_vec: Vec<String> = messages_slice
            .iter()
            .filter_map(|&msg_ptr| {
                if !msg_ptr.is_null() {
                    let msg_cstr = unsafe { std::ffi::CStr::from_ptr(msg_ptr) };
                    msg_cstr.to_str().ok().map(|s| s.trim_end().to_string())
                } else {
                    None
                }
            })
            .collect();
        unsafe { free_messages(n_messages, messages) };
        messages_vec
    } else {
        vec![]
    }
}
