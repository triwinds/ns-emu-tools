import logging
import os
import platform
import plistlib
import shutil
import subprocess
import time
from pathlib import Path
from typing import Optional

from exception.common_exception import VersionNotFoundException, IgnoredException
from module.downloader import download
from repository.ryujinx import *
from module.network import get_finial_url
from module.msg_notifier import send_notify
from config import config, dump_config, RyujinxConfig
from storage import storage, add_ryujinx_history
from utils.common import find_all_instances, kill_all_instances


logger = logging.getLogger(__name__)

IS_WINDOWS = platform.system() == 'Windows'
IS_MAC = platform.system() == 'Darwin'


def _find_ryujinx_app_in_directory(base: Path) -> Optional[Path]:
    base = Path(base)
    if base.suffix == '.app' and base.exists():
        return base
    if not base.exists():
        return None
    direct = sorted([p for p in base.glob('Ryujinx*.app')])
    if direct:
        return direct[0]
    deep = sorted([p for p in base.rglob('Ryujinx*.app')])
    if deep:
        return deep[0]
    return None


def _install_ryujinx_on_mac(temp_dir: Path, target_path: Path) -> Path:
    app_bundle = _find_ryujinx_app_in_directory(temp_dir)
    if not app_bundle:
        raise IgnoredException('未能在安装包中找到 Ryujinx.app')
    target_path = Path(target_path)
    if target_path.suffix == '.app':
        destination = target_path
        destination.parent.mkdir(parents=True, exist_ok=True)
    else:
        target_path.mkdir(parents=True, exist_ok=True)
        destination = target_path.joinpath(app_bundle.name)
    if destination.exists():
        shutil.rmtree(destination, ignore_errors=True)
    logger.info(f'Copying Ryujinx app bundle to {destination}')
    send_notify('安装 ryujinx 文件至目录...')
    shutil.copytree(app_bundle, destination)
    return destination


def _get_installed_ryujinx_app() -> Optional[Path]:
    return _find_ryujinx_app_in_directory(Path(config.ryujinx.path))


def _get_ryujinx_mac_binary() -> Optional[Path]:
    app_bundle = _get_installed_ryujinx_app()
    if not app_bundle:
        return None
    binary_dir = app_bundle.joinpath('Contents', 'MacOS')
    if not binary_dir.exists():
        return None
    for candidate in binary_dir.iterdir():
        if candidate.is_file() and os.access(candidate, os.X_OK):
            return candidate
    return None


def _read_macos_app_version(app_bundle: Path) -> Optional[str]:
    plist_path = app_bundle.joinpath('Contents', 'Info.plist')
    if not plist_path.exists():
        return None
    with plist_path.open('rb') as fp:
        info = plistlib.load(fp)
    return info.get('CFBundleShortVersionString') or info.get('CFBundleVersion')


def get_ryujinx_download_url(target_version: str, branch: str):
    release_info = get_ryujinx_release_info_by_version(target_version, branch)
    if not release_info or not release_info.tag_name:
        raise VersionNotFoundException(target_version, branch, 'ryujinx')
    assets = release_info.assets
    selected_asset: Optional[ReleaseAsset] = None
    if IS_WINDOWS:
        for asset in assets:
            name = asset.name.lower()
            if name.endswith('-win_x64.zip') or ('windows' in name and name.endswith('.zip')):
                selected_asset = asset
                break
    elif IS_MAC:
        for asset in assets:
            name = asset.name.lower()
            if ('macos' in name or 'osx' in name or name.endswith('-mac.zip') or name.endswith('-macos.zip')) \
                    and (name.endswith('.zip') or name.endswith('.tar.gz')):
                selected_asset = asset
                break
    else:
        for asset in assets:
            name = asset.name.lower()
            if ('linux' in name or name.endswith('.tar.gz')) and not name.endswith('.sig'):
                selected_asset = asset
                break
    if not selected_asset:
        for asset in assets:
            if asset.name.lower().endswith('.zip') or asset.name.lower().endswith('.tar.gz'):
                selected_asset = asset
                break
    if selected_asset:
        return get_finial_url(selected_asset.download_url)
    logger.warning(f'No download url found with branch: {branch}, version: {target_version}')
    send_notify(f'没有找到 {branch} [{target_version}] 版本的 ryujinx 下载链接')


