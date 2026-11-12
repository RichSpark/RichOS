// Native file boundaries for RichCleaner. One JSON request on stdin, one JSON response.
// This helper never records audio and never chooses a folder. It is bundled with RichOS.
import Foundation
import AppKit
import ApplicationServices
import PDFKit
import Darwin

struct Failure: Error, CustomStringConvertible { let description: String; init(_ text: String) {description=text} }
let fm=FileManager.default
func url(_ path:String)->URL {URL(fileURLWithPath:path)}
func fail(_ text:String)throws->Never {throw Failure(text)}
func string(_ r:[String:Any],_ key:String)throws->String {guard let v=r[key] as? String,!v.isEmpty else {try fail("Missing \(key)")};return v}
func canonical(_ u:URL)throws->URL {
    let standardized=u.standardizedFileURL
    guard standardized==standardized.resolvingSymlinksInPath() else {try fail("Symbolic links are not filed")}
    return standardized
}
func identity(_ u:URL,local:Bool=true,makeBookmark:Bool=true)throws->[String:Any] {
    let u=try canonical(u)
    var s=stat();guard lstat(u.path,&s)==0 else {try fail(String(cString:strerror(errno)))}
    guard s.st_mode & S_IFMT == S_IFREG else {try fail("Select a regular file, not a folder or package")}
    let values=try u.resourceValues(forKeys:[.isPackageKey,.isUbiquitousItemKey,.ubiquitousItemIsDownloadingKey,.ubiquitousItemDownloadingStatusKey])
    guard values.isPackage != true else {try fail("Packages are not filed")}
    if local && values.isUbiquitousItem == true && values.ubiquitousItemDownloadingStatus != .current {
        try fm.startDownloadingUbiquitousItem(at:u)
        try fail("The file is downloading from iCloud. Try again when it is available")
    }
    let bookmark=try !makeBookmark || s.st_nlink>1 ? Data():u.bookmarkData(options:.suitableForBookmarkFile,includingResourceValuesForKeys:nil,relativeTo:nil)
    return ["volume":UInt64(UInt32(bitPattern:s.st_dev)),"object":s.st_ino,"size":UInt64(s.st_size),"modified":"\(s.st_mtimespec.tv_sec):\(s.st_mtimespec.tv_nsec)","changed":"\(s.st_ctimespec.tv_sec):\(s.st_ctimespec.tv_nsec)","bookmark":bookmark.base64EncodedString()]
}
func same(_ a:[String:Any],_ b:[String:Any],version:Bool=true)throws {
    for key in ["volume","object"]+(version ? ["size","modified","changed"]:[]) {
        guard String(describing:a[key]!)==String(describing:b[key]!) else {try fail("The selected file changed. Nothing was replaced")}
    }
}
func original(_ request: [String: Any]) throws -> (URL, [String: Any]) {
    guard let expected = request["identity"] as? [String: Any] else { try fail("Missing identity") }
    let path = url(try string(request, "path"))
    if let actual = try? identity(path, local: false, makeBookmark: false),
       (try? same(expected, actual, version: false)) != nil {
        return (path, actual)
    }
    if let text = expected["bookmark"] as? String, let data = Data(base64Encoded: text), !data.isEmpty {
        var stale = false
        if let target = try? URL(resolvingBookmarkData: data, options: [.withoutUI, .withoutMounting],
                                 relativeTo: nil, bookmarkDataIsStale: &stale),
           let actual = try? identity(target, local: false, makeBookmark: false),
           (try? same(expected, actual, version: false)) != nil {
            return (target, actual)
        }
    }
    try fail("Locate the original file")
}

let cloudKeys: Set<URLResourceKey> = [.isUbiquitousItemKey, .ubiquitousItemDownloadingStatusKey,
                                      .ubiquitousItemDownloadingErrorKey]
