import logging
import os
import subprocess
from pathlib import Path
from module.msg_notifier import send_notify
from module.network import get_finial_url

logger = logging.getLogger(__name__)


def check_and_install_msvc():
    windir = Path(os.environ['windir'])
    if windir.joinpath(r'System32\msvcp140_atomic_wait.dll').exists():
        from utils.common import find_installed_software, is_newer_version
        software_list = find_installed_software(r'Microsoft Visual C\+\+ .+ Redistributable')
        if not software_list:
            logger.info(f'msvc already installed, but version not found in registry.')
            return
        logger.debug(f'Installed msvc: {software_list}')
        if not any(is_newer_version('14.38', s['version']) for s in software_list):
            logger.info(f'show update msvc notification.')
            send_notify('如果在启动模拟器时提示 [无法定位程序输入点]，可以试试更新你的 msvc')
            send_notify('下载链接：https://aka.ms/vs/17/release/VC_redist.x64.exe')
        return
    from module.downloader import download
    send_notify('开始下载 msvc 安装包...')
    logger.info('downloading msvc installer...')
    download_info = download(get_finial_url('https://aka.ms/vs/17/release/VC_redist.x64.exe'))
    install_file = download_info.files[0]
    send_notify('安装 msvc...')
    logger.info('install msvc...')
    process = subprocess.Popen([install_file.path])
    # process.wait()


def delete_path(path: str):
    import shutil
    path = Path(path)
    logger.info(f'delete_path: {str(path)}')
    if not path.exists():
        send_notify(f'{str(path)} 不存在')
        return
    if path.is_dir():
        logging.info(f'delete folder: {str(path)}')
        send_notify(f'正在删除 {str(path)} 目录...')
        shutil.rmtree(path, ignore_errors=True)
    elif path.is_file():
        logging.info(f'delete file: {str(path)}')
        send_notify(f'正在删除 {str(path)} 文件...')
        os.remove(path)
    logger.info(f'delete_path done: {str(path)}')
    send_notify(f'{str(path)} 删除完成')


if __name__ == '__main__':
    # infos = get_firmware_infos()
    # for info in infos:
    #     print(info)
    # check_and_install_msvc()
    print(check_update())