def install_ryujinx_by_version(target_version: str, branch: str):
    same_branch = False
    if config.ryujinx.version == target_version:
        if IS_WINDOWS:
            same_branch = branch == 'ldn' or detect_current_branch() == branch
        else:
            same_branch = (config.ryujinx.branch or 'mainline') == branch
    if same_branch:
        logger.info(f'Current ryujinx version is same as target version [{target_version}], skip install.')
        return f'当前就是 {branch} [{target_version}] 版本的 ryujinx , 跳过安装.'
    send_notify('正在获取 ryujinx 版本信息...')
    download_url = get_ryujinx_download_url(target_version, branch)
    if not download_url:
        send_notify('获取 ryujinx 下载链接失败')
        raise IgnoredException(f'No download url found with branch: {branch}, version: {target_version}')
    ryujinx_path = Path(config.ryujinx.path)
    if IS_WINDOWS:
        ryujinx_path.mkdir(parents=True, exist_ok=True)
    else:
        (ryujinx_path if ryujinx_path.suffix != '.app' else ryujinx_path.parent).mkdir(parents=True, exist_ok=True)
    logger.info(f'download ryujinx from url: {download_url}')
    send_notify('开始下载 ryujinx ...')
    info = download(download_url)
    file = info.files[0]
    archive_path = Path(file.path)
    from utils.package import uncompress
    import tempfile
    send_notify('正在解压 ryujinx 文件...')
    with tempfile.TemporaryDirectory(prefix='ryujinx-install-') as tmp_dir:
        tmp_path = Path(tmp_dir)
        uncompress(archive_path, tmp_path)
        if IS_WINDOWS:
            clear_ryujinx_folder(ryujinx_path)
            ryujinx_tmp_dir = tmp_path.joinpath('publish')
            if not ryujinx_tmp_dir.exists():
                subdirs = [p for p in tmp_path.iterdir() if p.is_dir()]
                if subdirs:
                    ryujinx_tmp_dir = subdirs[0]
                else:
                    ryujinx_tmp_dir = tmp_path
            logger.info('Copy back ryujinx files...')
            send_notify('安装 ryujinx 文件至目录...')
            try:
                shutil.copytree(ryujinx_tmp_dir, ryujinx_path, dirs_exist_ok=True)
            except Exception as e:
                from exception.install_exception import FailToCopyFiles
                raise FailToCopyFiles(e, 'Ryujinx 文件复制失败')
        elif IS_MAC:
            destination = _install_ryujinx_on_mac(tmp_path, ryujinx_path)
            logger.info(f'Ryujinx app installed at {destination}')
        else:
            raise IgnoredException('当前系统暂不支持安装 Ryujinx')
    config.ryujinx.version = target_version
    config.ryujinx.branch = branch
    dump_config()
    logger.info(f'Ryujinx {branch} of [{target_version}] install successfully.')
    if config.setting.download.autoDeleteAfterInstall and archive_path.exists():
        os.remove(archive_path)
    if IS_WINDOWS:
        from module.common import check_and_install_msvc
        check_and_install_msvc()
    return f'Ryujinx {branch} [{target_version}] 安装完成.'


def install_firmware_to_ryujinx(firmware_version=None):
    if firmware_version == config.ryujinx.firmware:
        logger.info(f'Current firmware are same as target version [{firmware_version}], skip install.')
        send_notify(f'当前的 固件 就是 [{firmware_version}], 跳过安装.')
        return
    firmware_path = get_ryujinx_user_folder().joinpath('bis', 'system', 'Contents', 'registered')
    tmp_dir = firmware_path.parent.joinpath('tmp')
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
    if not IS_WINDOWS:
        return
    send_notify('清除旧版 ryujinx 文件...')
    for path in ryujinx_path.glob('Ryujinx*.exe'):
        logger.debug(f'removing path: {path}')
        os.remove(path)


def get_ryujinx_user_folder():
    if IS_MAC:
        support_path = Path.home().joinpath('Library', 'Application Support', 'Ryujinx')
        support_path.mkdir(parents=True, exist_ok=True)
        return support_path
    ryujinx_path = Path(config.ryujinx.path)
    portable_path = ryujinx_path.joinpath('portable')
    if portable_path.exists():
        return portable_path
    appdata = os.environ.get('appdata')
    if appdata:
        appdata_path = Path(appdata)
        candidate = appdata_path.joinpath('Ryujinx')
        if candidate.exists():
            return candidate
    return portable_path


def get_ryujinx_exe_path():
    if IS_MAC:
        return _get_ryujinx_mac_binary()
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
    if IS_MAC:
        subprocess.Popen(['open', str(keys_path.resolve())])
    elif IS_WINDOWS:
        subprocess.Popen(f'explorer "{str(keys_path.resolve())}"')
    else:
        subprocess.Popen(['xdg-open', str(keys_path.resolve())])


