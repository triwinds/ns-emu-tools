@echo off
chcp>nul 2>nul 65001
:path
cd>nul 2>nul /D %~dp0
call venv\Scripts\activate.bat
pyinstaller --noconfirm --onefile --windowed --icon "./web/favicon.ico" --add-data "./module/aria2c.exe;./module/" --add-data "./web;web/"  "./main.py"
rem pyinstaller --noconfirm --onefile --console --icon "./web/favicon.ico" --add-data "./module/aria2c.exe;./module/" --add-data "./web;web/"  "./main.py"
pause
