@echo off
set "ROOT=%~dp0..\.."
set "PY=python"
where py >nul 2>nul && set "PY=py -3"
start "" cmd /c "timeout /t 1 >nul & start http://127.0.0.1:8796/prototypes/loadout-ui/"
%PY% -m http.server 8796 --bind 127.0.0.1 --directory "%ROOT%"
