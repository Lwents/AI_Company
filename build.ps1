param(
    [switch]$Run,
    [switch]$Debug
)
$ErrorActionPreference = "Stop"
Set-Location $PSScriptRoot

# Rust GNU toolchain needs MinGW (MSVC link.exe is not installed on this machine).
$cargoBin = Join-Path $env:USERPROFILE ".cargo\bin"
$selfContained = Join-Path $env:USERPROFILE ".rustup\toolchains\stable-x86_64-pc-windows-gnu\lib\rustlib\x86_64-pc-windows-gnu\bin\self-contained"
$mingw = @("C:\Program Files\WinLibs\mingw64\bin", "C:\Program Files (x86)\WinLibs\mingw64\bin") |
    Where-Object { Test-Path (Join-Path $_ "as.exe") } | Select-Object -First 1
if (-not $mingw) {
    $found = Get-ChildItem "$env:LOCALAPPDATA\Microsoft\WinGet\Packages" -Recurse -Filter "as.exe" -ErrorAction SilentlyContinue |
        Where-Object { $_.FullName -like "*mingw64\bin\as.exe" } | Select-Object -First 1
    if ($found) { $mingw = $found.DirectoryName }
}
if (-not $mingw) {
    Write-Host "Không tìm thấy MinGW-w64. Cài bằng: winget install BrechtSanders.WinLibs.POSIX.UCRT" -ForegroundColor Red
    exit 1
}
$env:PATH = "$cargoBin;$mingw;$selfContained;$env:PATH"

if (-not (Test-Path "node_modules")) {
    Write-Host "Cài thư viện giao diện..." -ForegroundColor Cyan
    npm install
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
}

Write-Host "Build giao diện..." -ForegroundColor Cyan
npm run build
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

Write-Host "Build backend Rust..." -ForegroundColor Cyan
Push-Location backend
if ($Debug) { cargo build } else { cargo build --release }
$code = $LASTEXITCODE
Pop-Location
if ($code -ne 0) { exit $code }

$profile = if ($Debug) { "debug" } else { "release" }
$exe = Join-Path $PSScriptRoot "backend\target\$profile\ai-company.exe"
Write-Host "`nXong: $exe" -ForegroundColor Green

if ($Run) {
    Start-Process "http://127.0.0.1:8787"
    & $exe
}
