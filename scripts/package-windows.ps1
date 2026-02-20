param(
  [Parameter(Mandatory = $true)]
  [string]$Version
)

$ErrorActionPreference = "Stop"

$installer = Get-ChildItem -Path "src-tauri/target/release/bundle/nsis" -Filter "*.exe" |
  Sort-Object LastWriteTime -Descending |
  Select-Object -First 1

if (-not $installer) {
  throw "Installer not found under src-tauri/target/release/bundle/nsis"
}

New-Item -ItemType Directory -Path "artifacts" -Force | Out-Null

$artifactName = "CRA-Client-$Version-windows-x64.exe"
$artifactPath = Join-Path "artifacts" $artifactName
Copy-Item -Path $installer.FullName -Destination $artifactPath -Force

$hash = (Get-FileHash -Path $artifactPath -Algorithm SHA256).Hash.ToLower()
Set-Content -Path "$artifactPath.sha256" -Value "$hash  $artifactName" -Encoding ascii

Write-Host "Packaged artifact: $artifactPath"
Write-Host "SHA256 file: $artifactPath.sha256"