func needsDownload(_ target: URL) throws -> Bool {
    let values = try target.resourceValues(forKeys: cloudKeys)
    return values.isUbiquitousItem == true && values.ubiquitousItemDownloadingStatus != .current
}

func downloadOriginal(_ target: URL) throws {
    var target = target
    guard try needsDownload(target) else { return }
    try fm.startDownloadingUbiquitousItem(at: target)
    let start = DispatchTime.now().uptimeNanoseconds
    while DispatchTime.now().uptimeNanoseconds - start < 20_000_000_000 {
        target.removeAllCachedResourceValues()
        let values = try target.resourceValues(forKeys: cloudKeys)
        if values.ubiquitousItemDownloadingStatus == .current { return }
        if let error = values.ubiquitousItemDownloadingError { throw error }
        Thread.sleep(forTimeInterval: 0.25)
    }
    try fail("Still downloading from iCloud. Open again when the download finishes")
}
func checked(_ r:[String:Any],_ u:URL)throws->[String:Any] {
    guard let expected=r["identity"] as? [String:Any] else {try fail("Missing file identity")}
    let actual=try identity(u);try same(expected,actual);return actual
}
func coordinate(_ first:URL,_ second:URL?=nil,replacement:Bool=false,_ body:(URL,URL?)throws->[String:Any])throws->[String:Any] {
    let c=NSFileCoordinator(filePresenter:nil);var error:NSError?;var result:Result<[String:Any],Error>?
    if let second=second {
        c.coordinate(writingItemAt:first,options:replacement ? .forReplacing:.forMoving,writingItemAt:second,options:replacement ? .forMoving:.forReplacing,error:&error) {a,b in result=Result{try body(a,b)}}
    } else {
        c.coordinate(readingItemAt:first,options:[],error:&error) {a in result=Result{try body(a,nil)}}
    }
    if let error=error {throw error};guard let result=result else {try fail("File coordination did not complete")};return try result.get()
}
// copyfile/replaceItemAt can rewrite quarantine provenance and round creation
// dates. Preserve the original's raw values through the coordinated replacement.
struct OriginalMetadata {
    let created:timespec
    let quarantine:Data?
    init(_ source:URL)throws {
        var info=stat();guard lstat(source.path,&info)==0 else {try fail("Could not read original file dates")};created=info.st_birthtimespec
        let count=getxattr(source.path,"com.apple.quarantine",nil,0,0,0)
        if count<0 {guard errno==ENOATTR else {try fail("Could not read the original quarantine attribute")};quarantine=nil}
        else {guard count<=65536 else {try fail("The quarantine attribute is unexpectedly large")};var data=Data(count:count);let copied=data.withUnsafeMutableBytes{getxattr(source.path,"com.apple.quarantine",$0.baseAddress,count,0,0)};guard copied==count else {try fail("The original quarantine attribute changed")};quarantine=data}
    }
    func restore(_ target:URL)throws {
        var attrs=attrlist();attrs.bitmapcount=UInt16(ATTR_BIT_MAP_COUNT);attrs.commonattr=UInt32(ATTR_CMN_CRTIME);var time=created
        guard setattrlist(target.path,&attrs,&time,MemoryLayout<timespec>.size,0)==0 else {try fail("Could not preserve the original creation date")}
        if let data=quarantine {let result=data.withUnsafeBytes{setxattr(target.path,"com.apple.quarantine",$0.baseAddress,data.count,0,0)};guard result==0 else {try fail("Could not preserve the original quarantine attribute")}}
        else if removexattr(target.path,"com.apple.quarantine",0) != 0 && errno != ENOATTR {try fail("Could not preserve the original quarantine state")}
    }
}
func flushFolder(_ folder:URL)throws {
    let fd=open(folder.path,O_RDONLY);guard fd>=0 else {try fail("The move needs recovery: could not open its folder for sync")};defer{close(fd)}
    guard fsync(fd)==0 else {try fail("The move needs recovery: could not sync the folder")}
}
func ax(_ element:AXUIElement,_ key:String)->CFTypeRef? {var value:CFTypeRef?;guard AXUIElementCopyAttributeValue(element,key as CFString,&value) == .success else {return nil};return value}
func axString(_ e:AXUIElement,_ key:String)->String? {ax(e,key) as? String}
func front()->Int32? {NSWorkspace.shared.frontmostApplication?.processIdentifier}

