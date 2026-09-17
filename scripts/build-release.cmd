@echo off
setlocal
cd /d "%~dp0.."

set "PATH=C:\Program Files\nodejs;%USERPROFILE%\.cargo\bin;%USERPROFILE%\.rustup\toolchains\stable-x86_64-pc-windows-msvc\bin;%~dp0..\.tools\cmake\bin;%PATH%"
"%SystemRoot%\System32\WindowsPowerShell\v1.0\powershell.exe" -ExecutionPolicy Bypass -File "%~dp0tauri-build-local.ps1"
