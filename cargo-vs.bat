@echo off
set "PATH=%USERPROFILE%\.cargo\bin;%PATH%"
call "D:\Program Files\Microsoft Visual Studio\18\Community\VC\Auxiliary\Build\vcvars64.bat"
if errorlevel 1 exit /b 1
where link.exe
if errorlevel 1 exit /b 1
cd /d "%~dp0"
cargo %*
exit /b %errorlevel%