// Keep this bound in step with the macOS Service's Rust entry point.
let maxSelection = 20

func selectedItems(_ focused: AXUIElement) -> [AXUIElement] {
    if let items = ax(focused, kAXSelectedChildrenAttribute) as? [AXUIElement], !items.isEmpty {
        return items
    }
    // When Finder focuses an icon, obtain its container's entire selection.
    // A selected focused icon alone doesn't tell us which other icons are selected.
    guard ax(focused, kAXSelectedAttribute) as? Bool == true,
          let parent = ax(focused, kAXParentAttribute),
          let items = ax(unsafeBitCast(parent, to: AXUIElement.self), kAXSelectedChildrenAttribute) as? [AXUIElement],
          items.contains(where: { CFEqual($0, focused) }) else { return [] }
    return items
}

func selectedURL(_ item: AXUIElement) throws -> URL {
    guard let raw = ax(item, kAXURLAttribute) else {
        try fail("macOS did not provide the selected file URL. Use Services")
    }
    if let target = raw as? URL, target.isFileURL { return target }
    if let text = raw as? String, let target = URL(string: text), target.isFileURL { return target }
    try fail("The selected file URL is unavailable. Use Services")
}

func selectionPayload(_ urls: [URL]) throws -> [String: Any] {
    guard !urls.isEmpty else { try fail("Select files on your Desktop, or use Services") }
    guard urls.count <= maxSelection else { try fail("Select up to \(maxSelection) Desktop files") }
    let desktop = try canonical(fm.url(for: .desktopDirectory, in: .userDomainMask, appropriateFor: nil, create: false))
    let paths = try urls.map(canonical).sorted { $0.path.utf8.lexicographicallyPrecedes($1.path.utf8) }
    var objects = Set<[UInt64]>()
    var targets = [[String: Any]]()
    for target in paths {
        guard target.deletingLastPathComponent() == desktop else {
            try fail("Select files directly on your Desktop")
        }
        let id = try identity(target)
        guard let volume = id["volume"] as? UInt64, let object = id["object"] as? UInt64,
              objects.insert([volume, object]).inserted else {
            try fail("The selection includes two names for the same file")
        }
        targets.append(["path": target.path, "identity": id])
    }
    if targets.count == 1 {
        var single = targets[0]
        single["route"] = "file"
        return single
    }
    return ["route": "file", "targets": targets]
}

