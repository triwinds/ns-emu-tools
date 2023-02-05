from pathlib import Path


switch_ui_path = Path('切换 UI 启动模式.bat')
switch_ui_template = """@echo off
chcp>nul 2>nul 65001
cd>nul 2>nul /D %~dp0
set net_path=""
if exist "NsEmuTools.exe" (
    set net_path="NsEmuTools.exe"
)
if exist "NsEmuTools-console.exe" (
    set net_path="NsEmuTools-console.exe"
)
if %net_path% == "" (
    echo 无法在当前目录找到 NsEmuTools 可执行文件, 请将 bat 脚本与 exe 放置在同一目录下。
    pause
)
echo %net_path%
echo "切换 UI 启动模式"
echo "0: 自动选择"
echo "1: 通过 webview 启动"
echo "2: 通过浏览器启动(自动查找浏览器)"
echo "3: 通过 chrome 浏览器启动"
echo "4: 通过 edge 浏览器启动"
echo "5: 通过默认浏览器启动"
set uc="0"
set mode="auto"
:GET_INPUT
set /p uc=选择启动模式(输入数字):
2>NUL CALL :CASE_%uc%
IF ERRORLEVEL 1 CALL :CASE_ERROR
:CASE_0
  set mode="auto"
  GOTO END_CASE
:CASE_1
  set mode="webview"
  GOTO END_CASE
:CASE_2
  set mode="browser"
  GOTO END_CASE
:CASE_3
  set mode="chrome"
  GOTO END_CASE
:CASE_4
  set mode="edge"
  GOTO END_CASE
:CASE_5
  set mode="user default"
  GOTO END_CASE
:CASE_ERROR
  echo "无法识别的模式，请重新输入"
  GOTO GET_INPUT
:END_CASE
  call %net_path% --switch-mode %mode%
  IF %ERRORLEVEL% == 0 ( 
    echo "变更已保存，将于下次启动时生效。" 
  ) else (
    echo "发生未知错误。" 
  )
  pause
  exit
"""


def create_scripts():
    with open(switch_ui_path, 'w', encoding='utf-8') as f:
        f.write(switch_ui_template)
