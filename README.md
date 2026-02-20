# CRA Client (Tauri Desktop Wrapper)

Windows desktop wrapper for the CRA web application. The desktop app does not host business logic or data. It loads the existing remote app URL and keeps computation/database activity on the server.

## What this repo changes

- Adds a Windows `.exe` desktop channel for internal users.
- Keeps the browser-based app available in parallel.
- Does not require changes in `CRA_Local_W2016_Server` for the wrapper itself.

## Runtime configuration

The app reads settings from environment variables first, then from `client.env` files.

Resolution order:
1. Process environment variables.
2. `client.env` in current working directory.
3. `client.env` next to the executable.
4. `%APPDATA%\CRA Client\client.env`.

Required keys:
- `APP_URL`: Full HTTPS URL of the existing web app.
- `ALLOWED_HOSTS`: Comma-separated host allowlist used by navigation guard. Must include `APP_URL` host.

Optional keys:
- `WINDOW_TITLE` (default `CRA Client`)
- `WINDOW_WIDTH` (default `1280`)
- `WINDOW_HEIGHT` (default `800`)

Example `client.env`:

```env
APP_URL=https://your-production-app.example.com
ALLOWED_HOSTS=your-production-app.example.com
WINDOW_TITLE=CRA Client
WINDOW_WIDTH=1280
WINDOW_HEIGHT=800
```

## Development

Prerequisites:
- Node.js 20+
- Rust toolchain (stable)
- Windows build tools for Tauri

Install and run:

```powershell
npm install
npm run tauri:dev
```

## Build

```powershell
npm install
npm run tauri:build
```

NSIS output is generated under:
- `src-tauri/target/release/bundle/nsis/`

## Deterministic pilot artifact name

Repackage built installer into required name format:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\package-windows.ps1 -Version 0.1.0
```

Result:
- `artifacts/CRA-Client-0.1.0-windows-x64.exe`
- `artifacts/CRA-Client-0.1.0-windows-x64.exe.sha256`

## Security and behavior

- App starts on a local bootstrap screen.
- It validates config and checks server reachability.
- If reachable, it navigates to `APP_URL`.
- If unreachable, it shows retry UI without restart.
- Navigation is restricted to `ALLOWED_HOSTS` inside the app.
- Non-allowlisted links are opened in the system browser.
- In release builds, `APP_URL` must use HTTPS.

## About

- Press `Alt+Shift+A` in the app to show About information (version + target host).
- Bootstrap screen also includes an About button.

## CI/CD

Tag-based GitHub Actions workflow:
- File: `.github/workflows/release-windows.yml`
- Trigger: push tag `v*`
- Produces release assets:
  - `CRA-Client-<version>-windows-x64.exe`
  - `CRA-Client-<version>-windows-x64.exe.sha256`

## Test checklist

- Missing `APP_URL` or `ALLOWED_HOSTS` shows config error screen.
- Unreachable server shows retry flow.
- Reachable server opens remote app without restart.
- Non-allowlisted navigation opens in default browser.
- Desktop and web browser access work concurrently against same backend.
