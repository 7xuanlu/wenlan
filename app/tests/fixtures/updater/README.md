# Disposable updater fixture

The signed archive contains one non-executable text file:
`Fixture.app/Contents/MacOS/fixture` with `Wenlan updater fixture v2`.
It is signed with a throwaway test key. Only the public key and signature
are retained; these are unrelated to Wenlan's release signing key.

`updater_native_tests.rs` serves it on an ephemeral loopback port and points
the real macOS updater at a temporary fake app. The tests do not launch or
replace Wenlan, access its daemon/data, use production endpoints, or restart.
The same test also rejects a tampered archive and preserves the old fixture.
