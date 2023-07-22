import shutil
import subprocess
import time
from pathlib import Path

from exception.common_exception import VersionNotFoundException, IgnoredException
from module.downloader import download
from repository.ryujinx import get_ryujinx_release_info_by_version, get_ldn_ryujinx_release_info_by_version
from module.network import get_github_download_url
from module.msg_notifier import send_notify
from config import config, dump_config, RyujinxConfig
from storage import storage, add_ryujinx_history
import logging
import os
from utils.common import find_all_instances, kill_all_instances, is_path_in_use


logger = logging.getLogger(__name__)


def get_ryujinx_download_url(target_version: str, branch: str):
    if branch in {'mainline', 'ava'}:
        release_info = get_ryujinx_release_info_by_version(target_version)
        if 'tag_name' not in release_info:
            raise VersionNotFoundException(target_version, branch, 'ryujinx')
        assets = release_info['assets']
        for asset in assets:
            name: str = asset['name']
            if branch == 'mainline' and name.startswith('ryujinx-') and name.endswith('-win_x64.zip'):
                return asset['browser_download_url']
            elif branch == 'ava' and name.startswith('test-ava-ryujinx-') and name.endswith('-win_x64.zip'):
                return asset['browser_download_url']
    elif branch == 'ldn':
        release_info = get_ldn_ryujinx_release_info_by_version(target_version)
        if 'tag_name' not in release_info:
            raise VersionNotFoundException(target_version, branch, 'ryujinx')
        assets = release_info['assets']
        ava_ldn_url, mainline_ldn_url = None, None
        for asset in assets:
            name: str = asset['name']
            if name.startswith('ava-ryujinx-') and name.endswith('-win_x64.zip'):
                ava_ldn_url = asset['browser_download_url']
            elif name.startswith('ryujinx-') and name.endswith('-win_x64.zip'):
                mainline_ldn_url = asset['browser_download_url']
        return ava_ldn_url or mainline_ldn_url


def install_ryujinx_by_version(target_version: str, branch: str):
    if config.ryujinx.version == target_version and \
            (branch == 'ldn' or detect_current_branch() == branch):
        logger.info(f'Current ryujinx version is same as target version [{target_version}], skip install.')
        return f'当前就是 {branch} [{target_version}] 版本的 ryujinx , 跳过安装.'
    send_notify('正在获取 ryujinx 版本信息...')
    download_url = get_ryujinx_download_url(target_version, branch)
    if not download_url:
        send_notify(f'获取 ryujinx 下载链接失败')
        raise IgnoredException(f'No download url found with branch: {branch}, version: {target_version}')
    ryujinx_path = Path(config.ryujinx.path)
    ryujinx_path.mkdir(parents=True, exist_ok=True)
    download_url = get_github_download_url(download_url)
    logger.info(f'download ryujinx from url: {download_url}')
    send_notify(f'开始下载 ryujinx ...')
    info = download(download_url, 'ryujinx')
    file = info.files[0]
    from utils.package import uncompress
    import tempfile
    tmp_dir = Path(tempfile.gettempdir()).joinpath('ryujinx-install')
    logger.info(f'Unpacking ryujinx files to {tmp_dir}.')
    send_notify('正在解压 ryujinx 文件...')
    uncompress(file.path, tmp_dir)
    clear_ryujinx_folder(ryujinx_path)
    ryujinx_tmp_dir = tmp_dir.joinpath('publish')
    logger.info(f'Copy back ryujinx files...')
    send_notify('安装 ryujinx 文件至目录...')
    try:
        shutil.copytree(ryujinx_tmp_dir, ryujinx_path, dirs_exist_ok=True)
    except Exception as e:
        from exception.install_exception import FailToCopyFiles
        raise FailToCopyFiles(e, 'Ryujinx 文件复制失败')
    shutil.rmtree(tmp_dir)
    config.ryujinx.version = target_version
    config.ryujinx.branch = branch
    dump_config()
    logger.info(f'Ryujinx {branch} of [{target_version}] install successfully.')
    if config.setting.download.autoDeleteAfterInstall:
        os.remove(file.path)
    from module.common import check_and_install_msvc
    check_and_install_msvc()
    return f'Ryujinx {branch} [{target_version}] 安装完成.'


def install_firmware_to_ryujinx(firmware_version=None):
    if firmware_version == config.ryujinx.firmware:
        logger.info(f'Current firmware are same as target version [{firmware_version}], skip install.')
        send_notify(f'当前的 固件 就是 [{firmware_version}], 跳过安装.')
        return
    firmware_path = get_ryujinx_user_folder().joinpath(r'bis\system\Contents\registered')
    tmp_dir = firmware_path.parent.joinpath('tmp/')
    try:
        from module.firmware import install_firmware
        new_version = install_firmware(firmware_version, tmp_dir)
        if new_version:
            shutil.rmtree(firmware_path, ignore_errors=True)
            firmware_path.mkdir(parents=True, exist_ok=True)
            for path in tmp_dir.glob('*.nca'):
                name = path.name[:-9] + '.nca' if path.name.endswith('.cnmt.nca') else path.name
                nca_dir = firmware_path.joinpath(name)
                nca_dir.mkdir()
                path.rename(nca_dir.joinpath('00'))
            config.ryujinx.firmware = new_version
            dump_config()
            send_notify(f'固件已安装至 {str(firmware_path)}')
            send_notify(f'固件 [{firmware_version}] 安装成功，请安装相应的 key 至 Ryujinx.')
    finally:
        shutil.rmtree(tmp_dir, ignore_errors=True)


