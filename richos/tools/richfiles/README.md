# RichFiles

RichFiles is a macOS file browser with a Windows-style List view and keyboard preset. List shows the full filename and extension. The same browser runs inside the RichOS navigation rail and as a standalone app.

## Build and open

Use the repository's macOS build environment with Xcode, Swift and Cargo. Set `RICHFILES_OUTPUT` to an absolute path for a new output directory outside the checkout, then run this from the repository root.

```sh
bash richos/app/scripts/package-richfiles.sh "$RICHFILES_OUTPUT"
open "$RICHFILES_OUTPUT/RichFiles.app"
```

The script builds and ad hoc signs a development bundle. RichOS packaging uses the same script with `--embedded` to build the optimized app and the native library for the navigation rail. The Swift package targets macOS 13 and later. Saved native Details column layouts use macOS 14 and later.

## Keyboard and macOS integration

- In the Windows preset, Ctrl+L focuses the address, Ctrl+C copies files, Ctrl+X cuts, Ctrl+V pastes and Ctrl+Z undoes a supported action. Enter opens, F2 renames, Space previews and Delete moves to Trash.
- RichFiles Settings offers the Mac preset and individual shortcut overrides. In the docked browser, open **More → RichFiles Settings**.
- **Use RichFiles for Folders** sets the macOS folder association. **Restore Previous App** restores the previous association. macOS still owns the Desktop and system Open and Save dialogs. **Reveal in Finder** still opens Finder.
- For a shortcut that opens the standalone app from other apps, install RichFiles in an Applications folder and open it once. Assign **Open RichFiles** in **System Settings → Keyboard → Keyboard Shortcuts → Services**.
- **Undock RichFiles** opens the current tabs and panes in the standalone app. **Dock in RichOS** returns the workspace to the RichOS rail. Transfers stop before the workspace switches.

Core browsing and file operations work locally. The optional RichFinder and RichCleaner menu actions use the corresponding RichOS services when installed.

RichGlance ships inside RichFiles as an optional viewer for PNG, JPEG (`.jpg`, `.jpeg` and `.jpe`), HEIC, HEIF, WebP, AVIF, GIF, TIFF, BMP, ICO and ICNS. RichFiles and Finder open images in the app chosen in macOS. Use **Open With** to try RichGlance on one file or choose it under **Get Info → Open with → Change All** in Finder for each image type you want it to open by default. RichFiles registers the viewer when first opened. RichGlance can open files while both RichFiles and RichOS are closed.

In RichGlance, press Left or Right to cycle through images in that folder, Control and scroll to zoom, drag to pan and Escape to close. Animated GIFs and WebPs play automatically. TIFF opens on its first page. **File → Open in Preview** opens the current image for editing.

The existing RichFiles timing log also records RichGlance decode, canvas update, folder scan and Preview handoff outcomes without storing filenames or paths. It is included in RichDoctor's Configuration diagnostics.

On volumes such as exFAT, regular file copies reserve the destination name exclusively after staging the file. A failed publication keeps the source and reports the output for review. Rename, move into and staged folder copy require the destination volume to support exclusive rename.

## Verify

Run the native suite and packaged archive checks from the repository root.

```sh
bash richos/app/scripts/run-tests.sh --no-host-screen --only richfiles.test.sh
```

RichFiles is part of the [GNU AGPL v3 licensed source](../../../LICENSE).
