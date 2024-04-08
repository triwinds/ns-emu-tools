@echo off
chcp>nul 2>nul 65001
:path
cd>nul 2>nul /D %~dp0
rem call venv\Scripts\activate.bat
poetry run pyinstaller --noconfirm --onefile --windowed --upx-dir "./build_tools/upx/" --icon "./web/favicon.ico" --add-data "./module/*.exe;./module/" --add-data "./web;web/"  "./main_devnull.py" --additional-hooks-dir=".\\hooks" --name "NsEmuTools"
poetry run pyinstaller --noconfirm --onefile --console --upx-dir "./build_tools/upx/" --icon "./web/favicon.ico" --add-data "./module/*.exe;./module/" --add-data "./web;web/"  "./main.py" --additional-hooks-dir=".\\hooks" --name "NsEmuTools-console"
rem pyinstaller --noconfirm --onefile --console --icon "./web/favicon.ico" --add-data "./module/*.exe;./module/" --add-data "./web;web/"  "./ui_webview.py" --additional-hooks-dir=".\\hooks" --name "NsEmuTools-webview"
pause
