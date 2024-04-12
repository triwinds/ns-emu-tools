import os
import shutil
import subprocess
import time
from repository.suyu import *
from exception.common_exception import VersionNotFoundException, IgnoredException
from module.msg_notifier import send_notify
from module.downloader import download
from module.network import get_finial_url
from storage import storage, dump_storage, add_suyu_history
from utils.common import decode_yuzu_path
from utils.package import uncompress
from config import config, dump_config, SuyuConfig
from pathlib import Path
import uuid
import tempfile
import logging


logger = logging.getLogger(__name__)


def list_suyu_releases():
    suyu_releases = load_suyu_releases()
    return suyu_releases


def download_suyu_release(tag_name, release_info):
    assets = release_info['assets']
    target_asset = None
    for asset in assets:
        if 'windows' in asset['name'].lower():
            target_asset = asset
            break
    if not target_asset:
        send_notify(f'未找到 Suyu [{tag_name}] 对应的 Windows 安装包')
        raise IgnoredException(f'No windows package found for tag {tag_name}')
    return download(get_finial_url(target_asset['browser_download_url'])).files[0].path


def copy_back_suyu_files(tmp_dir: Path, suyu_path: Path):
    logger.info(f'Copy back suyu files...')
    send_notify('安装 suyu 文件至目录...')
    try:
        shutil.copytree(tmp_dir, suyu_path, dirs_exist_ok=True)
        time.sleep(0.5)
    except Exception as e:
        from exception.install_exception import FailToCopyFiles
        raise FailToCopyFiles(e, 'Suzu 文件复制失败')
    shutil.rmtree(tmp_dir)


def install_suyu(tag_name):
    release_info = get_release_by_tag_name(tag_name)
    if not release_info:
        raise VersionNotFoundException(tag_name, 'dev', 'suyu')
    zip_file = download_suyu_release(tag_name, release_info)
    tmp_dir = Path(tempfile.gettempdir()).joinpath(uuid.uuid4().hex)
    logger.info(f'Unpacking suyu files...')
    send_notify('正在解压 suyu 文件...')
    uncompress(zip_file, tmp_dir)
    clear_suyu_folder()
    file_contents = os.listdir(tmp_dir)
    if len(file_contents) > 1:
        copy_back_suyu_files(tmp_dir, config.suyu.path)
    else:
        file_contents_dir = file_contents[0]
        logger.info(f'copy dir: {file_contents_dir}')
        copy_back_suyu_files(tmp_dir.joinpath(file_contents_dir), config.suyu.path)
        shutil.rmtree(tmp_dir)
    config.suyu.version = tag_name
    dump_config()
    if config.setting.download.autoDeleteAfterInstall:
        os.remove(zip_file)
    logger.info(f'Suyu [{tag_name}] installation finished.')
    send_notify(f'Suyu [{tag_name}] 安装完成.')
    from module.common import check_and_install_msvc
    check_and_install_msvc()


def clear_suyu_folder():
    suyu_path = Path(config.suyu.path)
    if not suyu_path.exists():
        return
    logger.info(f'clear suyu folder: {suyu_path}')
    send_notify(f'清理 suyu 文件夹: {suyu_path}')
    for file in suyu_path.iterdir():
        if file.name.lower() == 'user':
            continue
        if file.is_file():
            file.unlink()
        elif file.is_dir():
            shutil.rmtree(file)


def get_suyu_user_path():
    suyu_path = Path(config.suyu.path)
    if suyu_path.joinpath('user/').exists():
        return suyu_path.joinpath('user/')
    elif Path(os.environ['appdata']).joinpath('suyu/').exists():
        return Path(os.environ['appdata']).joinpath('suyu/')
    return suyu_path.joinpath('user/')


def get_suyu_exe_path():
    suyu_path = Path(config.suyu.path)
    return suyu_path.joinpath('suyu.exe')


def start_suyu():
    sy_path = get_suyu_exe_path()
    if sy_path.exists():
        logger.info(f'starting suyu from {sy_path}')
        subprocess.Popen([sy_path])
    else:
        logger.info(f'suyu not exist in [{sy_path}]')
        raise IgnoredException(f'suyu not exist in [{sy_path}]')
    
    
def open_suyu_keys_folder():
    keys_path = get_suyu_user_path().joinpath('keys')
    keys_path.mkdir(parents=True, exist_ok=True)
    keys_path.joinpath('把prod.keys放当前目录.txt').touch(exist_ok=True)
    logger.info(f'open explorer on path {keys_path}')
    subprocess.Popen(f'explorer "{str(keys_path.absolute())}"')
    
    
def _get_suyu_data_storage_config(user_path: Path):
    config_path = user_path.joinpath('config/qt-config.ini')
    if config_path.exists():
        import configparser
        suyu_qt_config = configparser.ConfigParser()
        suyu_qt_config.read(str(config_path.absolute()), encoding='utf-8')
        # data = {section: dict(suyu_qt_config[section]) for section in suyu_qt_config.sections()}
        # print(data)
        data_storage = suyu_qt_config['Data%20Storage']
        logger.debug(dict(data_storage))
        return data_storage
    
    
def get_suyu_nand_path():
    user_path = get_suyu_user_path()
    nand_path = user_path.joinpath('nand')
    try:
        data_storage = _get_suyu_data_storage_config(user_path)
        if data_storage:
            path_str = data_storage.get('nand_directory')
            nand_path = Path(path_str)
            logger.info(f'use nand path from suyu config: {nand_path}')
    except Exception as e:
        logger.warning(f'fail in parse suyu qt-config, error msg: {str(e)}')
    return nand_path
    
    
def install_firmware_to_suyu(firmware_version=None):
    if firmware_version == config.suyu.firmware:
        logger.info(f'Current firmware are same as target version [{firmware_version}], skip install.')
        send_notify(f'当前的 固件 就是 [{firmware_version}], 跳过安装.')
        return
    from module.firmware import install_firmware
    firmware_path = get_suyu_nand_path().joinpath(r'system\Contents\registered')
    new_version = install_firmware(firmware_version, firmware_path)
    if new_version:
        config.suyu.firmware = new_version
        dump_config()
        send_notify(f'固件已安装至 {str(firmware_path)}')
        send_notify(f'固件 [{firmware_version}] 安装成功，请安装相应的 key 至 suyu.')
        
        
def update_suyu_path(new_suyu_path: str):
    new_path = Path(new_suyu_path)
    if not new_path.exists():
        logger.info(f'create directory: {new_path}')
        new_path.mkdir(parents=True, exist_ok=True)
    if new_path.absolute() == Path(config.suyu.path).absolute():
        logger.info(f'No different with old suyu path, skip update.')
        return
    add_suyu_history(config.suyu)
    logger.info(f'setting suyu path to {new_path}')
    cfg = storage.suyu_history.get(str(new_path.absolute()), SuyuConfig())
    cfg.path = str(new_path.absolute())
    config.suyu = cfg
    if cfg.path not in storage.suyu_history:
        add_suyu_history(cfg)
    dump_config()


if __name__ == '__main__':
    install_suyu('v0.0.3')
