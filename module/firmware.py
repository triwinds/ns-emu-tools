import subprocess
import os
from pathlib import Path
import logging
from config import config, dump_config
import shutil
from module.msg_notifier import send_notify
import xmltodict
from functools import lru_cache
from config import config
from module.downloader import download
from utils.network import get_finial_url, get_durable_cache_session

logger = logging.getLogger(__name__)
hactool_path = Path(os.path.realpath(os.path.dirname(__file__))).joinpath('hactool.exe')


def _detect_firmware_version(emu_type: str):
    firmware_files = []
    version = None
    if emu_type == 'yuzu':
        from module.yuzu import get_yuzu_nand_path, get_yuzu_user_path
        firmware_path = get_yuzu_nand_path().joinpath(r'system\Contents\registered')
        key_path = get_yuzu_user_path().joinpath(r'keys/prod.keys')
        for file in firmware_path.glob('*.nca'):
            if not file.name.endswith('.cnmt.nca'):
                firmware_files.append(file)
    else:
        from module.ryujinx import get_ryujinx_user_folder
        firmware_path = get_ryujinx_user_folder().joinpath(r'bis\system\Contents\registered')
        key_path = get_ryujinx_user_folder().joinpath(r'system/prod.keys')
        for p in firmware_path.glob('**/00'):
            if p.is_file():
                firmware_files.append(p)
    if not key_path.exists():
        logger.error(f'prod keys not found in path: {key_path}')
        send_notify('未能找到相应的 prod.keys 文件')
        raise RuntimeError(f'prod keys not found in path: {key_path}')
    if not firmware_files:
        logger.error(f'no firmware files found in path: {firmware_path}')
        send_notify('未能找到相应的固件文件')
        raise RuntimeError(f'no firmware files found in path: {firmware_path}')
    target_file = find_target_firmware_file(firmware_files, key_path)
    if target_file:
        version = extract_version(target_file, key_path)
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
    for file in firmware_files:
        process = subprocess.Popen(f'"{str(hactool_path)}" -t  keygen -k "{str(key_path)}" -t nca "{str(file)}"',
                                   stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, shell=True)
        lines = process.communicate()[0].decode("utf-8").splitlines()
        title_id = None
        content_type = None
        for line in lines:
            if line.startswith('Title ID:'):
                title_id = line[9:].strip()
            elif line.startswith('Content Type:'):
                content_type = line[13:].strip()
        if title_id == '0100000000000809' and content_type == 'Data':
            logger.info(f'target firmware file: {file}')
            send_notify(f'找到目标固件文件: {file}')
            return file


def extract_version(target_file, key_path):
    import tempfile
    logger.info(f'decrypt file: {target_file}')
    send_notify(f'开始解析目标固件文件: {target_file}')
    tmp_path = Path(tempfile.gettempdir()).joinpath('nst/')
    process = subprocess.Popen(f'"{str(hactool_path)}" -t  keygen -k "{str(key_path)}" -t nca "{str(target_file)}" '
                               f'--romfsdir="{str(tmp_path)}"', shell=True,
                               stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    process.wait()
    if not tmp_path.exists():
        logger.info(f'Fail to decrypt file.')
        send_notify(f'无法解析固件文件, 可能是当前使用的密钥与固件版本不匹配')
        return
    if tmp_path.joinpath('file').exists():
        with open(tmp_path.joinpath('file'), 'rb') as f:
            f.seek(0x68)
            version = f.read(0x18).replace(b'\0', b'').decode()
            logger.info(f'firmware version: {version}')
            send_notify(f'固件版本: {version}')
    shutil.rmtree(tmp_path)
    return version


@lru_cache(1)
def get_firmware_infos():
    import urllib.parse
    base_url = 'https://archive.org/download/nintendo-switch-global-firmwares/'
    url = base_url + 'nintendo-switch-global-firmwares_files.xml'
    resp = get_durable_cache_session().get(get_finial_url(url), timeout=5)
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


if __name__ == '__main__':
    detect_firmware_version('yuzu')
