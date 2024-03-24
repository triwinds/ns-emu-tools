import os
import shutil
import time
from repository.suyu import *
from exception.common_exception import VersionNotFoundException, IgnoredException
from module.msg_notifier import send_notify
from module.downloader import download
from module.network import get_finial_url
from utils.package import uncompress
from config import config
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
        if asset['name'].endswith('Windows_x64.7z'):
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


def install_suyu_release(tag_name):
    release_info = get_release_by_tag_name(tag_name)
    if not release_info:
        raise VersionNotFoundException(tag_name, 'dev', 'suyu')
    zip_file = download_suyu_release(tag_name, release_info)
    tmp_dir = Path(tempfile.gettempdir()).joinpath(uuid.uuid4().hex)
    logger.info(f'Unpacking suyu files...')
    send_notify('正在解压 suyu 文件...')
    uncompress(zip_file, tmp_dir)
    copy_back_suyu_files(tmp_dir.joinpath('Release'), config.suyu.path)
    if config.setting.download.autoDeleteAfterInstall:
        os.remove(zip_file)
    logger.info(f'Suyu [{tag_name}] installation finished.')
    send_notify(f'Suyu [{tag_name}] 安装完成.')
    from module.common import check_and_install_msvc
    check_and_install_msvc()


if __name__ == '__main__':
    install_suyu_release('v0.0.1')
