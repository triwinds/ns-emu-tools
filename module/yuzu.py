import os
import shutil
import subprocess
import tempfile
import time
from pathlib import Path
import logging

import py7zr

from config import config, dump_config
from module.common import get_firmware_infos, get_keys_info, download_keys_by_name
from module.downloader import download
from module.msg_notifier import send_notify
from repository.yuzu import get_latest_yuzu_release_info, get_yuzu_release_info_by_version
from module.network import get_finial_url


logger = logging.getLogger(__name__)


def download_yuzu(release_info):
    assets = release_info['assets']
    for asset in assets:
        if asset['content_type'] == 'application/x-7z-compressed':
            url = get_finial_url(asset['browser_download_url'])
            logger.info(f"downloading yuzu from {url}")
            info = download(url)
            file = info.files[0]
            return file.path


def install_yuzu(target_version=None):
    if target_version == config.yuzu.yuzu_version:
        logger.info(f'Current yuzu version is same as target version [{target_version}], skip install.')
        return f'当前就是 [{target_version}] 版本的 yuzu , 跳过安装.'
    send_notify('正在获取 yuzu 版本信息...')
    if target_version:
        release_info = get_yuzu_release_info_by_version(target_version)
    else:
        release_info = get_latest_yuzu_release_info()
    version = release_info["tag_name"][3:]
    if version == config.yuzu.yuzu_version:
        logger.info(f'Current yuzu version is same as target version [{version}], skip install.')
        return f'当前就是 [{version}] 版本的 yuzu , 跳过安装.'
    logger.info(f'target yuzu version: {release_info["tag_name"][3:]}')
    yuzu_path = Path(config.yuzu.yuzu_path)
    logger.info(f'target yuzu path: {yuzu_path}')
    send_notify('开始下载 yuzu...')
    yuzu_package_path = download_yuzu(release_info)
    with py7zr.SevenZipFile(yuzu_package_path) as zf:
        zf: py7zr.SevenZipFile = zf
        logger.info(f'Unpacking yuzu files...')
        send_notify('正在解压 yuzu 文件...')
        zf.extractall(tempfile.gettempdir())
        tmp_dir = Path(tempfile.gettempdir()).joinpath('yuzu-windows-msvc-early-access')
        for useless_file in tmp_dir.glob('yuzu-windows-msvc-source-*.tar.xz'):
            os.remove(useless_file)
        logger.info(f'Copy back yuzu files...')
        send_notify('安装 yuzu 文件至目录...')
        shutil.copytree(tmp_dir, yuzu_path, dirs_exist_ok=True)
        shutil.rmtree(tmp_dir)
        config.yuzu.yuzu_version = version
        dump_config()
        logger.info(f'Yuzu of [{version}] install successfully.')
    os.remove(yuzu_package_path)
    return f'Yuzu [{version}] 安装完成.'


def install_key_to_yuzu(target_name=None):
    send_notify('正在获取 key 信息...')
    keys_info = get_keys_info()
    if not target_name and config.yuzu.yuzu_firmware:
        for k in keys_info:
            if config.yuzu.yuzu_firmware in k:
                logger.info(f'key [{k}] maybe suitable for firmware [{config.yuzu.yuzu_firmware}].')
                target_name = k
                break
    if not target_name:
        idx2name = {}
        logger.info('Follow keys are available:')
        for i, name in enumerate(keys_info.keys()):
            logger.info(f'  {i}: {name}')
            idx2name[str(i)] = name
        choose = input('Choose num: ')
        if choose not in idx2name:
            raise RuntimeError(f'Not available choose: {choose}')
        target_name = idx2name[choose]
    elif config.yuzu.key_file == target_name:
        logger.info(f'Current key file is same as target file [{target_name}], skip install.')
        return f'当前的 key 就是 [{target_name}], 跳过安装.'
    file = download_keys_by_name(target_name)
    with py7zr.SevenZipFile(file) as zf:
        zf: py7zr.SevenZipFile = zf
        keys_path = Path(config.yuzu.yuzu_path).joinpath(r'user\keys')
        keys_path.mkdir(parents=True, exist_ok=True)
        logger.info(f'Extracting keys to {keys_path}')
        send_notify('提取 key 至目录...')
        zf.extractall(keys_path)
        config.yuzu.key_file = target_name
        dump_config()
        logger.info(f'Keys [{target_name}] install successfully.')
    return f'Keys [{target_name}] 安装完成.'


