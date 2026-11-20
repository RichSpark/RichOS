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
- In **RichFiles Settings → Keyboard → Mac desktop**, turn on **Press Delete to move selected desktop items to Trash**. RichFiles uses its Window Gestures background helper for this shortcut. Follow the Accessibility and Input Monitoring instructions in Settings if macOS asks for access.
- **Use RichFiles for Folders** sets the macOS folder association. **Restore Previous App** restores the previous association. macOS still owns the Desktop and system Open and Save dialogs. **Reveal in Finder** still opens Finder.
- For a shortcut that opens the standalone app from other apps, install RichFiles in an Applications folder and open it once. Assign **Open RichFiles** in **System Settings → Keyboard → Keyboard Shortcuts → Services**.
- **Undock RichFiles** opens the current tabs and panes in the standalone app. **Dock in RichOS** returns the workspace to the RichOS rail. Transfers stop before the workspace switches.

Core browsing and file operations work locally. The optional RichFinder and RichCleaner menu actions use the corresponding RichOS services when installed.

RichGlance ships inside RichFiles as an optional viewer for PNG, JPEG (`.jpg`, `.jpeg` and `.jpe`), HEIC, HEIF (`.heif` and `.hif`), WebP, AVIF, GIF, TIFF, BMP (`.bmp` and `.dib`), ICO and ICNS. Use **Open With** to try RichGlance on one file. To make it the default across Finder, the Desktop and RichFiles, use **Default image viewer** in RichFiles Settings. Each row reflects macOS's choice for that format. Turning the switch off restores the previous apps when they are available and preserves choices you made separately. RichGlance can open files while both RichFiles and RichOS are closed.

In RichGlance, press Left or Right to cycle through images in that folder, Control and scroll to zoom, drag to pan and Escape to close. Animated GIFs and WebPs play automatically. TIFF opens on its first page. **File → Open in Preview** opens the current image for editing.

Hover over the **Info** button to peek at the current image’s RichCleaner note, keywords, folder and saved date. Click to keep it visible or press **I**. **Show by default** and **Shortcut** are available beside the button and in the **View** menu. The overlay reads the local RichCleaner catalog when the image matches its last saved record.

The existing RichFiles timing log also records RichGlance decode, canvas update, folder scan, Preview handoff, default-viewer and metadata lookup outcomes without storing filenames, paths or note text. It is included in RichDoctor's Configuration diagnostics.

On volumes such as exFAT, regular file copies reserve the destination name exclusively after staging the file. A failed publication keeps the source and reports the output for review. Rename, move into and staged folder copy require the destination volume to support exclusive rename.

## Verify

Run the native suite and packaged archive checks from the repository root.

```sh
bash richos/app/scripts/run-tests.sh --no-host-screen --only richfiles.test.sh
```

RichFiles is part of the [GNU AGPL v3 licensed source](../../../LICENSE).


## Precision capture

Choose **File → Capture Region…** in RichGlance or enable its global shortcut in RichOS Settings. On first capture, macOS may ask for screen recording permission under the name of the app that bundles RichGlance. Each session freezes the display under the pointer.

- Aim with the pointer, then drag the selection or use Shift–Return to place each corner.
- Arrow keys move the target by one image pixel. Release the mouse to lock the selection and use Tab to switch the active corner.
- M toggles the magnifier. Return copies a PNG. Use **Save PNG…** to write the selected pixels to a file.
- Escape backs up one step: close the Save dialog, clear the current selection or exit capture if only the crosshairs are showing. **Start Over** clears the selection and **Cancel Capture** exits.
