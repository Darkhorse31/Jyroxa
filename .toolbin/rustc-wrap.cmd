@echo off
setlocal
set "RUSTC=%~1"
if "%RUSTC%"=="" exit /b 1
if exist "%~dp0dlltool-wrap.exe" if exist "%~dp0rustc-wrap.exe" (
    "%~dp0rustc-wrap.exe" %*
    exit /b %ERRORLEVEL%
)
shift
"%RUSTC%" %*
exit /b %ERRORLEVEL%
