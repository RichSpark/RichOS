The app's WebKit source is the authoring source for gallery previews, saved images
and the launch videos. On macOS AVFoundation decodes the first frames of the
local H.264 video during window construction. The display layer plays the
decoded frames at their authored timestamps and fades after the endpoint.
The WebView reports its first paint independently of playback.
Home's bloom starts only after the native fade callback releases the page's cover.

Render a catalog from this checkout with Xcode's Swift compiler, Node and FFmpeg:

```sh
bash richos/app/native/splash/render-packs.sh /absolute/output app-version
```

Pass opening IDs after the version to export a subset. Each export renders the
1400 by 852 point layout at 2× resolution and verifies its complete 60 fps
timeline. The recorded gold bar bounds keep the Home transition aligned when
the video scales to fill a resized window.

The two standard packs and both sound cues are bundled. Packaging renders the
update edition for the bundle's version. The other packs live as GitHub release
assets at the tag declared in `src-tauri/src/splash_assets.rs`. The app checks the
committed byte count and SHA-256 before activating a downloaded selection. After
changing the artwork or the pack format, update the release tag, asset manifest
and the two bundled packs together. Active packs are available offline. The
gallery's Play button always uses its local HTML preview.

The local app log records when the native frame zero is scheduled, when the
authored endpoint is reached and when the native fade finishes. The launch
speed walk uses the scheduled first frame for its LaunchServices timing.
