//! The native macOS Services receive selections from other apps on a service pasteboard.
use objc2::{
    msg_send,
    rc::Retained,
    runtime::{AnyClass, AnyObject, ClassBuilder, Sel},
    sel,
};
use richos_core::cleaner::MAX_BATCH_FILES;
use std::{ffi::CStr, path::PathBuf, sync::{Mutex, OnceLock}};
use tauri::{AppHandle, Emitter};
static APP: OnceLock<AppHandle> = OnceLock::new();
static SEARCH_QUERY: Mutex<Option<String>> = Mutex::new(None);
const SEARCH_TEXT_TYPE: &str = "public.utf8-plain-text";

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
        respond_with_error(error, &message);
        eprintln!("[richos] RichCleaner Service: {message}");
    }
}

fn respond_with_error(error: *mut *mut AnyObject, message: &str) {
    if !error.is_null() {
        if let Some(value) = unsafe { text(message) } {
            unsafe { *error = Retained::autorelease_ptr(value); }
        }
    }
}

fn selected_query(pasteboard: *mut AnyObject) -> Result<String, String> {
    if pasteboard.is_null() {
        return Err("No search text was provided".into());
    }
    let kind = unsafe { text(SEARCH_TEXT_TYPE) }.ok_or("The search text type is unavailable")?;
    let value: *mut AnyObject = unsafe { msg_send![pasteboard, stringForType: &*kind] };
    let query = unsafe { rust_string(value) }.ok_or("The search text is unavailable")?;
    if query.len() > 4096 {
        return Err("The search text is too long".into());
    }
    Ok(query.lines().map(str::trim).collect::<Vec<_>>().join(" ").trim().to_owned())
}

extern "C-unwind" fn receive_search(
    _: *mut AnyObject,
    _: Sel,
    pasteboard: *mut AnyObject,
    _user: *mut AnyObject,
    error: *mut *mut AnyObject,
) {
    let result = objc2::rc::autoreleasepool(|_| -> Result<(), String> {
        let query = selected_query(pasteboard)?;
        let app = APP.get().ok_or("RichOS is starting. Try again when it is ready")?;
        *SEARCH_QUERY.lock().map_err(|_| "RichFinder is unavailable")? = Some(query);
        let app = app.clone();
        // Window creation can send work back to the event loop, so leave this callback first.
        std::thread::spawn(move || {
            crate::reopen_window(&app);
            if let Some(window) = crate::conversation_window(&app) {
                if let Err(error) = window.emit("rich://finder-service", ()) {
                    eprintln!("[richos] RichFinder Service notification failed: {error}");
                }
            }
        });
        Ok(())
    });
    if let Err(message) = result {
        respond_with_error(error, &message);
        eprintln!("[richos] RichFinder Service: {message}");
    }
}

#[richos_command::command]
pub fn finder_take_service_query(window: tauri::WebviewWindow) -> Result<Option<String>, String> {
    if window.label() != "main" {
        return Err("Open RichFinder in the main RichOS window".into());
    }
    Ok(SEARCH_QUERY.lock().map_err(|_| "RichFinder is unavailable")?.take())
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
        builder.add_method(
            sel!(richFinderSearch:userData:error:),
            receive_search as extern "C-unwind" fn(
                *mut AnyObject, Sel, *mut AnyObject, *mut AnyObject, *mut *mut AnyObject,
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
    fn the_search_service_uses_only_bounded_text_from_its_pasteboard() {
        objc2::rc::autoreleasepool(|_| unsafe {
            let class = AnyClass::get(c"NSPasteboard").unwrap();
            let board: *mut AnyObject = msg_send![class, pasteboardWithUniqueName];
            let kind = text(SEARCH_TEXT_TYPE).unwrap();
            assert!(selected_query(board).is_err());
            let query = text("  holiday photos\nfrom last year  ").unwrap();
            let written: bool = msg_send![board, setString: &*query, forType: &*kind];
            assert!(written);
            assert_eq!(selected_query(board).unwrap(), "holiday photos from last year");
            let oversized = text(&"é".repeat(2049)).unwrap();
            let written: bool = msg_send![board, setString: &*oversized, forType: &*kind];
            assert!(written);
            assert!(selected_query(board).unwrap_err().contains("too long"));
            let _: () = msg_send![board, releaseGlobally];
        });
    }
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
