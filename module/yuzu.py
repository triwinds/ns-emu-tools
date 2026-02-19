import logging
import os
import platform
import plistlib
import shutil
import subprocess
import tempfile
import time
from pathlib import Path
from typing import Optional

from config import config, dump_config, YuzuConfig
from storage import storage, add_yuzu_history
from module.downloader import download
from module.msg_notifier import send_notify
from repository.yuzu import get_yuzu_release_info_by_version, get_all_yuzu_release_versions, get_latest_change_log
from module.network import get_github_download_url
from utils.common import decode_yuzu_path, find_all_instances, kill_all_instances
from exception.common_exception import VersionNotFoundException, IgnoredException


logger = logging.getLogger(__name__)

IS_WINDOWS = platform.system() == 'Windows'
IS_MAC = platform.system() == 'Darwin'

detect_exe_list = [r'yuzu.exe', r'eden.exe', r'citron.exe', r'suzu.exe', 'cemu.exe']
download_available_branch = ['citron', 'eden']


def _is_yuzu_app_candidate(app_path: Path) -> bool:
    name = app_path.name.lower()
    return name.endswith('.app') and any(keyword in name for keyword in ('yuzu', 'eden', 'citron', 'suyu'))


def _find_yuzu_app_in_directory(base: Path) -> Optional[Path]:
    base = Path(base)
    if base.suffix == '.app' and base.exists() and _is_yuzu_app_candidate(base):
        return base
    if not base.exists():
        return None
    direct = sorted([p for p in base.glob('*.app') if _is_yuzu_app_candidate(p)])
    if direct:
        return direct[0]
    deep = [p for p in base.rglob('*.app') if _is_yuzu_app_candidate(p)]
    if deep:
        return sorted(deep)[0]
    return None


def _install_yuzu_on_mac(temp_dir: Path, target_path: Path) -> Path:
    app_bundle = _find_yuzu_app_in_directory(temp_dir)
    if not app_bundle:
        raise IgnoredException('未能在安装包中找到 Yuzu/Eden/Citron.app')
    target_path = Path(target_path)
    if target_path.suffix == '.app':
        destination = target_path
        destination.parent.mkdir(parents=True, exist_ok=True)
    else:
        target_path.mkdir(parents=True, exist_ok=True)
        destination = target_path.joinpath(app_bundle.name)
    if destination.exists():
        shutil.rmtree(destination, ignore_errors=True)
    logger.info(f'Copying {app_bundle.name} to {destination}')
    send_notify(f'安装 {get_emu_name()} 文件至目录...')
    shutil.copytree(app_bundle, destination)
    return destination


def _get_installed_yuzu_app() -> Optional[Path]:
    return _find_yuzu_app_in_directory(Path(config.yuzu.yuzu_path))


def _get_yuzu_mac_binary() -> Optional[Path]:
    app_bundle = _get_installed_yuzu_app()
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


def _resolve_asset_download_url(asset) -> str:
    url = asset.download_url
    if 'github.com' in url:
        return get_github_download_url(url)
    return url


def get_emu_name():
    if config.yuzu.branch == 'eden':
        return 'Eden'
    elif config.yuzu.branch == 'citron':
        return 'Citron'
    return 'Yuzu'


