# DeepPi

A Windows desktop host that brings Pi Coding Agent and DeepSeek Harness (DSH) into one application: run several Pi tasks in parallel (native terminal or structured chat), use the native DSH UI, browse and edit project files, review Git changes and commit/push, install Pi extensions, manage model credentials, and upgrade the runtimes in-app.

No need to install Node, Pi, or DSH beforehand — DeepPi manages its own runtimes and does not interfere with anything already installed on the machine.

## Download and install

Download the latest `DeepPi_<version>_x64-setup.exe` (NSIS) from [GitHub Releases](https://github.com/springkai66/deep-pi/releases) and run it.

- Requires Windows 10/11 x64. WebView2 is installed by the setup bootstrapper.
- The installer is not Authenticode-signed yet. Windows SmartScreen may warn on first launch: choose "More info" → "Run anyway", after verifying the download source.
- Uninstall from "Settings → Apps", or run the uninstaller in the install directory.

## Getting started

1. **Add model credentials** — open "Settings → Models & credentials" and enter your provider API key (keys are kept in Windows Credential Manager, never written to a config file).
2. **Open a project** — click "Add project directory" in the left rail and pick a code directory.
3. **Start a task** — click "New Pi task" to create a terminal or chat task and start working with Pi.

The managed runtimes (Node, Pi, DSH) are installed on demand from official distributions the first time a feature needs them; the confirmation dialog states which version will be downloaded.

## Features

- **Pi tasks** — several tasks in parallel; terminal and chat modes can be switched at any time and share the same session, so context is never lost.
- **DSH workspace** — the native DSH Web UI loaded in the same window, with sessions, settings, and the plugin marketplace.
- **Files** — file tree, file-name and content search, read-only preview, built-in editor, diff comparison, and recovery copies.
- **Git** — change classification, diff view, explicit stage/unstage, commit, push, and remote verification; never force-pushes by default.
- **Pi extensions** — search, install, update, and uninstall Pi packages, confirming the source and version first.
- **Model credentials** — protocol, base URL, headers, proxy, and connection test.
- **Runtimes & updates** — upgrade Node / Pi / DSH / dshmarket in-app, keeping the previous version for rollback.

## Known limitations (v1.0)

- The installer is not Authenticode-signed yet; an open-source signing application is in progress.
- Local Pi/DSH/Node/npm installations are not read; DeepPi only uses its own managed runtimes and configuration directory (Node is installed by the app from the official distribution and verified against SHA-256).
- DSH runs the verified `0.1.5-rc.2`: the host adapts its launch-token auth and the `/api/<ns>/<method>` RPC envelope; upstream updates are still gated behind compatibility verification.
- Host shortcuts do not work while the DSH child view has focus (see [KEYBOARD_SHORTCUTS.md](./KEYBOARD_SHORTCUTS.md)).
- **Content** search in the file sidebar depends on `rg.exe` (ripgrep) being available on the system `PATH`: DeepPi neither bundles nor auto-installs it, and shows a "ripgrep not found" message in that mode when it is missing (file-name search is unaffected; see [TROUBLESHOOTING.md](./TROUBLESHOOTING.md)).
- The DeepPi self-update pipeline is configured, but the pipeline currently ships v1.0.2; real cross-version self-update and rollback are being validated with the 1.0.2 release.
- The 8-hour soak, very large repositories and parts of the native interaction acceptance are still open (see [NATIVE_ACCEPTANCE.md](./NATIVE_ACCEPTANCE.md)).

## Development

Build, test, and release procedures are documented in [DEVELOPMENT.md](./DEVELOPMENT.md) and [RELEASING.md](./RELEASING.md).

## Documentation

- [Keyboard shortcuts](./KEYBOARD_SHORTCUTS.md)
- [Troubleshooting](./TROUBLESHOOTING.md)
- [Pi RPC compatibility baseline](./RPC_COMPATIBILITY.md)
- [Security](./SECURITY.md)
- [Release process](./RELEASING.md)
- [中文 README](./README.md)

## License

MIT
