@echo off
setlocal
pnpm run tauri %*
if errorlevel 1 exit /b %errorlevel%
node "%GITHUB_WORKSPACE%\.github\scripts\verify-officecli-windows-msi.mjs" "src-tauri\target\x86_64-pc-windows-msvc\release\bundle\msi"
exit /b %errorlevel%
