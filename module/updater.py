from module.downloader import download, download_path
from module.msg_notifier import send_notify
import sys
from pathlib import Path
import subprocess
import logging
from config import current_version


logger = logging.getLogger(__name__)
script_template = """@echo off
chcp>nul 2>nul 65001
echo 开始准备更新

echo 尝试优雅关闭程序（允许自动清理）...
taskkill /IM NsEmuTools* >nul 2>nul
echo 等待程序正常退出（3秒）...
timeout /t 3 /nobreak

echo 检查是否还有残留进程...
tasklist /FI "IMAGENAME eq NsEmuTools*" 2>nul | find /I "NsEmuTools" >nul
if %ERRORLEVEL% equ 0 (
  echo 程序未能正常退出，强制终止...
  taskkill /F /IM NsEmuTools* >nul 2>nul
  echo 等待进程完全退出...
  timeout /t 3 /nobreak
  
  echo 清理强制终止残留的临时文件...
  if not "<current_meipass>"=="" (
    if exist "<current_meipass>" (
      echo 删除旧版本临时目录: <current_meipass>
      rmdir /s /q "<current_meipass>" 2>nul
    )
  )
) else (
  echo 程序已正常退出
)

if exist "<old_exe>" (
  echo 备份原文件至 "<old_exe>.bak"
  move /Y "<old_exe>" "<old_exe>.bak"
)
if exist "_internal" (
  echo 备份 _internal 目录
  if exist "_internal_bak" (
    rmdir /s /q "_internal_bak"
  )
  move /Y "_internal" "_internal_bak"
  timeout /t 1 /nobreak
)
if exist "_internal_bak" (
  echo 清理旧的 _internal_bak 备份
  rmdir /s /q "_internal_bak" 2>nul
)
if not exist "<upgrade_files_folder>" (
  echo 无法找到更新文件 "<upgrade_files_folder>"
  pause
) else (
  echo 复制文件中
  robocopy "<upgrade_files_folder>" . /MOVE /E /NFL /NDL /NC
  if exist "download/upgrade_files" (
    timeout /t 1 /nobreak
    rmdir /s /q "download/upgrade_files"
  )
  echo 清理完成，准备启动新版本...
  timeout /t 2 /nobreak
  echo 启动程序
  start /b "NsEmuTools" "<target_place>"
)
DEL "%~f0"
"""


def _parse_version(version_str):
    qualifier = 'zzzzzzzzzzzz'
    sp = version_str.split('-')
    if len(sp) == 2:
        version, qualifier = sp
    else:
        version = version_str
    version = version.strip()
    major, minor, incr = version.split('.')
    return int(major), int(minor), int(incr), qualifier


def check_update(prerelease=False):
    from repository.my_info import get_all_release
    cur_ver_group = _parse_version(current_version)
    release_infos = get_all_release()
    remote_version = None
    if prerelease:
        remote_version = release_infos[0]['tag_name']
    else:
        for ri in release_infos:
            if not ri['prerelease']:
                remote_version = ri['tag_name']
                break
    if not remote_version:
        remote_version = release_infos[0]['tag_name']
    remote_ver_group = _parse_version(remote_version)
    return cur_ver_group < remote_ver_group, remote_version


def download_net_by_tag(tag: str):
    from repository.my_info import get_release_info_by_tag
    from module.network import get_github_download_url
    release_info = get_release_info_by_tag(tag)
    logger.info(f'start download NET release by tag: {tag}, release name: {release_info.get("name")}')
    execute_path = Path(sys.argv[0])
    logger.info(f'execute_path: {execute_path}')
    asset_map = {asset['name']: asset for asset in release_info['assets']}
    target_asset = asset_map.get('NsEmuTools-dir.7z')
    if not target_asset:
        target_asset = asset_map.get(execute_path.name, asset_map.get('NsEmuTools.exe'))
    target_file_name = target_asset["name"]
    logger.info(f'target_file_name: {target_file_name}')
    logger.info(f'start download {target_file_name}, version: [{tag}]')
    send_notify(f'开始下载 {target_file_name}, 版本: [{tag}]')
    upgrade_files_path = download_path.joinpath('upgrade_files')
    info = download(get_github_download_url(target_asset['browser_download_url']),
                    save_dir=str(upgrade_files_path.absolute()),
                    options={'allow-overwrite': 'true'})
    filepath = info.files[0].path.absolute()
    logger.info(f'{target_file_name} of [{tag}] downloaded to {filepath}')
    send_notify(f'{target_file_name} 版本: [{tag}] 已下载至')
    send_notify(f'{filepath}')
    return filepath


def update_self_by_tag(tag: str):
    # upgrade_files_path = download_path.joinpath('upgrade_files')
    # upgrade_file_path = upgrade_files_path.joinpath('NsEmuTools.7z')
    upgrade_file_path = download_net_by_tag(tag)
    upgrade_files_folder = upgrade_file_path.parent
    if not upgrade_file_path:
        logger.error(f'something wrong in downloading.')
        send_notify(f'下载时出现问题, 更新已取消.')
        return
    if upgrade_file_path.name.endswith('.7z'):
        from utils.package import uncompress
        uncompress(upgrade_file_path, upgrade_file_path.parent)
        upgrade_file_path.unlink()
        upgrade_files_folder = upgrade_file_path.parent.joinpath('NsEmuTools')
    target_path = Path('NsEmuTools.exe') if Path('NsEmuTools.exe').exists() else Path('NsEmuTools-console.exe')
    
    # 获取当前程序的临时目录（如果是 PyInstaller 打包的）
    current_meipass = getattr(sys, '_MEIPASS', '')
    
    script = script_template\
        .replace('<old_exe>', str(Path(sys.argv[0]).absolute()))\
        .replace('<upgrade_files_folder>', str(upgrade_files_folder))\
        .replace('<target_place>', str(target_path.absolute()))\
        .replace('<current_meipass>', current_meipass)
    logger.info(f'creating update script')
    with open('update.bat', 'w', encoding='utf-8') as f:
        f.write(script)
    script_path = Path(sys.argv[0]).parent.joinpath('update.bat').absolute()
    logger.info(f'execute script')
    subprocess.Popen(f'start cmd /c "{script_path}"', shell=True)
    try:
        from ui_webview import close_all_windows
        close_all_windows()
    except:
        pass
    send_notify(f'由于浏览器的安全限制，程序无法主动关闭当前窗口。因此请手动关闭当前窗口。')
    send_notify(f'webview 版本可以避免这个问题，如果你的系统版本比较新，可以尝试使用一下 webview 版本。')
    logger.info(f'exit')
    sys.exit()


if __name__ == '__main__':
    # print(check_update())
    print(_parse_version('0.0.1') < _parse_version('0.0.2'))
    print(_parse_version('0.0.1-beta1') < _parse_version('0.0.1'))
    print(_parse_version('0.0.1-beta1') < _parse_version('0.0.1-beta2'))
    print(_parse_version('0.0.1-alpha1') < _parse_version('0.0.1-beta2'))