def download_yuzu(target_version, branch):
    if branch not in download_available_branch:
        raise IgnoredException(f'Only support install yuzu on branch {download_available_branch}')
    send_notify(f'正在获取 {get_emu_name()} 版本信息...')
    release_info = get_yuzu_release_info_by_version(target_version, branch)
    if not release_info.tag_name:
        raise VersionNotFoundException(target_version, branch, 'yuzu')
    logger.info(f'target {get_emu_name()} version: {target_version}')
    yuzu_path = Path(config.yuzu.yuzu_path)
    logger.info(f'target yuzu path: {yuzu_path}')
    send_notify(f'开始下载 {get_emu_name()}...')
    assets = release_info.assets
    url = None
    for asset in assets:
        name = asset.name.lower()
        if IS_WINDOWS:
            if asset.name.endswith('.7z'):
                url = _resolve_asset_download_url(asset)
                break
            elif name.startswith('windows-yuzu-ea-') and name.endswith('.zip'):
                url = _resolve_asset_download_url(asset)
                break
            elif name.startswith('eden-windows-') and name.endswith('.zip'):
                url = _resolve_asset_download_url(asset)
                break
            elif 'windows' in name and name.endswith('.zip'):
                url = _resolve_asset_download_url(asset)
                break
        elif IS_MAC:
            if any(keyword in name for keyword in ('macos', 'mac', 'osx')) \
                    and (name.endswith('.zip') or name.endswith('.tar.gz')):
                url = _resolve_asset_download_url(asset)
                break
        else:
            if ('linux' in name or 'ubuntu' in name) and (name.endswith('.tar.gz') or name.endswith('.zip')):
                url = _resolve_asset_download_url(asset)
                break
    if not url:
        for asset in assets:
            name = asset.name.lower()
            if name.endswith(('.zip', '.7z', '.tar.gz')):
                url = _resolve_asset_download_url(asset)
                break
    if not url:
        raise IgnoredException('Fail to fetch yuzu download url.')
    logger.info(f"downloading {get_emu_name()} from {url}")
    info = download(url)
    file = info.files[0]
    return file.path


def unzip_yuzu(package_path: Path, target_dir=None):
    logger.info(f'Unpacking yuzu files...')
    send_notify(f'正在解压 {get_emu_name()} 文件...')
    from utils.package import uncompress
    if target_dir is None:
        target_dir = tempfile.gettempdir()
    uncompress(package_path, target_dir)
    return target_dir


def install_eden(target_version: str):
    yuzu_path = Path(config.yuzu.yuzu_path)
    yuzu_package_path = download_yuzu(target_version, 'eden')
    with tempfile.TemporaryDirectory(prefix='eden-install-') as tmp_dir:
        tmp_path = Path(tmp_dir)
        unzip_yuzu(yuzu_package_path, tmp_path)
        if IS_MAC:
            destination = _install_yuzu_on_mac(tmp_path, yuzu_path)
            logger.info(f'Eden app installed at {destination}')
        else:
            copy_back_yuzu_files(tmp_path, yuzu_path)
    logger.info(f'Eden of [{target_version}] install successfully.')
    if config.setting.download.autoDeleteAfterInstall:
        os.remove(yuzu_package_path)


def install_citron(target_version: str):
    yuzu_path = Path(config.yuzu.yuzu_path)
    yuzu_package_path = download_yuzu(target_version, 'citron')
    with tempfile.TemporaryDirectory(prefix='citron-install-') as tmp_dir:
        tmp_path = Path(tmp_dir)
        unzip_yuzu(yuzu_package_path, tmp_path)
        if IS_MAC:
            destination = _install_yuzu_on_mac(tmp_path, yuzu_path)
            logger.info(f'Citron app installed at {destination}')
        else:
            release_dirs = [p for p in tmp_path.iterdir() if p.is_dir()]
            release_dir = release_dirs[0] if release_dirs else tmp_path
            copy_back_yuzu_files(release_dir, yuzu_path)
    logger.info(f'Citron of [{target_version}] install successfully.')
    if config.setting.download.autoDeleteAfterInstall:
        os.remove(yuzu_package_path)


def copy_back_yuzu_files(tmp_dir: Path, yuzu_path: Path, ):
    for useless_file in tmp_dir.glob('yuzu-windows-msvc-source-*.tar.xz'):
        os.remove(useless_file)
    logger.info(f'Copy back {get_emu_name()} files...')
    send_notify(f'安装 {get_emu_name()} 文件至目录...')
    try:
        shutil.copytree(tmp_dir, yuzu_path, dirs_exist_ok=True)
        time.sleep(0.5)
    except Exception as e:
        logger.exception(e)
        from exception.install_exception import FailToCopyFiles
        raise FailToCopyFiles(e, f'{get_emu_name()} 文件复制失败')


