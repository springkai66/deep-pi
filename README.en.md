# DeepPi

A Windows desktop host that brings Pi Coding Agent and DeepSeek Harness (DSH) into one application: parallel Pi terminal and RPC sessions, the native DSH Web UI, a Pi package marketplace, provider and credential management, project files with Git review, and in-app runtime upgrades for Pi and DSH.

## Download and install

Download the latest `DeepPi_<version>_x64-setup.exe` (NSIS) from [GitHub Releases](https://github.com/springkai66/deep-pi/releases).

- Requires Windows 10/11 x64; WebView2 is installed by the setup bootstrapper.
- The v1.0.0 installer is not Authenticode-signed yet. Windows SmartScreen may warn on first launch: choose "More info" → "Run anyway", after verifying the download source.
- Uninstall from "Settings → Apps" or the uninstaller in the install directory.

## Requirements (development)

- Windows 10/11
- Node.js 22+
- pnpm 10.30.3
- Stable Rust MSVC toolchain
- Microsoft C++ Build Tools
- Microsoft Edge WebView2

## Development

```powershell
pnpm install --frozen-lockfile --ignore-scripts
pnpm tauri dev
```

## Checks

```powershell
pnpm check
pnpm test
pnpm build
pnpm security:audit
pnpm licenses:check
```

## Build

Build an NSIS installer:

```powershell
pnpm tauri build --bundles nsis
```

Build NSIS and MSI installers:

```powershell
pnpm tauri build --bundles nsis,msi
```

Artifacts are written to `src-tauri/target/release/bundle/`.

Build the application executable without an installer:

```powershell
pnpm tauri build --no-bundle
```

This is not a fully self-contained portable build; WebView2 and the Pi/DSH runtimes are still required.

## Release

Push a `v*` tag to run `.github/workflows/release.yml`, which builds, signs (optional) and publishes the Windows artifacts and syncs the `stable` updater channel. Tagged releases require the Tauri updater signing secrets; Authenticode certificate secrets are optional and enable signing automatically. See [RELEASING.md](./RELEASING.md).

## Known limitations (v1.0)

- The installer is not Authenticode-signed yet; an open-source signing application is in progress.
- Local Pi/DSH/Node/npm installations are not read; DeepPi only uses its own managed runtimes and configuration directory (Node is installed by the app from the official distribution and verified against SHA-256).
- Host shortcuts do not work while the DSH child Webview has focus (see [KEYBOARD_SHORTCUTS.md](./KEYBOARD_SHORTCUTS.md)).
- The DeepPi self-update pipeline is configured (updater signature and `stable` channel), but v1.0.0 is the first release; real cross-version self-update and rollback will be validated when the next version ships.
- The 8-hour soak, very large repositories and parts of the native interaction acceptance are still open (see [NATIVE_ACCEPTANCE.md](./NATIVE_ACCEPTANCE.md)).

## Documentation

- [Keyboard shortcuts](./KEYBOARD_SHORTCUTS.md)
- [Pi RPC compatibility baseline](./RPC_COMPATIBILITY.md)
- [Native acceptance checklist](./NATIVE_ACCEPTANCE.md)
- [Release process](./RELEASING.md)
- [中文 README](./README.md)
- [Security](./SECURITY.md)
- [Troubleshooting](./TROUBLESHOOTING.md)
- [UI redesign plan and records](./UI_REDESIGN_PLAN.md)

## License

MIT
