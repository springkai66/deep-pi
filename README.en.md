# DeepPi

A Windows desktop host for Pi Coding Agent and DeepSeek Harness.

## Requirements

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

Push a `v*` tag to run `.github/workflows/release.yml`. Production releases require Tauri updater signing secrets and Authenticode certificate secrets.

## Documentation

- [中文 README](./README.md)
- [Security](./SECURITY.md)
- [Troubleshooting](./TROUBLESHOOTING.md)

## License

MIT