def remove_all_executable_file():
    send_notify('正在删除旧模拟器的可执行文件...')
    logger.info('remove all executable file...')
    yz_path = Path(config.yuzu.yuzu_path)
    if IS_MAC:
        candidates = []
        if yz_path.suffix == '.app':
            candidates.append(yz_path)
        if yz_path.exists() and yz_path.is_dir():
            candidates.extend([p for p in yz_path.glob('*.app') if _is_yuzu_app_candidate(p)])
        removed = set()
        for app_bundle in candidates:
            app_bundle = app_bundle.resolve()
            if app_bundle in removed or not app_bundle.exists():
                continue
            removed.add(app_bundle)
            logger.info(f'remove {app_bundle}')
            if app_bundle.is_dir():
                shutil.rmtree(app_bundle, ignore_errors=True)
            else:
                app_bundle.unlink(missing_ok=True)  # type: ignore[arg-type]
        return
    for exe_name in detect_exe_list:
        exe_path = yz_path.joinpath(exe_name)
        if exe_path.exists():
            logger.info(f'remove {exe_path}')
            exe_path.unlink()


def install_yuzu(target_version, branch='ea'):
    if target_version == config.yuzu.yuzu_version:
        logger.info(f'Current {get_emu_name()} version is same as target version [{target_version}], skip install.')
        send_notify(f'当前就是 [{target_version}] 版本的 {get_emu_name()} , 跳过安装.')
        return
    remove_all_executable_file()
    if branch == 'eden':
        install_eden(target_version)
    elif branch == 'citron':
        install_citron(target_version)
    else:
        raise IgnoredException('Only support install yuzu on branch [eden/citron]')
    yuzu_path = Path(config.yuzu.yuzu_path)
    if IS_WINDOWS:
        yuzu_path.mkdir(parents=True, exist_ok=True)
    else:
        (yuzu_path if yuzu_path.suffix != '.app' else yuzu_path.parent).mkdir(parents=True, exist_ok=True)
    exe_path = get_yuzu_exe_path()
    if IS_WINDOWS and exe_path and exe_path.exists() and config.setting.other.rename_yuzu_to_cemu:
        target_path = yuzu_path.joinpath('cemu.exe')
        os.replace(exe_path, target_path)
        logger.info(f'Rename {exe_path.name} to {target_path}')
        send_notify(f'重命名 {exe_path.name}  为 {target_path}')
    config.yuzu.yuzu_version = target_version
    config.yuzu.branch = branch
    dump_config()
    from module.common import check_and_install_msvc
    check_and_install_msvc()
    send_notify(f'{get_emu_name()} [{target_version}] 安装成功.')


def install_firmware_to_yuzu(firmware_version=None):
    if firmware_version == config.yuzu.yuzu_firmware:
        logger.info(f'Current firmware are same as target version [{firmware_version}], skip install.')
        send_notify(f'当前的 固件 就是 [{firmware_version}], 跳过安装.')
        return
    from module.firmware import install_firmware
    firmware_path = get_yuzu_nand_path().joinpath('system', 'Contents', 'registered')
    new_version = install_firmware(firmware_version, firmware_path)
    if new_version:
        config.yuzu.yuzu_firmware = new_version
        dump_config()
        send_notify(f'固件已安装至 {str(firmware_path)}')
        send_notify(f'固件 [{firmware_version}] 安装成功，请安装相应的 key 至 {get_emu_name()}.')


def get_yuzu_exe_path():
    if IS_MAC:
        return _get_yuzu_mac_binary()
    yz_path = Path(config.yuzu.yuzu_path)
    if ((config.setting.other.rename_yuzu_to_cemu or not yz_path.joinpath('yuzu.exe').exists())
            and yz_path.joinpath('cemu.exe').exists()):
        return yz_path.joinpath('cemu.exe')
    for exe_name in detect_exe_list:
        if yz_path.joinpath(exe_name).exists():
            return yz_path.joinpath(exe_name)
    return yz_path.joinpath('yuzu.exe')


