@echo off
set PATH=%USERPROFILE%\.cargo\bin;%PATH%
call "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvars64.bat" >nul
cd /d %~dp0src-tauri
cargo test --lib webhook::tests
