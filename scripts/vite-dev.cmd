@echo off
setlocal
cd /d "%~dp0.."
set "PATH=C:\Program Files\nodejs;%PATH%"
node scripts\free-dev-port.mjs
node node_modules\vite\bin\vite.js
