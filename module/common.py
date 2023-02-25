import logging
import os
import subprocess
from pathlib import Path

from utils.network import get_finial_url

logger = logging.getLogger(__name__)


def check_and_install_msvc():
    windir = Path(os.environ['windir'])
    if windir.joinpath(r'System32\msvcp140_atomic_wait.dll').exists():
        logger.info(f'msvc already installed.')
        return
    from module.downloader import download
    from module.msg_notifier import send_notify
    send_notify('开始下载 msvc 安装包...')
    logger.info('downloading msvc installer...')
    download_info = download(get_finial_url('https://aka.ms/vs/17/release/VC_redist.x64.exe'))
    install_file = download_info.files[0]
    send_notify('安装 msvc...')
    logger.info('install msvc...')
    process = subprocess.Popen([install_file.path])
    # process.wait()


if __name__ == '__main__':
    # infos = get_firmware_infos()
    # for info in infos:
    #     print(info)
    # check_and_install_msvc()
    print(check_update())
