import shutil
import subprocess
import time
from pathlib import Path

from module.downloader import download
from repository.ryujinx import get_ryujinx_release_info_by_version
from utils.network import get_finial_url
from module.msg_notifier import send_notify
from config import config, dump_config
import logging
import os


logger = logging.getLogger(__name__)


def install_ryujinx_by_version(target_version: str):
    if config.ryujinx.version == target_version:
        logger.info(f'Current ryujinx version is same as target version [{target_version}], skip install.')
        return f'当前就是 [{target_version}] 版本的 ryujinx , 跳过安装.'
    send_notify('正在获取 ryujinx 版本信息...')
    release_info = get_ryujinx_release_info_by_version(target_version)
    assets = release_info['assets']
    download_url = None
    for asset in assets:
        name: str = asset['name']
        if name.startswith('ryujinx-') and name.endswith('-win_x64.zip'):
            download_url = asset['browser_download_url']
            break
    if not download_url:
        send_notify(f'获取 ryujinx 下载链接失败')
        raise RuntimeError(f'No download url found with version: {target_version}')
    download_url = get_finial_url(download_url)
    logger.info(f'download ryujinx from url: {download_url}')
    send_notify(f'开始下载 ryujinx ...')
    info = download(download_url)
    file = info.files[0]
    ryujinx_path = Path(config.ryujinx.path)
    ryujinx_path.mkdir(parents=True, exist_ok=True)
    clear_ryujinx_folder(ryujinx_path)
    import zipfile
    with zipfile.ZipFile(file.path, 'r') as zf:
        import tempfile
        tmp_dir = Path(tempfile.gettempdir()).joinpath('ryujinx-install')
        logger.info(f'Unpacking ryujinx files to {tmp_dir}.')
        send_notify('正在解压 ryujinx 文件...')
        zf.extractall(str(tmp_dir.absolute()))
        ryujinx_tmp_dir = tmp_dir.joinpath('publish')
        logger.info(f'Copy back ryujinx files...')
        send_notify('安装 ryujinx 文件至目录...')
        kill_all_ryujinx_instance()
        shutil.copytree(ryujinx_tmp_dir, ryujinx_path, dirs_exist_ok=True)
        shutil.rmtree(tmp_dir)
        config.ryujinx.version = target_version
        dump_config()
        logger.info(f'Ryujinx of [{target_version}] install successfully.')
    os.remove(file.path)
    from module.common import check_and_install_msvc
    check_and_install_msvc()
    return f'Ryujinx [{target_version}] 安装完成.'


def install_firmware_to_ryujinx(firmware_version=None):
    if firmware_version == config.ryujinx.firmware:
        logger.info(f'Current firmware are same as target version [{firmware_version}], skip install.')
        send_notify(f'当前的 固件 就是 [{firmware_version}], 跳过安装.')
        return
    firmware_path = get_ryujinx_user_folder().joinpath(r'bis\system\Contents\registered')
    shutil.rmtree(firmware_path, ignore_errors=True)
    firmware_path.mkdir(parents=True, exist_ok=True)
    tmp_dir = firmware_path.joinpath('tmp/')
    from module.common import install_firmware
    new_version = install_firmware(firmware_version, tmp_dir)
    if new_version:
        for path in tmp_dir.glob('*.nca'):
            name = path.name[:-9] + '.nca' if path.name.endswith('.cnmt.nca') else path.name
            nca_dir = firmware_path.joinpath(name)
            nca_dir.mkdir()
            path.rename(nca_dir.joinpath('00'))
        shutil.rmtree(tmp_dir, ignore_errors=True)
        config.ryujinx.firmware = new_version
        dump_config()
        send_notify(f'固件 [{firmware_version}] 安装成功，请安装相应的 key 至 Ryujinx.')


def clear_ryujinx_folder(ryujinx_path: Path):
    send_notify('清除旧版 ryujinx 文件...')
    for path in ryujinx_path.glob('*'):
        if path.name == 'portable':
            continue
        logger.debug(f'removing path: {path}')
        if path.is_dir():
            shutil.rmtree(path)
        else:
            os.remove(path)


def kill_all_ryujinx_instance():
    import psutil
    kill_flag = False
    for p in psutil.process_iter():
        if p.name().startswith('Ryujinx.'):
            send_notify(f'关闭 Ryujinx 进程 [{p.pid}]')
            logger.info(f'kill Ryujinx process [{p.pid}]')
            p.kill()
            kill_flag = True
    if kill_flag:
        time.sleep(1)


def get_ryujinx_user_folder():
    ryujinx_path = Path(config.ryujinx.path)
    if ryujinx_path.joinpath('portable/').exists():
        return ryujinx_path.joinpath('portable/')
    elif Path(os.environ['appdata']).joinpath('Ryujinx/').exists():
        return Path(os.environ['appdata']).joinpath('Ryujinx/')
    return ryujinx_path.joinpath('portable/')


def open_ryujinx_keys_folder():
    keys_path = get_ryujinx_user_folder().joinpath('system')
    keys_path.mkdir(parents=True, exist_ok=True)
    keys_path.joinpath('把prod.keys放当前目录.txt').touch(exist_ok=True)
    logger.info(f'open explorer on path {keys_path}')
    subprocess.Popen(f'explorer "{str(keys_path.absolute())}"')


def start_ryujinx():
    rj_path = Path(config.ryujinx.path).joinpath('Ryujinx.exe')
    if rj_path.exists():
        logger.info(f'starting Ryujinx from: {rj_path}')
        subprocess.Popen([rj_path])
    else:
        logger.error(f'Ryujinx not exist in [{rj_path}]')
        raise RuntimeError(f'Ryujinx not exist in [{rj_path}]')


if __name__ == '__main__':
    # install_ryujinx_by_version('1.1.335')
    # clear_ryujinx_folder(Path(config.ryujinx.path))
    # install_firmware_to_ryujinx('15.0.0')
    open_ryujinx_keys_folder()
