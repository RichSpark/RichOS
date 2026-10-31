// RETIRED (CEO, 2026-10-02: only the build users get ever goes on his physical phones, and every test
// on them tests that build). `mobile.mjs device build|verify` built the preserved shell app
// (dev.richos.mobile.loop) as a Debug build and installed it on the physical iPhone through
// xcodebuild. That app is not the build users get, so the command refuses before anything is built
// or installed. A physical iPhone is reached only through `richos/mobile/native-ios/bin/rios device`,
// which puts RichConnect's Release build on it.
export async function device() {
  throw Error('Retired: `mobile.mjs device` put a Debug build of the preserved shell app on a physical iPhone. '
    + 'Only the build users get (RichConnect, Release) goes on a physical phone, and only through '
    + '`richos/mobile/native-ios/bin/rios device ...` (CEO 2026-10-02). The simulator commands are unchanged.');
}
