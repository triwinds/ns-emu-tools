import logging
import os
import shutil
import subprocess
from functools import lru_cache
from pathlib import Path

import xmltodict

from config import config
from module.downloader import download
from module.msg_notifier import send_notify
from utils.network import get_finial_url, session

logger = logging.getLogger(__name__)


@lru_cache(1)
def get_firmware_infos():
    import urllib.parse
    base_url = 'https://archive.org/download/nintendo-switch-global-firmwares/'
    url = base_url + 'nintendo-switch-global-firmwares_files.xml'
    resp = session.get(get_finial_url(url), timeout=5)
    data = xmltodict.parse(resp.text)
    files = data['files']['file']
    res = []
    for info in files:
        if 'ZIP' != info['format']:
            continue
        info['name'] = info['@name']
        del info['@name']
        info['url'] = base_url + urllib.parse.quote(info['name'])
        version = info['name'][9:-4]
        info['version'] = version
        version_num = 0
        for num in version.split('.'):
            version_num *= 100
            version_num += int(''.join(ch for ch in num if ch.isdigit()))
        info['version_num'] = version_num
        res.append(info)
    res = sorted(res, key=lambda x: x['version_num'], reverse=True)
    return res


def check_file_md5(file: Path, target_md5: str):
    if not file.exists() or not file.is_file():
        return None
    import hashlib
    logger.debug(f'calculating md5 of file: {file}')
    send_notify('开始校验文件 md5...')
    hash_md5 = hashlib.md5()
    with file.open('rb') as f:
        for chunk in iter(lambda: f.read(4096), b""):
            hash_md5.update(chunk)
    file_md5 = hash_md5.hexdigest()
    send_notify(f'本地文件 md5: {file_md5}')
    send_notify(f'远端文件 md5: {target_md5}')
    logger.debug(f'file md5: {file_md5}, target md5: {target_md5}')
    return file_md5.lower() == target_md5.lower()


def check_and_install_msvc():
    windir = Path(os.environ['windir'])
    if windir.joinpath(r'System32\msvcp140_atomic_wait.dll').exists():
        logger.info(f'msvc already installed.')
        return
    from module.downloader import download
    from module.msg_notifier import send_notify
    send_notify('开始下载 msvc 安装包...')
    logger.info('downloading msvc installer...')
    download_info = download(get_finial_url('https://aka.ms/vs/17/release/VC_redist.x64.exe'))
    install_file = download_info.files[0]
    send_notify('安装 msvc...')
    logger.info('install msvc...')
    process = subprocess.Popen([install_file.path])
    # process.wait()


def install_firmware(firmware_version, target_firmware_path):
    send_notify('正在获取固件信息...')
    firmware_infos = get_firmware_infos()
    target_info = None
    if firmware_version:
        firmware_map = {fi['version']: fi for fi in firmware_infos}
        target_info = firmware_map.get(firmware_version)
    if not target_info:
        logger.info(f'Target firmware version [{firmware_version}] not found, skip install.')
        send_notify(f'Target firmware version [{firmware_version}] not found, skip install.')
        return
    url = get_finial_url(target_info['url'])
    send_notify(f'开始下载固件...')
    logger.info(f"downloading firmware of [{firmware_version}] from {url}")
    info = download(url)
    file = info.files[0]
    if config.setting.download.verifyFirmwareMd5 and not check_file_md5(file.path, target_info['md5']):
        logger.info(f'firmware md5 not match, removing file [{file}]...')
        os.remove(file)
        from exception.common_exception import Md5NotMatchException
        raise Md5NotMatchException()
    import zipfile
    with zipfile.ZipFile(file.path, 'r') as zf:
        firmware_path = target_firmware_path
        shutil.rmtree(firmware_path, ignore_errors=True)
        firmware_path.mkdir(parents=True, exist_ok=True)
        send_notify(f'开始解压安装固件...')
        logger.info(f'Unzipping firmware files to {firmware_path}')
        zf.extractall(firmware_path)
        logger.info(f'Firmware of [{firmware_version}] install successfully.')
    if config.setting.download.autoDeleteAfterInstall:
        os.remove(file.path)
    return firmware_version


if __name__ == '__main__':
    # infos = get_firmware_infos()
    # for info in infos:
    #     print(info)
    # check_and_install_msvc()
    print(check_update())