func context()throws->[String:Any] {
    guard let app=NSWorkspace.shared.frontmostApplication else {try fail("The focused application is unavailable")}
    guard app.bundleIdentifier=="com.apple.finder" else {try fail("Focus changed. Select the files and try again")}
    guard AXIsProcessTrusted() else {try fail("Allow Accessibility in RichScribe settings, or use Services")}
    let finder=AXUIElementCreateApplication(app.processIdentifier)
    AXUIElementSetMessagingTimeout(finder,0.15)
    guard let value=ax(finder,kAXFocusedUIElementAttribute) else {try fail("Select files on your Desktop, or use Services")}
    let focused=unsafeBitCast(value,to:AXUIElement.self)
    let role=axString(focused,kAXRoleAttribute) ?? ""
    if [kAXTextFieldRole,kAXTextAreaRole,kAXComboBoxRole].contains(role) {
        guard front()==app.processIdentifier else {try fail("Focus changed. Try again")}
        return ["route":"text"]
    }
    // Read only the current focused file area, never Finder's retained global selection.
    // A Desktop icon has no ordinary Finder document window. The URL must also be directly
    // under the OS-provided Desktop, so another window's selected file cannot qualify.
    if let w=ax(focused,kAXWindowAttribute) {
        let window=unsafeBitCast(w,to:AXUIElement.self)
        if axString(window,kAXSubroleAttribute)==kAXStandardWindowSubrole {try fail("Select a file on your Desktop, or use Services")}
    }
    let selected = selectedItems(focused)
    guard selected.count <= maxSelection else { try fail("Select up to \(maxSelection) Desktop files") }
    var payload = try selectionPayload(selected.map(selectedURL))
    // Repeat the focused area and every selected element before returning. The owner
    // resolves all URLs and versions again immediately before opening the microphone.
    guard front()==app.processIdentifier else {try fail("Focus changed. Select the files and try again")}
    guard let again=ax(finder,kAXFocusedUIElementAttribute),CFEqual(value,again) else {try fail("Focus changed. Try again")}
    let selectedAgain = selectedItems(focused)
    guard selectedAgain.count == selected.count,
          selected.allSatisfy({ item in selectedAgain.contains(where: { CFEqual($0, item) }) }),
          front() == app.processIdentifier else {
        try fail("The selection changed. Try again")
    }
    payload["pid"] = app.processIdentifier
    return payload
}
func main(_ r:[String:Any])throws->[String:Any] {
    switch try string(r,"action") {
    case "context":return try context()
    case "service-context":
        guard let paths = r["paths"] as? [String],
              paths.allSatisfy({ $0.hasPrefix("/") && !$0.utf8.contains(0) }) else {
            try fail("macOS did not supply valid file URLs")
        }
        var payload = try selectionPayload(paths.map(url))
        payload["service"] = true
        return payload
    case "directories":
        return ["desktop":try fm.url(for:.desktopDirectory,in:.userDomainMask,appropriateFor:nil,create:false).path,"documents":try fm.url(for:.documentDirectory,in:.userDomainMask,appropriateFor:nil,create:false).path]
    case "root", "validate-root":
        let root=try canonical(url(string(r,"path")))
        let desktop=try canonical(fm.url(for:.desktopDirectory,in:.userDomainMask,appropriateFor:nil,create:false))
        guard root != desktop,!root.path.hasPrefix(desktop.path+"/") else {try fail("Choose a filing folder outside Desktop")}
        if !fm.fileExists(atPath:root.path){guard r["action"] as? String == "root" else {try fail("The configured filing folder is unavailable")};try fm.createDirectory(at:root,withIntermediateDirectories:true)}
        _=try canonical(root)
        var a=stat();var b=stat();guard stat(root.path,&a)==0,stat(desktop.path,&b)==0,a.st_dev==b.st_dev,a.st_mode & S_IFMT == S_IFDIR else {try fail("Choose a folder on the same disk as Desktop")}
        guard fm.isWritableFile(atPath:root.path) else {try fail("The filing folder is not writable")}
        return ["path":root.path]
    case "inspect":return try identity(url(string(r,"path")))
    case "resolve", "open-check", "open-location":
        var (target, actual) = try original(r)
        if r["action"] as? String == "open-check" {
            return ["path":target.path,"identity":actual,"download":try needsDownload(target)]
        }
        if r["action"] as? String == "open-location" {
            try downloadOriginal(target)
            // The file may have moved or been replaced while iCloud was downloading.
            (target, actual) = try original(r)
            try same(actual, identity(target, makeBookmark: false), version: false)
        }
        return ["path":target.path,"identity":actual]
    case "destination":
        let root=try canonical(url(string(r,"root")));let parts=r["parts"] as? [String] ?? [];var folder=root
        for part in parts {guard !part.isEmpty,!part.hasPrefix("."),!part.contains("/"),!part.contains(":"),!part.contains("\\") else {try fail("Invalid folder component")};folder.appendPathComponent(part,isDirectory:true);if !fm.fileExists(atPath:folder.path){try fm.createDirectory(at:folder,withIntermediateDirectories:false)};_ = try canonical(folder)}
        let name=try string(r,"name");guard name==URL(fileURLWithPath:name).lastPathComponent else {try fail("Invalid filename")}
        let original=folder.appendingPathComponent(name);var candidate=original;var suffix=2
        while fm.fileExists(atPath:candidate.path) {candidate=folder.appendingPathComponent(original.deletingPathExtension().lastPathComponent+" \(suffix)"+(original.pathExtension.isEmpty ? "":"."+original.pathExtension));suffix+=1}
        return ["path":candidate.path]
    case "pdf-protection":
        let u=try canonical(url(string(r,"path")))
        guard let document=PDFDocument(url:u) else {try fail("The PDF cannot be read safely")}
        if document.isEncrypted || document.isLocked {return ["protected":true,"reason":"The PDF is encrypted or locked, so its bytes were left unchanged"]}
        guard let data=CGDataProvider(url:u as CFURL),let pdf=CGPDFDocument(data) else {try fail("The PDF structure cannot be read safely")}
        let catalog=pdf.catalog;var permissions:CGPDFDictionaryRef?
        if let catalog=catalog,CGPDFDictionaryGetDictionary(catalog,"Perms",&permissions) {return ["protected":true,"reason":"The PDF contains signature permissions, so its bytes were left unchanged"]}
        var form:CGPDFDictionaryRef?;var flags:CGPDFInteger=0
        if let catalog=catalog,CGPDFDictionaryGetDictionary(catalog,"AcroForm",&form),let form=form,CGPDFDictionaryGetInteger(form,"SigFlags",&flags),flags != 0 {return ["protected":true,"reason":"The PDF contains a signature, so its bytes were left unchanged"]}
        for index in 0..<document.pageCount {if let page=document.page(at:index),page.annotations.contains(where:{$0.widgetFieldType == .signature}) {return ["protected":true,"reason":"The PDF has a signature field, so its bytes were left unchanged"]}}
        return ["protected":false]
    case "reserve":
        let source=try canonical(url(string(r,"path")))
        return try coordinate(source) {a,_ in
            _=try checked(r,a)
            var info=stat();guard lstat(a.path,&info)==0,info.st_nlink<=1 else {try fail("This file has multiple hard links. Its note is saved, but the file was not annotated or sorted")}
            let directory=try fm.url(for:.itemReplacementDirectory,in:.userDomainMask,appropriateFor:a,create:true)
            let staged=directory.appendingPathComponent(UUID().uuidString+"."+a.pathExtension)
            try fm.setAttributes([.posixPermissions:0o700],ofItemAtPath:directory.path)
            var reserved=try identity(a);reserved["object"]=UInt64(0);reserved["size"]=UInt64(0);reserved["modified"]="";reserved["changed"]="";reserved["bookmark"]=""
            return ["path":staged.path,"identity":reserved]
        }
    case "materialize":
        let source=try canonical(url(string(r,"path")));let staged=try canonical(url(string(r,"staged")))
        return try coordinate(source) {a,_ in
            _=try checked(r,a)
            var directory=stat();guard lstat(staged.deletingLastPathComponent().path,&directory)==0,directory.st_uid==geteuid(),directory.st_mode & 0o077==0,staged.path.contains("TemporaryItems/"),UUID(uuidString:staged.deletingPathExtension().lastPathComponent) != nil else {try fail("The staging directory is not owned by this operation")}
            try fm.copyItem(at:a,to:staged)
            let attributes=try fm.attributesOfItem(atPath:staged.path)
            try fm.setAttributes([.posixPermissions:((attributes[.posixPermissions] as? NSNumber)?.intValue ?? 0o600) | 0o600],ofItemAtPath:staged.path)
            return try identity(staged)
        }
    case "attributes":
        let source=try canonical(url(string(r,"path")));let staged=try canonical(url(string(r,"staged")))
        _=try checked(r,source)
        let metadata=try OriginalMetadata(source)
        guard copyfile(source.path,staged.path,nil,copyfile_flags_t(COPYFILE_METADATA))==0 else {try fail("Could not preserve the file attributes")}
        try metadata.restore(staged)
        let handle=try FileHandle(forReadingFrom:staged);try handle.synchronize();try handle.close()
        return try identity(staged)
    case "replace":
        let source=try canonical(url(string(r,"path")));let staged=try canonical(url(string(r,"staged")))
        return try coordinate(source,staged,replacement:true) {a,b in
            _=try checked(r,a)
            guard let expected=r["stagedIdentity"] as? [String:Any],let b=b else {try fail("Missing staged identity")}
            try same(expected,identity(b))
            let metadata=try OriginalMetadata(a)
            // copyItem preserves ACL/xattrs; usingNewMetadataOnly retains the staged copy's
            // metadata rather than allowing the replacement API to silently choose fields.
            _=try fm.replaceItemAt(a,withItemAt:b,backupItemName:nil,options:.usingNewMetadataOnly)
            try metadata.restore(a)
            let handle=try FileHandle(forReadingFrom:a);try handle.synchronize();try handle.close()
            try flushFolder(a.deletingLastPathComponent())
            return try identity(a)
        }
    case "rename":
        let source=try canonical(url(string(r,"path")));let destination=try canonical(url(string(r,"destination")))
        return try coordinate(source,destination) {a,b in
            _=try checked(r,a);guard let b=b else {try fail("Missing destination")}
            var parent=stat();guard stat(b.deletingLastPathComponent().path,&parent)==0 else {try fail("Destination is unavailable")}
            let expected=r["identity"] as! [String:Any]
            guard (expected["volume"] as? NSNumber)?.uint32Value==UInt32(bitPattern:parent.st_dev) else {try fail("Choose a folder on the same disk as Desktop")}
            guard renameatx_np(AT_FDCWD,a.path,AT_FDCWD,b.path,UInt32(RENAME_EXCL))==0 else {if errno==EEXIST{try fail("destination-collision")};try fail(String(cString:strerror(errno)))}
            for parent in [a.deletingLastPathComponent(),b.deletingLastPathComponent()] {try flushFolder(parent)}
            return try identity(b)
        }
    case "cleanup":
        let staged=try canonical(url(string(r,"path")))
        // Only operation-owned UUID files in the native replacement directory are removable.
        guard UUID(uuidString:staged.deletingPathExtension().lastPathComponent) != nil,staged.path.contains("TemporaryItems/") else {try fail("Refused to remove an unowned temporary file")}
        var directory=stat();let found=lstat(staged.deletingLastPathComponent().path,&directory);if found != 0 && errno==ENOENT{return ["ok":true]};guard found==0,directory.st_uid==geteuid(),directory.st_mode & 0o077==0 else {try fail("Refused to clean an unowned staging directory")}
        // Each reservation owns one unique native replacement directory. Remove its
        // interrupted writer outputs as well as the staged file, never a source folder.
        try fm.removeItem(at:staged.deletingLastPathComponent())
        return ["ok":true]
    default:try fail("Unknown RichCleaner native operation")
    }
}
do {
    let data=FileHandle.standardInput.readDataToEndOfFile();guard data.count<=256*1024 else {throw Failure("Request too large")}
    guard let request=try JSONSerialization.jsonObject(with:data) as? [String:Any] else {throw Failure("Invalid request")}
    let answer=try main(request);FileHandle.standardOutput.write(try JSONSerialization.data(withJSONObject:["ok":answer],options:[.sortedKeys]))
} catch {FileHandle.standardOutput.write(try! JSONSerialization.data(withJSONObject:["error":String(describing:error)]));exit(1)}