def clear_ryujinx_folder(ryujinx_path: Path):
    send_notify('清除旧版 ryujinx 文件...')
    for path in ryujinx_path.glob('Ryujinx*.exe'):
        logger.debug(f'removing path: {path}')
        os.remove(path)


def get_ryujinx_user_folder():
    ryujinx_path = Path(config.ryujinx.path)
    if ryujinx_path.joinpath('portable/').exists():
        return ryujinx_path.joinpath('portable/')
    elif Path(os.environ['appdata']).joinpath('Ryujinx/').exists():
        return Path(os.environ['appdata']).joinpath('Ryujinx/')
    return ryujinx_path.joinpath('portable/')


def get_ryujinx_exe_path():
    ryujinx_path = Path(config.ryujinx.path)
    if ryujinx_path.joinpath('Ryujinx.Ava.exe').exists():
        return ryujinx_path.joinpath('Ryujinx.Ava.exe')
    elif ryujinx_path.joinpath('Ryujinx.exe').exists():
        return ryujinx_path.joinpath('Ryujinx.exe')


def open_ryujinx_keys_folder():
    keys_path = get_ryujinx_user_folder().joinpath('system')
    keys_path.mkdir(parents=True, exist_ok=True)
    keys_path.joinpath('把prod.keys放当前目录.txt').touch(exist_ok=True)
    logger.info(f'open explorer on path {keys_path}')
    subprocess.Popen(f'explorer "{str(keys_path.absolute())}"')


def start_ryujinx():
    rj_path = get_ryujinx_exe_path()
    if rj_path:
        logger.info(f'starting Ryujinx from: {rj_path}')
        subprocess.Popen([rj_path])
    else:
        logger.info(f'Ryujinx exe not exist in [{config.ryujinx.path}]')
        raise IgnoredException(f'Ryujinx exe not exist in [{config.ryujinx.path}]')


def detect_current_branch():
    rj_path = get_ryujinx_exe_path()
    if not rj_path:
        return None
    if rj_path.name.endswith('Ava.exe'):
        return 'ava'
    else:
        return 'mainline'


def detect_ryujinx_version():
    send_notify('正在检测 Ryujinx 版本...')
    rj_path = get_ryujinx_exe_path()
    if not rj_path:
        send_notify('未能找到 Ryujinx 程序')
        config.ryujinx.version = None
        dump_config()
        return None
    instances = find_all_instances('Ryujinx.')
    if instances:
        logger.info(f'Ryujinx pid={[p.pid for p in instances]} is running, skip install.')
        send_notify(f'Ryujinx 正在运行中, 请先关闭 Ryujinx.')
        return
    config.ryujinx.branch = detect_current_branch()
    st_inf = subprocess.STARTUPINFO()
    st_inf.dwFlags = st_inf.dwFlags | subprocess.STARTF_USESHOWWINDOW
    subprocess.Popen([rj_path], startupinfo=st_inf, shell=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    version, try_count = None, 0
    from utils.common import get_all_window_name
    while try_count < 30 and not version:
        try_count += 1
        time.sleep(0.5)
        try:
            for window_name in get_all_window_name():
                if window_name.startswith('Ryujinx '):
                    version = window_name[16:] if window_name.startswith('Ryujinx Console ') else window_name[8:]
                    send_notify(f'当前 Ryujinx 版本 [{version}]')
                    logger.info(f'Current Ryujinx version: {version}')
                    break
        except:
            logger.exception('error occur in get_all_window_name')
    kill_all_instances('Ryujinx.')
    if version:
        if 'ldn' in version:
            idx = version.index('ldn')
            version = version[idx+3:]
            config.ryujinx.branch = 'ldn'
    else:
        send_notify(f'检测失败！没有找到 Ryujinx 窗口...')
    config.ryujinx.version = version
    dump_config()
    return version


def update_ryujinx_path(new_ryujinx_path: str):
    new_path = Path(new_ryujinx_path)
    if not new_path.exists():
        logger.info(f'create directory: {new_path}')
        new_path.mkdir(parents=True, exist_ok=True)
    if new_path.absolute() == Path(config.ryujinx.path).absolute():
        logger.info(f'No different with old ryujinx path, skip update.')
        return
    add_ryujinx_history(config.ryujinx)
    logger.info(f'setting ryujinx path to {new_path}')
    cfg = storage.ryujinx_history.get(str(new_path.absolute()), RyujinxConfig())
    cfg.path = str(new_path.absolute())
    config.ryujinx = cfg
    if cfg.path not in storage.ryujinx_history:
        add_ryujinx_history(cfg)
    dump_config()


if __name__ == '__main__':
    # install_ryujinx_by_version('1.1.338', 'ava')
    # clear_ryujinx_folder(Path(config.ryujinx.path))
    # install_firmware_to_ryujinx('15.0.0')
    # open_ryujinx_keys_folder()
    # detect_ryujinx_version()
    # install_ryujinx_by_version('3.0.1', 'ldn')
    # kill_all_ryujinx_instance(Path(config.ryujinx.path))
    a = 1
    print()
