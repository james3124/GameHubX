# GameHubX release build

CI is the only APK build path. Pushes and pull requests produce the debug APK;
tags matching `v*` also produce a signed release APK.

## Signing secrets

Configure these GitHub Actions repository secrets:

| Secret | Value |
|---|---|
| `GAMEHUBX_KEYSTORE_BASE64` | Base64 of the PKCS#12 release keystore |
| `GAMEHUBX_KEYSTORE_PASSWORD` | Keystore password |
| `GAMEHUBX_KEY_ALIAS` | Key alias (defaults to `gamehubx` if omitted) |
| `GAMEHUBX_KEY_PASSWORD` | Key password (defaults to the keystore password) |

Create a keystore locally and keep an offline backup:

```bash
keytool -genkeypair -v \
  -keystore gamehubx-release.p12 \
  -storetype PKCS12 \
  -alias gamehubx \
  -keyalg RSA -keysize 4096 -validity 10000 \
  -dname "CN=GameHubX,O=GameHubX,C=US"
base64 -w0 gamehubx-release.p12 > gamehubx-release.p12.base64
```

Never commit the keystore or the base64 file.

## Cutting a release

1. Ensure the debug APK boots on the target phone and the release checklist in
   `docs/STATUS.md` is satisfied.
2. Push a tag, for example `v0.1.0`. Do not tag until the tree is ready.
3. CI builds `assembleDebug` and `assembleRelease`, then uploads
   `gamehubx-debug-apk` and `gamehubx-release-apk`.
4. Verify the downloaded release APK:

```bash
apksigner verify --print-certs gamehubx-release.apk
```

The release version name comes from the tag with a leading `v` removed; the
version code comes from the workflow run number.

## Signing behavior

- No keystore env: `assembleRelease` produces an unsigned APK (useful only for
  compile checks); tag builds fail early because the keystore secret is
  required.
- Keystore env present: the `release` signing config uses it.
- Debug builds never use the release keystore.

## Distribution

GameHubX is GPL-3.0-only. Publishing an APK requires publishing the
corresponding source and preserving `LICENSE` plus the vendored Winlator
`LICENSE`/`THIRD_PARTY_NOTICES.txt`. Review third-party runtime notices before
release (Wine, Box64, DXVK, PRoot, PulseAudio, Mesa/Turnip, FEXCore, Winlator
assets).
