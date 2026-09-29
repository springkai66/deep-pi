# DeepPi

DeepPi is a desktop workspace for development with [Pi Coding Agent](https://github.com/badlogic/pi-mono) and DeepSeek Harness (DSH). It organizes agent sessions around projects and tasks, with integrated file, Git, model, and runtime management.

## Features

- **Parallel tasks:** Run multiple Pi sessions and switch between structured chat and the native terminal; use DSH in a separate workspace.
- **Project workflow:** Browse, search, and edit files; inspect diffs; stage, commit, and push Git changes.
- **Models and extensions:** Configure model providers and credentials; manage Pi extensions, Skills, and MCP services.
- **Managed runtimes:** Install and manage Node.js, Pi, and DSH on demand without configuring a global environment.
- **Supporting tools:** Task board, priority checklist, themes, and multilingual UI.

## Install and get started

The current release targets **Windows 10/11 x64**. Download `DeepPi_0.1.0_x64-setup.exe` or the MSI installer from [Releases](https://github.com/springkai66/deep-pi/releases). The installer downloads WebView2 when needed. The installer is not yet Windows code-signed; download it from this repository's Release page and verify the source.

On first launch, configure a model provider or sign in with an official account, add a project directory, and create a Pi task. Managed runtimes are installed when needed and remain separate from existing local Node.js / Pi / DSH installations.

## Roadmap

- **macOS support:** Adapt desktop capabilities and distribution.
- **WeChat interaction:** Explore task notifications and interactive requests through WeChat.
- **Remote access:** Explore secure access to workspace capabilities from other devices.

These are planned directions, **not features of the current release**.

## Development

DeepPi uses Tauri v2, Rust, and SvelteKit. After installing dependencies, start the desktop development environment with `pnpm dev:desktop`. See [DEVELOPMENT.md](./DEVELOPMENT.md) and [RELEASING.md](./RELEASING.md) for build and release instructions.

中文：[README.md](./README.md) · License: [MIT](./LICENSE)
