import subprocess
import os
from pathlib import Path
import logging
from config import config, dump_config
import shutil
from module.msg_notifier import send_notify

logger = logging.getLogger(__name__)
hactool_path = Path(os.path.realpath(os.path.dirname(__file__))).joinpath('hactool.exe')


def detect_firmware_version(emu_type: str):
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
    if version:
        if emu_type == 'yuzu':
            config.yuzu.yuzu_firmware = version
        else:
            config.ryujinx.firmware = version
        dump_config()
    return version


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
    if tmp_path.joinpath('file').exists():
        with open(tmp_path.joinpath('file'), 'rb') as f:
            f.seek(0x68)
            version = f.read(0x18).replace(b'\0', b'').decode()
            logger.info(f'firmware version: {version}')
            send_notify(f'固件版本: {version}')
    shutil.rmtree(tmp_path)
    return version


if __name__ == '__main__':
    detect_firmware_version('yuzu')