def install_firmware_to_yuzu(firmware_version=None):
    if firmware_version == config.yuzu.yuzu_firmware:
        logger.info(f'Current firmware are same as target version [{firmware_version}], skip install.')
        return f'当前的 固件 就是 [{firmware_version}], 跳过安装.'
    send_notify('正在获取固件信息...')
    firmware_infos = get_firmware_infos()
    if firmware_version:
        firmware_map = {fi['version']: fi for fi in firmware_infos}
        target_info = firmware_map.get(firmware_version)
    else:
        idx2info = {}
        logger.info('Available firmwares:')
        for i in range(5):
            logger.info(f"  {i}: {firmware_infos[i]}")
            idx2info[str(i)] = firmware_infos[i]
        choose = input('Choose num: ')
        if choose not in idx2info:
            raise RuntimeError(f'Invalid choose: {choose}')
        target_info = idx2info[choose]
        firmware_version = target_info['version']
    if firmware_version == config.yuzu.yuzu_firmware:
        logger.info(f'Current firmware are same as target version [{firmware_version}], skip install.')
        return f'当前的 固件 就是 [{firmware_version}], 跳过安装.'
    if not target_info:
        logger.info(f'Target firmware version [{firmware_version}] not found, skip install.')
        return f'Target firmware version [{firmware_version}] not found, skip install.'
    url = get_finial_url(target_info['url'])
    send_notify(f'开始下载固件...')
    logger.info(f"downloading firmware of [{firmware_version}] from {url}")
    info = download(url)
    file = info.files[0]
    yuzu_path = Path(config.yuzu.yuzu_path)
    import zipfile
    with zipfile.ZipFile(file.path, 'r') as zf:
        firmware_path = yuzu_path.joinpath(r'\user\nand\system\Contents\registered')
        shutil.rmtree(firmware_path, ignore_errors=True)
        firmware_path.mkdir(parents=True, exist_ok=True)
        send_notify(f'开始解压安装固件...')
        logger.info(f'Unzipping firmware files to {firmware_path}')
        zf.extractall(firmware_path)
        config.yuzu.yuzu_firmware = firmware_version
        dump_config()
        logger.info(f'Firmware of [{firmware_version}] install successfully.')
    os.remove(file.path)
    return f'固件 [{firmware_version}] 安装成功，请安装相应的 key 至 yuzu.'


def detect_yuzu_version():
    send_notify('正在检测 yuzu 版本...')
    yz_path = Path(config.yuzu.yuzu_path).joinpath('yuzu.exe')
    if not yz_path.exists():
        send_notify('未能找到 yuzu 程序')
        return None
    st_inf = subprocess.STARTUPINFO()
    st_inf.dwFlags = st_inf.dwFlags | subprocess.STARTF_USESHOWWINDOW
    subprocess.Popen(['powershell', 'Start-Process', str(yz_path.absolute()), '-WindowStyle', 'Hidden'],
                     startupinfo=st_inf)
    time.sleep(3)
    from pywinauto import Desktop
    windows = Desktop().windows()
    version = None
    for w in windows:
        if w.window_text().startswith('yuzu Early Access '):
            version = w.window_text()[18:]
            send_notify(f'当前 yuzu 版本 [{version}]')
            break
    import psutil
    for p in psutil.process_iter():
        if p.name() == 'yuzu.exe':
            p.kill()
            break
    if version:
        config.yuzu.yuzu_version = version
        dump_config()
        return version


if __name__ == '__main__':
    # install_yuzu()
    # install_firmware_to_yuzu()
    # install_key_to_yuzu()
    print(detect_yuzu_version())

