import subprocess
import os
from pathlib import Path
import logging
from config import dump_config
import shutil
from module.msg_notifier import send_notify
from functools import lru_cache
from config import config
from module.downloader import download
from module.network import get_finial_url, get_durable_cache_session, request_github_api, get_github_download_url

from exception.common_exception import IgnoredException
import urllib.parse

logger = logging.getLogger(__name__)
hactool_path = Path(os.path.realpath(os.path.dirname(__file__))).joinpath('hactool.exe')


def _detect_firmware_version(emu_type: str):
    firmware_files = []
    version = None
    if emu_type == 'yuzu':
        from module.yuzu import get_yuzu_nand_path, get_yuzu_user_path
        firmware_path = get_yuzu_nand_path().joinpath('system', 'Contents', 'registered')
        key_path = get_yuzu_user_path().joinpath('keys', 'prod.keys')
        for file in firmware_path.glob('*.nca'):
            if not file.name.endswith('.cnmt.nca'):
                firmware_files.append(file)
    else:
        from module.ryujinx import get_ryujinx_user_folder
        firmware_path = get_ryujinx_user_folder().joinpath('bis', 'system', 'Contents', 'registered')
        key_path = get_ryujinx_user_folder().joinpath('system', 'prod.keys')
        for p in firmware_path.glob('**/00'):
            if p.is_file():
                firmware_files.append(p)
    if not key_path.exists():
        logger.info(f'prod keys not found in path: {key_path}')
        send_notify('未能找到相应的 prod.keys 文件')
        raise IgnoredException(f'prod keys not found in path: {key_path}')
    if not firmware_files:
        logger.info(f'no firmware files found in path: {firmware_path}')
        send_notify('未能找到相应的固件文件')
        raise IgnoredException(f'no firmware files found in path: {firmware_path}')
    target_file = find_target_firmware_file(firmware_files, key_path)
    if target_file:
        version = extract_version(target_file)
    return version


def detect_firmware_version(emu_type: str):
    version = None
    try:
        version = _detect_firmware_version(emu_type)
    except Exception as e:
        raise e
    finally:
        if emu_type == 'yuzu':
            config.yuzu.yuzu_firmware = version
        else:
            config.ryujinx.firmware = version
        dump_config()


def find_target_firmware_file(firmware_files, key_path):
    logger.info(f'scanning firmware files...')
    send_notify('开始扫描固件文件...')
    from module.nsz_wrapper import reload_key, parse_nca_header
    reload_key(key_path)
    from nsz.Fs.Type import Content
    for file in firmware_files:
        header = parse_nca_header(file)
        if header and header.titleId == '0100000000000809' and header.contentType == Content.DATA:
            logger.info(f'target firmware file: {file}')
            send_notify(f'找到目标固件文件: {file}')
            return file


def extract_version(target_file):
    logger.info(f'decrypt file: {target_file}')
    send_notify(f'开始解析目标固件文件...')
    from module.nsz_wrapper import read_firmware_version_from_nca
    version = read_firmware_version_from_nca(target_file)
    if not version:
        logger.info(f'Fail to decrypt file.')
        send_notify(f'无法解析固件文件, 可能是当前使用的密钥与固件版本不匹配')
        return
    logger.info(f'Firmware version: {version}')
    send_notify(f'固件版本: {version}')
    return version


def get_firmware_infos():
    if config.setting.network.firmwareDownloadSource == 'nsarchive':
        return get_firmware_infos_from_nsarchive()
    else:
        return get_firmware_infos_from_github()


@lru_cache(1)
def get_firmware_infos_from_nsarchive():
    url = 'https://nsarchive.e6ex.com/nsf/firmwares.json'
    resp = get_durable_cache_session().get(get_finial_url(url), timeout=15)
    res = []
    for info in resp.json():
        info['version'] = info['name'][9:]
        info['url'] = 'https://nsarchive.e6ex.com/nsf/' + urllib.parse.quote(info['filename'])
        res.append(info)
    return res


@lru_cache(1)
def get_firmware_infos_from_github():
    data = request_github_api('https://api.github.com/repos/THZoria/NX_Firmware/releases')
    res = []
    for release in data:
        target_asset = None
        for asset in release['assets']:
            if 'zip' in asset['content_type']:
                target_asset = asset
                break
        if target_asset is None:
            break
        info = {
            'name': release['name'][9:],
            'version': release['tag_name'],
            'url': target_asset['browser_download_url'],
            'filename': target_asset['name'],
            'size': _sizeof_fmt(target_asset['size']),
        }
        res.append(info)
    return res


def _sizeof_fmt(num, suffix="B"):
    for unit in ("", "Ki", "Mi", "Gi", "Ti", "Pi", "Ei", "Zi"):
        if abs(num) < 1024.0:
            return f"{num:3.1f}{unit}{suffix}"
        num /= 1024.0
    return f"{num:.1f}Yi{suffix}"


def check_file_md5(file: Path, target_md5: str):
    if not file.exists() or not file.is_file():
        return None
    if not target_md5:
        return True
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
    url = target_info['url']
    if 'github.com' in url:
        url = get_github_download_url(url)
    send_notify(f'开始下载固件...')
    logger.info(f"downloading firmware of [{firmware_version}] from {url}")
    info = download(url)
    file = info.files[0]
    if config.setting.download.verifyFirmwareMd5 and not check_file_md5(file.path, target_info.get('md5')):
        logger.info(f'firmware md5 not match, removing file [{file}]...')
        os.remove(file.path)
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


def get_available_firmware_sources():
    return [
        ['由 github.com/THZoria/NX_Firmware 提供的固件', 'github'],
        ['由 darthsternie.net 提供的固件', 'nsarchive']
    ]


if __name__ == '__main__':
    from pprint import pp
    detect_firmware_version('yuzu')
    # pp(get_firmware_infos())
    # pp(get_firmware_infos_from_github())
