# CRA Client (Tauri Desktop Wrapper)

Windows desktop wrapper for the CRA web application. The desktop app does not host business logic or data. It loads the remote app URL and keeps computation/database activity on the server.

## Primary target host

Primary web app target is:
- `http://192.168.50.55:3000` (internal HTTP deployment)

Planned production target after TLS setup:
- `https://192.168.50.55`

## Runtime configuration

The app reads settings from namespaced environment variables first, then from `client.env` files.
On first run, it auto-creates `%APPDATA%\CRA Client\client.env` if missing.

Resolution order:
1. Process environment variables (`CRA_CLIENT_*` only).
2. `client.env` in current working directory.
3. `client.env` next to the executable.
4. `%APPDATA%\CRA Client\client.env`.

Supported process environment variables:
- `CRA_CLIENT_APP_URL`
- `CRA_CLIENT_ALLOWED_HOSTS`
- `CRA_CLIENT_WINDOW_TITLE`
- `CRA_CLIENT_WINDOW_WIDTH`
- `CRA_CLIENT_WINDOW_HEIGHT`
- `CRA_CLIENT_ALLOW_LOCALHOST_RELEASE` (optional, default `false`)
- `CRA_CLIENT_MIN_WEB_BUILD_HASH` (optional parity gate)
- `CRA_CLIENT_ENFORCE_WEB_BUILD` (optional, `true|false`)

`APP_URL` and `ALLOWED_HOSTS` remain supported in `client.env` for backward compatibility.
Generic process env keys like `APP_URL` / `ALLOWED_HOSTS` are intentionally ignored to prevent host pollution from unrelated machine variables.

Required keys:
- `APP_URL`: Target URL of the existing web app.
- `ALLOWED_HOSTS`: Comma-separated host allowlist used by navigation guard. Must include the `APP_URL` host.

Optional keys:
- `WINDOW_TITLE` (default `CRA Client`)
- `WINDOW_WIDTH` (default `1280`)
- `WINDOW_HEIGHT` (default `800`)
- `MIN_WEB_BUILD_HASH` (optional required minimum web build hash/prefix)
- `ENFORCE_WEB_BUILD` (optional, defaults: release `true`, debug `false`)

Development `client.env` (current deployment):

```env
APP_URL=http://192.168.50.55:3000
ALLOWED_HOSTS=192.168.50.55
WINDOW_TITLE=CRA Client
WINDOW_WIDTH=1280
WINDOW_HEIGHT=800
```

Release `client.env`:

```env
APP_URL=http://192.168.50.55:3000
ALLOWED_HOSTS=192.168.50.55
WINDOW_TITLE=CRA Client
WINDOW_WIDTH=1280
WINDOW_HEIGHT=800
```

Release builds in this internal profile accept HTTP or HTTPS `APP_URL`.
Release builds reject localhost-style `APP_URL` hosts (`localhost`, `127.0.0.1`, `::1`, `tauri.localhost`) unless `CRA_CLIENT_ALLOW_LOCALHOST_RELEASE=true` is explicitly set.

### Web build parity gate

To guarantee CRA Client opens only an up-to-date web deployment, set:

```env
MIN_WEB_BUILD_HASH=aacb669
ENFORCE_WEB_BUILD=true
```

Behavior:
- CRA Client requests `${APP_URL}/api/admin/deploy-info`.
- It compares `build.hash` (fallback `git.hash`) with `MIN_WEB_BUILD_HASH` (prefix match, case-insensitive).
- If mismatch and `ENFORCE_WEB_BUILD=true`, app launch is blocked with retry/about diagnostics.
- If mismatch and `ENFORCE_WEB_BUILD=false`, warning is shown and launch continues.

About dialog and startup log include:
- connected web build hash/time
- required minimum hash
- parity enforcement mode

### First run behavior

If `%APPDATA%\CRA Client\client.env` does not exist, the app creates it with:

```env
APP_URL=http://192.168.50.55:3000
ALLOWED_HOSTS=192.168.50.55
WINDOW_TITLE=CRA Client
WINDOW_WIDTH=1280
WINDOW_HEIGHT=800
```

Upgrade note from `v0.1.6`:
- If app data still has the previous auto-generated `APP_URL=https://192.168.50.55`, `v0.1.7` migrates it automatically to `http://192.168.50.55:3000`.

## Development

Prerequisites:
- Node.js 20+
- Rust toolchain (stable) with `cargo`/`rustc` on PATH
- Windows build tools for Tauri

Install and run:

```powershell
npm.cmd install
npm.cmd run tauri:dev
```

`npm.cmd` is recommended in this environment because PowerShell may block `npm.ps1` by execution policy.

## Build

```powershell
npm.cmd install
npm.cmd run tauri:build
```

NSIS output is generated under:
- `src-tauri/target/release/bundle/nsis/`

## HTTPS endpoint for release (Caddy)

Use Caddy on the server host to terminate TLS and reverse proxy to the Node app on port `3000`.

1. Install Caddy service on the server (`192.168.50.55`).
2. Place config at `C:\ProgramData\Caddy\Caddyfile`.
3. Use the sample file in this repo: `docs/caddy/Caddyfile`.
4. Trust Caddy's root CA on all client machines.
5. Switch `APP_URL` to `https://192.168.50.55` for release packaging.

## Deterministic pilot artifact name

Repackage built installer into required name format:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\package-windows.ps1 -Version 0.1.14
```

Result:
- `artifacts/CRA-Client-0.1.14-windows-x64.exe`
- `artifacts/CRA-Client-0.1.14-windows-x64.exe.sha256`

## Security and behavior

- App starts on a local bootstrap screen.
- It validates config and checks server reachability.
- If reachable, it navigates to `APP_URL`.
- If unreachable, it shows retry UI without restart.
- Navigation is restricted to `ALLOWED_HOSTS` inside the app.
- Non-allowlisted links are blocked and stay inside the desktop app.
- This internal build supports HTTP and HTTPS targets.
- Startup diagnostics are appended to `%APPDATA%\CRA Client\logs\startup.log`.

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

## Troubleshooting

- `failed to get cargo metadata: program not found`
  - Install Rust and ensure `%USERPROFILE%\.cargo\bin` is on PATH.
- `npm.ps1 cannot be loaded because running scripts is disabled`
  - Use `npm.cmd` instead of `npm` in PowerShell.
- `Failed to load PostCSS config ... Unexpected token ... "name"... is not valid JSON`
  - Ensure `package.json` is UTF-8 without BOM. The release workflow includes a normalization step.
- App opens Edge/Chrome to `https://tauri.localhost` and app window does not continue
  - Upgrade to `v0.1.6` or later. This version keeps Tauri internal bootstrap URLs in-app and blocks browser pop-out.
- Server unreachable at `https://192.168.50.55/` while your server is HTTP-only
  - Use `v0.1.7` or later, which supports internal HTTP target `http://192.168.50.55:3000`.
- `Could not reach server at http://192.168.50.55:3000`
  - Verify network path/firewall and that the server process is listening on port `3000`.
