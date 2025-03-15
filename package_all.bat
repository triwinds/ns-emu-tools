@echo off
chcp>nul 2>nul 65001
:path
cd>nul 2>nul /D %~dp0
rem call venv\Scripts\activate.bat
uv run pyinstaller --noconfirm --onefile --windowed --icon "./web/favicon.ico" --add-data "./module/*.exe;./module/" --add-data "./web;web/"  "./main_devnull.py" --additional-hooks-dir=".\\hooks" --name "NsEmuTools"
uv run pyinstaller --noconfirm --onefile --console --icon "./web/favicon.ico" --add-data "./module/*.exe;./module/" --add-data "./web;web/"  "./main.py" --additional-hooks-dir=".\\hooks" --name "NsEmuTools-console"
uv run pyinstaller --noconfirm --windowed --icon "./web/favicon.ico" --add-data "./module/*.exe;./module/" --add-data "./web;web/"  "./main_devnull.py" --additional-hooks-dir=".\\hooks" --name "NsEmuTools"
uv run python build_tools/zip_files.py
pause
