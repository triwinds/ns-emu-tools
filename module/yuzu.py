import os
import shutil
import tempfile
from pathlib import Path

import py7zr

from config import yuzu_config, dump_yuzu_config
from module.common import get_firmware_infos, get_keys_info, download_keys_by_name
from module.downloader import download
from repository.yuzu import get_latest_yuzu_release_info, get_yuzu_release_info_by_version


def download_yuzu(release_info):
    assets = release_info['assets']
    for asset in assets:
        if asset['content_type'] == 'application/x-7z-compressed':
            url = asset['browser_download_url'].replace('https://github.com', 'https://cfrp.e6ex.com/gh')
            print(f"downloading yuzu from {url}")
            info = download(url)
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
        return f'Current yuzu version is same as target version [{version}], skip install.'
    print(f'target yuzu version: {release_info["tag_name"][3:]}')
    yuzu_path = Path(yuzu_config.yuzu_path)
    print(f'target yuzu path: {yuzu_path}')
    yuzu_package_path = download_yuzu(release_info)
    with py7zr.SevenZipFile(yuzu_package_path) as zf:
        zf: py7zr.SevenZipFile = zf
        print(f'Unpacking yuzu files...')
        zf.extractall(tempfile.gettempdir())
        tmp_dir = Path(tempfile.gettempdir()).joinpath('yuzu-windows-msvc-early-access')
        for useless_file in tmp_dir.glob('yuzu-windows-msvc-source-*.tar.xz'):
            os.remove(useless_file)
        print(f'Copy back yuzu files...')
        shutil.copytree(tmp_dir, yuzu_path, dirs_exist_ok=True)
        shutil.rmtree(tmp_dir)
        yuzu_config.yuzu_version = version
        dump_yuzu_config()
        print(f'Yuzu of [{version}] install successfully.')
    os.remove(yuzu_package_path)
    return f'Yuzu of [{version}] install successfully.'


def install_key_to_yuzu(target_name=None):
    keys_info = get_keys_info()
    if not target_name and yuzu_config.yuzu_firmware:
        for k in keys_info:
            if yuzu_config.yuzu_firmware in k:
                print(f'key [{k}] maybe suitable for firmware [{yuzu_config.yuzu_firmware}].')
                target_name = k
                break
    if not target_name:
        idx2name = {}
        print('Follow keys are available:')
        for i, name in enumerate(keys_info.keys()):
            print(f'  {i}: {name}')
            idx2name[str(i)] = name
        choose = input('Choose num: ')
        if choose not in idx2name:
            raise RuntimeError(f'Not available choose: {choose}')
        target_name = idx2name[choose]
    elif yuzu_config.key_file == target_name:
        print(f'Current key file is same as target file [{target_name}], skip install.')
        return f'Current key file is same as target file [{target_name}], skip install.'
    file = download_keys_by_name(target_name)
    with py7zr.SevenZipFile(file) as zf:
        zf: py7zr.SevenZipFile = zf
        keys_path = Path(yuzu_config.yuzu_path).joinpath(r'user\keys')
        keys_path.mkdir(parents=True, exist_ok=True)
        print(f'Extracting keys to {keys_path}')
        zf.extractall(keys_path)
        yuzu_config.key_file = target_name
        dump_yuzu_config()
        print(f'Keys [{target_name}] install successfully.')
    return f'Keys [{target_name}] install successfully.'


def install_firmware_to_yuzu(firmware_version=None):
    firmware_infos = get_firmware_infos()
    if firmware_version:
        firmware_map = {fi['version']: fi for fi in firmware_infos}
        target_info = firmware_map.get(firmware_version)
    else:
        idx2info = {}
        print('Available firmwares:')
        for i in range(5):
            print(f"  {i}: {firmware_infos[i]}")
            idx2info[str(i)] = firmware_infos[i]
        choose = input('Choose num: ')
        if choose not in idx2info:
            raise RuntimeError(f'Invalid choose: {choose}')
        target_info = idx2info[choose]
        firmware_version = target_info['version']
    if firmware_version == yuzu_config.yuzu_firmware:
        print(f'Current firmware are same as target version [{firmware_version}], skip install.')
        return f'Current firmware are same as target version [{firmware_version}], skip install.'
    if not target_info:
        print(f'Target firmware version [{firmware_version}] not found, skip install.')
        return f'Target firmware version [{firmware_version}] not found, skip install.'
    url = target_info['url'].replace('https://archive.org', 'https://cfrp.e6ex.com/archive')
    print(f"downloading firmware of [{firmware_version}] from {url}")
    info = download(url)
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
    return f'固件 [{firmware_version}] 安装成功，请安装相应的 key 至 yuzu.'


if __name__ == '__main__':
    # install_yuzu()
    # install_firmware_to_yuzu()
    install_key_to_yuzu()
