import os
import shutil
import tempfile
from pathlib import Path

import py7zr

from config import yuzu_config, dump_yuzu_config
from module.common import get_firmware_infos
from module.downloader import download
from repository.yuzu import get_latest_yuzu_release_info, get_yuzu_release_info_by_version


def download_yuzu(release_info):
    assets = release_info['assets']
    for asset in assets:
        if asset['content_type'] == 'application/x-7z-compressed':
            print(f"downloading yuzu from {asset['browser_download_url']}")
            info = download(asset['browser_download_url'])
            file = info.files[0]
            return file.path


def install_yuzu(target_version=None):
    if target_version:
        release_info = get_yuzu_release_info_by_version(target_version)
    else:
        release_info = get_latest_yuzu_release_info()
    version = release_info["tag_name"][3:]
    if version == yuzu_config.yuzu_version:
        print(f'Current yuzu version is same as target version [{version}], skip install.')
        return
    print(f'target yuzu version: {release_info["tag_name"][3:]}')
    yuzu_path = Path(yuzu_config.yuzu_path)
    print(f'target yuzu path: {yuzu_path}')
    yuzu_package_path = download_yuzu(release_info)
    with py7zr.SevenZipFile(yuzu_package_path) as zf:
        zf: py7zr.SevenZipFile = zf
        print(f'Unpacking yuzu files...')
        zf.extractall(tempfile.gettempdir())
        tmp_dir = Path(tempfile.gettempdir()).joinpath('yuzu-windows-msvc-early-access')
        print(f'Copy back yuzu files...')
        shutil.copytree(tmp_dir, yuzu_path, dirs_exist_ok=True)
        shutil.rmtree(tmp_dir)
        yuzu_config.yuzu_version = version
        dump_yuzu_config()
        print(f'Yuzu of [{version}] install successfully.')
    os.remove(yuzu_package_path)


def install_firmware_to_yuzu(firmware_version=None):
    firmware_infos = get_firmware_infos()
    if firmware_version:
        firmware_map = {fi['version']: fi for fi in firmware_infos}
        target_info = firmware_map.get(firmware_version)
    else:
        target_info = firmware_infos[0]
        firmware_version = target_info['version']
    if not target_info:
        print(f'Target firmware version [{firmware_version}] not found, skip install.')
        return
    print(f"downloading firmware of [{firmware_version}] from {target_info['url']}")
    info = download(target_info['url'])
    file = info.files[0]
    yuzu_path = Path(yuzu_config.yuzu_path)
    import zipfile
    with zipfile.ZipFile(file.path, 'r') as zf:
        firmware_path = yuzu_path.joinpath(r'\user\nand\system\Contents\registered')
        shutil.rmtree(firmware_path, ignore_errors=True)
        firmware_path.mkdir(parents=True, exist_ok=True)
        print(f'Unzipping firmware files to {firmware_path}')
        zf.extractall(firmware_path)
        yuzu_config.yuzu_firmware = firmware_version
        dump_yuzu_config()
        print(f'Firmware of [{firmware_version}] install successfully.')
    os.remove(file.path)


if __name__ == '__main__':
    # install_yuzu()
    install_firmware_to_yuzu()
