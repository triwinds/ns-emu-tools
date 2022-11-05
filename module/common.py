import os
import shutil
import subprocess
from functools import lru_cache
from pathlib import Path
from module.msg_notifier import send_notify
from module.downloader import download_path
import requests
import bs4
from utils.network import get_finial_url
import logging
from module.downloader import download

logger = logging.getLogger(__name__)


@lru_cache(1)
def get_firmware_infos():
    base_url = 'https://archive.org/download/nintendo-switch-global-firmwares/'
    resp = requests.get(get_finial_url(base_url))
    soup = bs4.BeautifulSoup(resp.text, features="html.parser")
    a_tags = soup.select('#maincontent > div > div > pre > table > tbody > tr > td > a')
    archive_versions = []
    for a in a_tags:
        name = a.text
        if name.startswith('Firmware ') and name.endswith('.zip'):
            size = a.parent.next_sibling.next_sibling.next_sibling.next_sibling.text
            version = name[9:-4]
            version_num = 0
            for num in version.split('.'):
                version_num *= 100
                version_num += int(''.join(ch for ch in num if ch.isdigit()))
            archive_versions.append({
                'name': name,
                'version': version,
                'size': size,
                'url': base_url + a.attrs['href'],
                'version_num': version_num,
            })
    archive_versions = sorted(archive_versions, key=lambda x: x['version_num'], reverse=True)
    return archive_versions


def check_and_install_msvc():
    windir = Path(os.environ['windir'])
    if windir.joinpath(r'System32\msvcp140_atomic_wait.dll').exists():
        logger.info(f'msvc already installed.')
        return
    from module.downloader import download
    from module.msg_notifier import send_notify
    send_notify('开始下载 msvc 安装包...')
    logger.info('downloading msvc installer...')
    download_info = download('https://aka.ms/vs/17/release/VC_redist.x64.exe')
    install_file = download_info.files[0]
    send_notify('安装 msvc...')
    logger.info('install msvc...')
    process = subprocess.Popen([install_file.path])
    # process.wait()


def check_update(prerelease=False):
    from repository.my_info import get_latest_release
    latest_release = get_latest_release(prerelease)
    latest_tag = latest_release['tag_name']
    from config import current_version
    latest_version_num = calc_version_num(latest_tag)
    current_version_num = calc_version_num(current_version)
    return latest_version_num > current_version_num, latest_tag


def calc_version_num(version_str: str):
    version_num = 0
    for s in version_str.split('.'):
        version_num *= 100
        num_str = ''.join(ch for ch in s if ch.isdigit())
        if num_str:
            version_num += int(num_str)
    return version_num


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
    import zipfile
    with zipfile.ZipFile(file.path, 'r') as zf:
        firmware_path = target_firmware_path
        shutil.rmtree(firmware_path, ignore_errors=True)
        firmware_path.mkdir(parents=True, exist_ok=True)
        send_notify(f'开始解压安装固件...')
        logger.info(f'Unzipping firmware files to {firmware_path}')
        zf.extractall(firmware_path)
        logger.info(f'Firmware of [{firmware_version}] install successfully.')
    os.remove(file.path)
    return firmware_version


if __name__ == '__main__':
    # infos = get_firmware_infos()
    # for info in infos:
    #     print(info)
    check_and_install_msvc()
