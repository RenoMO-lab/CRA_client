# Caddy HTTPS Setup (`192.168.50.55`)

This setup provides HTTPS termination for CRA Client release builds while the Node app continues to run on port `3000`.

## Server steps

1. Install Caddy on server `192.168.50.55`.
2. Copy `docs/caddy/Caddyfile` to:
   - `C:\ProgramData\Caddy\Caddyfile`
3. Restart Caddy service:

```powershell
Restart-Service caddy
```

4. Confirm proxy health from the server:

```powershell
Invoke-WebRequest -Uri https://192.168.50.55 -UseBasicParsing
```

## Trust the internal CA on client machines

When `tls internal` is used, Caddy issues certs from its local CA. Import Caddy's root certificate into `Trusted Root Certification Authorities` on every CRA Client machine.

## CRA Client release config

Use:

```env
APP_URL=https://192.168.50.55
ALLOWED_HOSTS=192.168.50.55
```
