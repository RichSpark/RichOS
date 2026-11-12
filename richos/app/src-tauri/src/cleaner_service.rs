//! The native macOS Service receives file URLs from Finder. It never simulates Copy.
use objc2::{
    msg_send,
    rc::Retained,
    runtime::{AnyClass, AnyObject, ClassBuilder, Sel},
    sel,
};
use richos_core::cleaner::MAX_BATCH_FILES;
use std::{ffi::CStr, path::PathBuf, sync::OnceLock};
use tauri::AppHandle;
static APP: OnceLock<AppHandle> = OnceLock::new();

unsafe fn text(value: &str) -> Option<Retained<AnyObject>> {
    let class = AnyClass::get(c"NSString")?;
    let value = std::ffi::CString::new(value).ok()?;
    let string: *mut AnyObject = unsafe { msg_send![class,stringWithUTF8String:value.as_ptr()] };
    unsafe { Retained::retain(string) }
}
unsafe fn rust_string(value: *mut AnyObject) -> Option<String> {
    let class = AnyClass::get(c"NSString")?;
    if value.is_null() {
        return None;
    }
    let is_string: bool = unsafe { msg_send![value, isKindOfClass:class] };
    if !is_string {
        return None;
    }
    let bytes: *const std::ffi::c_char = unsafe { msg_send![value, UTF8String] };
    if bytes.is_null() {
        None
    } else {
        let bytes = unsafe { CStr::from_ptr(bytes) }.to_bytes();
        const UTF8_ENCODING: usize = 4;
        let length: usize = unsafe { msg_send![value, lengthOfBytesUsingEncoding:UTF8_ENCODING] };
        // A NUL inside an NSString must never truncate a supplied path into another file.
        if bytes.len() != length {
            return None;
        }
        std::str::from_utf8(bytes).ok().map(str::to_owned)
    }
}

fn selected_paths(pasteboard: *mut AnyObject) -> Result<Vec<PathBuf>, String> {
    if pasteboard.is_null() {
        return Err("Select files on your Desktop".into());
    }
    let kind =
        unsafe { text("NSFilenamesPboardType") }.ok_or("The file URL type is unavailable")?;
    let paths: *mut AnyObject = unsafe { msg_send![pasteboard,propertyListForType:&*kind] };
    let array = AnyClass::get(c"NSArray").ok_or("The file URL list is unavailable")?;
    let is_array: bool = !paths.is_null() && unsafe { msg_send![paths, isKindOfClass:array] };
    if !is_array {
        return Err("macOS did not supply a list of file URLs".into());
    }
    let count: usize = unsafe { msg_send![paths, count] };
    if count == 0 {
        return Err("Select files on your Desktop".into());
    }
    if count > MAX_BATCH_FILES {
        return Err(format!("Select up to {MAX_BATCH_FILES} Desktop files"));
    }
    (0..count)
        .map(|index| {
            let path: *mut AnyObject = unsafe { msg_send![paths, objectAtIndex:index] };
            let path = unsafe { rust_string(path) }.ok_or("The selected file URL is unavailable")?;
            Ok(PathBuf::from(path))
        })
        .collect()
}

extern "C-unwind" fn receive(
    _: *mut AnyObject,
    _: Sel,
    pasteboard: *mut AnyObject,
    _user: *mut AnyObject,
    error: *mut *mut AnyObject,
) {
    let result = objc2::rc::autoreleasepool(|_| -> Result<(), String> {
        let paths = selected_paths(pasteboard)?;
        let app = APP
            .get()
            .ok_or("RichOS is starting. Try again when it is ready")?;
        crate::cleaner_host::service(app.clone(), paths)
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
    fn the_service_reads_each_supplied_path_and_refuses_empty_or_oversized_selections() {
        objc2::rc::autoreleasepool(|_| unsafe {
            let class = AnyClass::get(c"NSPasteboard").unwrap();
            for (count, expected) in [
                (0usize, Some("Select files on your Desktop")),
                (1, None),
                (2, None),
                (MAX_BATCH_FILES, None),
                (MAX_BATCH_FILES + 1, Some("Select up to")),
            ] {
                let board: *mut AnyObject = msg_send![class, pasteboardWithUniqueName];
                let names = (0..count)
                    .map(|i| format!("/Users/fixture/Desktop/selected {i}.png"))
                    .collect::<Vec<_>>();
                let paths = names.iter().map(|name| text(name).unwrap()).collect::<Vec<_>>();
                let objects = paths.iter().map(|path| &**path as *const AnyObject).collect::<Vec<_>>();
                let array: *mut AnyObject = msg_send![AnyClass::get(c"NSArray").unwrap(),arrayWithObjects:objects.as_ptr(), count:count];
                let kind = text("NSFilenamesPboardType").unwrap();
                let _: bool = msg_send![board,setPropertyList:array, forType:&*kind];
                let result = selected_paths(board);
                if let Some(expected) = expected {
                    let message = result.unwrap_err();
                    assert!(message.contains(expected), "{count} selected: {message}");
                } else {
                    assert_eq!(result.unwrap(), names.into_iter().map(PathBuf::from).collect::<Vec<_>>());
                }
                let _: () = msg_send![board, releaseGlobally];
            }
        });
    }
}
