//! The native macOS Service receives file URLs from Finder. It never simulates Copy.
use objc2::{
    msg_send,
    rc::Retained,
    runtime::{AnyClass, AnyObject, ClassBuilder, Sel},
    sel,
};
use std::{ffi::CStr, sync::OnceLock};
use tauri::AppHandle;
static APP: OnceLock<AppHandle> = OnceLock::new();
unsafe fn text(value: &str) -> Option<Retained<AnyObject>> {
    let class = AnyClass::get(c"NSString")?;
    let value = std::ffi::CString::new(value).ok()?;
    let string: *mut AnyObject = unsafe { msg_send![class,stringWithUTF8String:value.as_ptr()] };
    unsafe { Retained::retain(string) }
}
unsafe fn rust_string(value: *mut AnyObject) -> Option<String> {
    if value.is_null() {
        return None;
    }
    let bytes: *const std::ffi::c_char = unsafe { msg_send![value, UTF8String] };
    if bytes.is_null() {
        None
    } else {
        Some(
            unsafe { CStr::from_ptr(bytes) }
                .to_string_lossy()
                .into_owned(),
        )
    }
}
extern "C-unwind" fn receive(
    _: *mut AnyObject,
    _: Sel,
    pasteboard: *mut AnyObject,
    _user: *mut AnyObject,
    error: *mut *mut AnyObject,
) {
    let result = objc2::rc::autoreleasepool(|_| -> Result<(), String> {
        if pasteboard.is_null() {
            return Err("Select one Desktop file".into());
        }
        let kind =
            unsafe { text("NSFilenamesPboardType") }.ok_or("The file URL type is unavailable")?;
        let paths: *mut AnyObject = unsafe { msg_send![pasteboard,propertyListForType:&*kind] };
        if paths.is_null() {
            return Err("macOS did not supply a file URL".into());
        }
        let count: usize = unsafe { msg_send![paths, count] };
        if count != 1 {
            return Err("One file at a time".into());
        }
        let path: *mut AnyObject = unsafe { msg_send![paths,objectAtIndex:0usize] };
        let path = unsafe { rust_string(path) }.ok_or("The selected file URL is unavailable")?;
        let app = APP
            .get()
            .ok_or("RichOS is starting. Try again when it is ready")?;
        crate::cleaner_host::service(app.clone(), std::path::PathBuf::from(path))
    });
    if let Err(message) = result {
        if !error.is_null() {
            if let Some(value) = unsafe { text(&message) } {
                unsafe {
                    *error = Retained::autorelease_ptr(value);
                }
            }
        }
        eprintln!("[richos] RichCleaner Service: {message}");
    }
}
pub fn register(app: &AppHandle) -> Result<(), String> {
    APP.set(app.clone())
        .map_err(|_| "RichCleaner Service is already registered")?;
    let superclass = AnyClass::get(c"NSObject").ok_or("macOS services are unavailable")?;
    let mut builder = ClassBuilder::new(c"RichCleanerService", superclass)
        .ok_or("The Service class is unavailable")?;
    // SAFETY: NSService's documented selector takes pasteboard, userData and NSString**.
    unsafe {
        builder.add_method(
            sel!(richCleanerNote:userData:error:),
            receive
                as extern "C-unwind" fn(
                    *mut AnyObject,
                    Sel,
                    *mut AnyObject,
                    *mut AnyObject,
                    *mut *mut AnyObject,
                ),
        );
    }
    let class = builder.register();
    let provider: Retained<AnyObject> = unsafe { msg_send![class, new] };
    let application =
        AnyClass::get(c"NSApplication").ok_or("The macOS application is unavailable")?;
    let instance: *mut AnyObject = unsafe { msg_send![application, sharedApplication] };
    // NSApplication retains its Services provider for the application's lifetime.
    unsafe {
        let _: () = msg_send![instance,setServicesProvider:&*provider];
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn the_service_reads_supplied_file_paths_and_refuses_zero_or_multiple_files() {
        objc2::rc::autoreleasepool(|_| unsafe {
            let class = AnyClass::get(c"NSPasteboard").unwrap();
            for (count, expected) in [
                (0usize, "One file at a time"),
                (2, "One file at a time"),
                (1, "RichOS is starting"),
            ] {
                let board: *mut AnyObject = msg_send![class, pasteboardWithUniqueName];
                let path = text("/Users/fixture/Desktop/selected.png").unwrap();
                let objects = [&*path as *const AnyObject, &*path as *const AnyObject];
                let array: *mut AnyObject = msg_send![AnyClass::get(c"NSArray").unwrap(),arrayWithObjects:objects.as_ptr(), count:count];
                let kind = text("NSFilenamesPboardType").unwrap();
                let _: bool = msg_send![board,setPropertyList:array, forType:&*kind];
                let mut error = std::ptr::null_mut();
                receive(
                    std::ptr::null_mut(),
                    sel!(richCleanerNote:userData:error:),
                    board,
                    std::ptr::null_mut(),
                    &mut error,
                );
                let message = rust_string(error).unwrap();
                assert!(message.contains(expected), "{count} selected: {message}");
                let _: () = msg_send![board, releaseGlobally];
            }
        });
    }
}