def detect_yuzu_version():
    send_notify(f'正在检测 {get_emu_name()} 版本...')
    if IS_MAC:
        app_bundle = _get_installed_yuzu_app()
        if not app_bundle or not app_bundle.exists():
            send_notify(f'未能找到 {get_emu_name()} 程序')
            config.yuzu.yuzu_version = None
            dump_config()
            return None
        version = _read_macos_app_version(app_bundle)
        if version:
            send_notify(f'当前 {get_emu_name()} 版本 [{version}]')
            logger.info(f'Current {get_emu_name()} version: {version}')
        else:
            send_notify(f'未能从 {get_emu_name()} 应用信息中读取版本号')
            logger.warning(f'Info.plist does not contain version info for {app_bundle}')
        name_lower = app_bundle.stem.lower()
        if 'eden' in name_lower:
            config.yuzu.branch = 'eden'
        elif 'citron' in name_lower:
            config.yuzu.branch = 'citron'
        elif config.yuzu.branch not in download_available_branch:
            config.yuzu.branch = 'eden'
        config.yuzu.yuzu_version = version
        dump_config()
        return version
    yz_path = get_yuzu_exe_path()
    if not yz_path or not yz_path.exists():
        send_notify(f'未能找到 {get_emu_name()} 程序')
        config.yuzu.yuzu_version = None
        dump_config()
        return None
    instances = find_all_instances(yz_path.name)
    if instances:
        logger.info(f'Yuzu pid={[p.pid for p in instances]} is running.')
        send_notify(f'yuzu 正在运行中, 请先关闭之.')
        return None
    send_notify(f'正在启动 {get_emu_name()} ...')
    subprocess.Popen([yz_path.absolute()])
    version = None
    branch = None
    try:
        try_cnt = 0
        from utils.common import get_all_window_name
        while try_cnt < 30 and not version:
            time.sleep(0.5)
            for window_name in get_all_window_name():
                if window_name.startswith('yuzu '):
                    logger.info(f'yuzu window name: {window_name}')
                    if window_name.startswith('yuzu Early Access '):
                        version = window_name[18:]
                        branch = 'ea'
                    else:
                        version = window_name[5:]
                        branch = 'mainline'
                    send_notify(f'当前 yuzu 版本 [{version}]')
                    logger.info(f'current yuzu version: {version}, branch: {branch}')
                    break
                elif window_name.startswith('Eden | '):
                    version = window_name[7:]
                    branch = 'eden'
                    send_notify(f'当前 Eden 模拟器版本 [{version}]')
                    logger.info(f'current eden version: {version}, branch: {branch}')
                    break
                elif window_name.startswith('citron | '):
                    version = window_name[9:]
                    branch = 'citron'
                    send_notify(f'当前 Citron 模拟器版本 [{version}]')
                    logger.info(f'current citron version: {version}, branch: {branch}')
                    break
            try_cnt += 1
    except Exception:
        logger.exception('error occur in get_all_window_name')
    kill_all_instances(yz_path.name)
    if version:
        config.yuzu.branch = branch
    else:
        send_notify(f'检测失败！没有找到 {get_emu_name()} 窗口...')
    config.yuzu.yuzu_version = version
    dump_config()
    return version


def start_yuzu():
    if IS_MAC:
        app_bundle = _get_installed_yuzu_app()
        if app_bundle and app_bundle.exists():
            logger.info(f'starting {get_emu_name()} from app bundle {app_bundle}')
            subprocess.Popen(['open', '-n', str(app_bundle.resolve())])
            return
        logger.info(f'{get_emu_name()} app bundle not found in [{config.yuzu.yuzu_path}]')
        raise IgnoredException(f'{get_emu_name()} app bundle not found in [{config.yuzu.yuzu_path}]')
    yz_path = get_yuzu_exe_path()
    if yz_path and yz_path.exists():
        logger.info(f'starting yuzu from {yz_path}')
        subprocess.Popen([yz_path])
    else:
        logger.info(f'yuzu not exist in [{yz_path}]')
        raise IgnoredException(f'yuzu not exist in [{yz_path}]')


def get_yuzu_user_path():
    if IS_MAC:
        base = Path.home().joinpath('Library', 'Application Support')
        for name in ('yuzu', 'Eden', 'citron'):
            candidate = base.joinpath(name)
            if candidate.exists():
                return candidate
        default_path = base.joinpath('yuzu')
        default_path.mkdir(parents=True, exist_ok=True)
        return default_path
    yuzu_path = Path(config.yuzu.yuzu_path)
    user_dir = yuzu_path.joinpath('user')
    if user_dir.exists():
        return user_dir
    appdata = os.environ.get('appdata')
    if appdata:
        appdata_path = Path(appdata)
        for name in ('yuzu', 'eden', 'citron'):
            candidate = appdata_path.joinpath(name)
            if candidate.exists():
                return candidate
    return user_dir