def start_ryujinx():
    if IS_MAC:
        app_bundle = _get_installed_ryujinx_app()
        if app_bundle and app_bundle.exists():
            logger.info(f'starting Ryujinx from app bundle: {app_bundle}')
            subprocess.Popen(['open', '-n', str(app_bundle.resolve())])
            return
        logger.info(f'Ryujinx app bundle not found in [{config.ryujinx.path}]')
        raise IgnoredException(f'Ryujinx app bundle not found in [{config.ryujinx.path}]')
    rj_path = get_ryujinx_exe_path()
    if rj_path:
        logger.info(f'starting Ryujinx from: {rj_path}')
        subprocess.Popen([rj_path])
    else:
        logger.info(f'Ryujinx exe not exist in [{config.ryujinx.path}]')
        raise IgnoredException(f'Ryujinx exe not exist in [{config.ryujinx.path}]')


def detect_current_branch():
    if IS_MAC:
        return config.ryujinx.branch or 'mainline'
    rj_path = get_ryujinx_exe_path()
    if not rj_path:
        return None
    if rj_path.name.endswith('Ava.exe'):
        return 'ava'
    else:
        return 'mainline'


def detect_ryujinx_version():
    send_notify('正在检测 Ryujinx 版本...')
    if IS_MAC:
        app_bundle = _get_installed_ryujinx_app()
        if not app_bundle or not app_bundle.exists():
            send_notify('未能找到 Ryujinx 程序')
            config.ryujinx.version = None
            dump_config()
            return None
        version = _read_macos_app_version(app_bundle)
        if version:
            send_notify(f'当前 Ryujinx 版本 [{version}]')
            logger.info(f'Current Ryujinx version: {version}')
        else:
            send_notify('未能从 Ryujinx 应用信息中读取版本号')
            logger.warning(f'Info.plist does not contain version info for {app_bundle}')
        if not config.ryujinx.branch:
            config.ryujinx.branch = 'mainline'
        config.ryujinx.version = version
        dump_config()
        return version
    rj_path = get_ryujinx_exe_path()
    if not rj_path:
        send_notify('未能找到 Ryujinx 程序')
        config.ryujinx.version = None
        dump_config()
        return None
    instances = find_all_instances('Ryujinx.')
    if instances:
        logger.info(f'Ryujinx pid={[p.pid for p in instances]} is running, skip install.')
        send_notify('Ryujinx 正在运行中, 请先关闭 Ryujinx.')
        return
    config.ryujinx.branch = detect_current_branch()
    if IS_WINDOWS:
        st_inf = subprocess.STARTUPINFO()
        st_inf.dwFlags = st_inf.dwFlags | subprocess.STARTF_USESHOWWINDOW
        subprocess.Popen([rj_path], startupinfo=st_inf, shell=True,
                         stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    else:
        subprocess.Popen([rj_path], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    time.sleep(3)
    version, try_count = None, 0
    from utils.common import get_all_window_name
    while try_count < 30 and not version:
        try_count += 1
        time.sleep(0.5)
        try:
            for window_name in get_all_window_name():
                if window_name.startswith('Ryujinx ') and '-' not in window_name:
                    version = window_name[16:] if window_name.startswith('Ryujinx Console ') else window_name[8:]
                    if version.startswith('Console '):
                        version = version[9:]
                    send_notify(f'当前 Ryujinx 版本 [{version}]')
                    logger.info(f'Current Ryujinx version: {version}')
                    break
        except:
            logger.exception('error occur in get_all_window_name')
    kill_all_instances('Ryujinx.')
    if version:
        if 'ldn' in version:
            idx = version.index('ldn')
            version = version[idx + 3:]
            config.ryujinx.branch = 'ldn'
        elif 'Canary' in version:
            idx = version.index('Canary')
            version = version[idx + 7:]
            config.ryujinx.branch = 'canary'
    else:
        send_notify('检测失败！没有找到 Ryujinx 窗口...')
    config.ryujinx.version = version
    dump_config()
    return version


def update_ryujinx_path(new_ryujinx_path: str):
    new_path = Path(new_ryujinx_path)
    target_dir = new_path
    if IS_MAC and new_path.suffix == '.app':
        target_dir = new_path.parent
    if target_dir and not target_dir.exists():
        logger.info(f'create directory: {target_dir}')
        target_dir.mkdir(parents=True, exist_ok=True)
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
    # install_ryujinx_by_version('1.2.78', 'mainline')
    # clear_ryujinx_folder(Path(config.ryujinx.path))
    # install_firmware_to_ryujinx('15.0.0')
    # open_ryujinx_keys_folder()
    detect_ryujinx_version()
    # install_ryujinx_by_version('3.0.1', 'ldn')
    # kill_all_ryujinx_instance(Path(config.ryujinx.path))
    # a = 1
    # print()
