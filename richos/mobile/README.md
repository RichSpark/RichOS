# RichOS on your phone

**Purely optional.** RichOS runs completely on one Mac in your office. Nothing here is needed for that,
in the same way that syncing a second Mac through GitHub is optional.

**What a mobile app is, here:** an app that lets you use RichOS while you are on the go and away from
the office, outside your home network. Inside your home network the desktop app is the better tool.

The phone apps live here: the native RichConnect apps for iPhone ([`native-ios/`](native-ios/)) and
Android ([`native-android/`](native-android/)). See [the mobile development loop](DEVELOPMENT.md) for the
shared CLI's headless and iOS simulator targets.

RichOS used to serve a phone web page from your Mac. **It was retired on 2026-10-30** and the
RichConnect app replaces it: open the app on your phone and scan the code your Mac shows.

**Reaching your Mac from outside the office** takes one of two paths:

- **Technical users:** your own Tailscale account on the Mac and on the phone. RichOS guides you
  through it with simple how-to screens in the app. Full setup and troubleshooting, including the one
  thing that trips up almost everybody: [**Tailscale setup and
  troubleshooting**](./TAILSCALE-SETUP-AND-TROUBLESHOOTING.md).

  **The one thing, said here too, because it decides everything after it:** Tailscale has no username
  and password. You sign in with a Google, Apple, Microsoft or GitHub identity and that identity **is**
  your private network — so your Mac and your phone must sign in with the **same** one. Two different
  identities make two separate networks that cannot see each other and neither device says so. If you
  would rather not use a personal identity on both devices, make one that is only for this — a new
  Google account costs nothing — and use it on both. Decide before you sign in anywhere; the
  [setup page](./TAILSCALE-SETUP-AND-TROUBLESHOOTING.md) has the rest.
- **Everyone else:** a hosted relay, coming later.

On the Tailscale path the RichConnect app talks only to your Mac: there is no server of ours in
between, nothing to sign up for with us and nothing to pay us for.