def open_yuzu_keys_folder():
    keys_path = get_yuzu_user_path().joinpath('keys')
    keys_path.mkdir(parents=True, exist_ok=True)
    keys_path.joinpath('把prod.keys放当前目录.txt').touch(exist_ok=True)
    logger.info(f'open explorer on path {keys_path}')
    if IS_MAC:
        subprocess.Popen(['open', str(keys_path.resolve())])
    elif IS_WINDOWS:
        subprocess.Popen(f'explorer "{str(keys_path.resolve())}"')
    else:
        subprocess.Popen(['xdg-open', str(keys_path.resolve())])


def _get_yuzu_data_storage_config(user_path: Path):
    config_path = user_path.joinpath('config', 'qt-config.ini')
    if config_path.exists():
        import configparser
        yuzu_qt_config = configparser.ConfigParser()
        yuzu_qt_config.read(str(config_path.absolute()), encoding='utf-8')
        # data = {section: dict(yuzu_qt_config[section]) for section in yuzu_qt_config.sections()}
        # print(data)
        data_storage = yuzu_qt_config['Data%20Storage']
        logger.debug(dict(data_storage))
        return data_storage


def get_yuzu_nand_path():
    user_path = get_yuzu_user_path()
    nand_path = user_path.joinpath('nand')
    try:
        data_storage = _get_yuzu_data_storage_config(user_path)
        if data_storage:
            path_str = data_storage.get('nand_directory')
            if path_str:
                decoded = decode_yuzu_path(path_str) if '\u' in path_str else path_str
                nand_path = Path(decoded).expanduser()
                logger.info(f'use nand path from yuzu config: {nand_path}')
    except Exception as e:
        logger.warning(f'fail in parse yuzu qt-config, error msg: {str(e)}')
    return nand_path


def get_yuzu_load_path():
    user_path = get_yuzu_user_path()
    load_path = user_path.joinpath('load')
    try:
        data_storage = _get_yuzu_data_storage_config(user_path)
        if data_storage:
            path_str = data_storage.get('load_directory')
            if path_str:
                decoded = decode_yuzu_path(path_str) if '\\u' in path_str else path_str
                load_path = Path(decoded).expanduser()
                logger.info(f'use load path from yuzu config: {load_path}')
    except Exception as e:
        logger.warning(f'fail in parse yuzu qt-config, error msg: {str(e)}')
    return load_path


def update_yuzu_path(new_yuzu_path: str):
    new_path = Path(new_yuzu_path)
    target_dir = new_path
    if IS_MAC and new_path.suffix == '.app':
        target_dir = new_path.parent
    if target_dir and not target_dir.exists():
        logger.info(f'create directory: {target_dir}')
        target_dir.mkdir(parents=True, exist_ok=True)
    if new_path.absolute() == Path(config.yuzu.yuzu_path).absolute():
        logger.info(f'No different with old yuzu path, skip update.')
        return
    add_yuzu_history(config.yuzu)
    logger.info(f'setting yuzu path to {new_path}')
    cfg = storage.yuzu_history.get(str(new_path.absolute()), YuzuConfig())
    cfg.yuzu_path = str(new_path.absolute())
    config.yuzu = cfg
    if cfg.yuzu_path not in storage.yuzu_history:
        add_yuzu_history(cfg)
    dump_config()


def get_yuzu_change_logs():
    branch = config.yuzu.branch
    markdown = get_latest_change_log(branch)
    return markdown


if __name__ == '__main__':
    # install_yuzu('1220', 'mainline')
    # install_firmware_to_yuzu()
    # install_key_to_yuzu()
    print(detect_yuzu_version())
    # print(get_yuzu_user_path().joinpath(r'nand\system\Contents\registered'))
    # open_yuzu_keys_folder()
    # print(get_yuzu_load_path())
    # from utils.common import decode_yuzu_path
    # test_str = r'D:/Yuzu/user\'/\x65b0\x5efa\x6587\x4ef6\x5939/'
    # print(decode_yuzu_path(test_str))
    # get_yuzu_commit_logs()
