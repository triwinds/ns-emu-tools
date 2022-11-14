import subprocess
import time
from typing import Optional
import logging
import aria2p
from pathlib import Path
import os
from module.msg_notifier import send_notify
from utils.network import get_available_port, get_global_options, init_download_options_with_proxy

aria2: Optional[aria2p.API] = None
aria2_process: Optional[subprocess.Popen] = None
download_path = Path('./download/')
aria2_path = Path(os.path.realpath(os.path.dirname(__file__))).joinpath('aria2c.exe')
if not download_path.exists():
    download_path.mkdir()
logger = logging.getLogger(__name__)


def init_aria2():
    global aria2
    global aria2_process
    if aria2:
        return
    port = get_available_port()
    send_notify(f'starting aria2 daemon at port {port}')
    logger.info(f'starting aria2 daemon at port {port}')
    st_inf = subprocess.STARTUPINFO()
    st_inf.dwFlags = st_inf.dwFlags | subprocess.STARTF_USESHOWWINDOW
    aria2_process = subprocess.Popen([aria2_path, '--enable-rpc', '--rpc-listen-port', str(port), '--disable-ipv6=true',
                                      '--rpc-secret', '123456', '--log', 'aria2.log', '--log-level', 'info'],
                                     stdout=subprocess.DEVNULL, stderr=subprocess.STDOUT, startupinfo=st_inf)
    aria2 = aria2p.API(
        aria2p.Client(
            host="http://localhost",
            port=port,
            secret="123456",
            timeout=0.1
        )
    )
    global_options = get_global_options()
    logger.info(f'aria2 global options: {global_options}')
    aria2.set_global_options(global_options)
    import atexit
    atexit.register(shutdown_aria2)


def download(url, save_dir=None, options=None, download_in_background=False):
    init_aria2()
    tmp = init_download_options_with_proxy()
    if options is not None:
        tmp.update(options)
    options = tmp
    if save_dir is not None:
        options['dir'] = save_dir
    else:
        options['dir'] = str(download_path)
    options['auto-file-renaming'] = 'false'
    options['allow-overwrite'] = 'false'
    info = aria2.add_uris([url], options=options)
    if download_in_background:
        return info
    info = aria2.get_download(info.gid)
    while info.is_active:
        print(f'\rprogress: {info.progress_string()}, '
                    f'connections: {info.connections}, '
                    f'{info.completed_length_string()}/{info.total_length_string()} , '
                    f'download speed: {info.download_speed_string()}, eta: {info.eta_string()}', end='')
        send_notify(f'下载速度: {info.download_speed_string()}, '
                    f'{info.completed_length_string()}/{info.total_length_string()}')
        time.sleep(0.3)
        info = aria2.get_download(info.gid)
    print('\r')
    if info.error_code != '0':
        if info.error_code == '13':
            logger.info('file already exist.')
        else:
            logger.info(f'info.error_code: {info.error_code}')
    else:
        logger.info(f'progress: {info.progress_string()}, total size: {info.total_length_string()}')
    send_notify('下载完成')
    aria2.autopurge()
    return info


def shutdown_aria2():
    if aria2_process:
        # logger.info('Shutdown aria2...')
        aria2_process.kill()


if __name__ == '__main__':
    info = download('http://ipv4.download.thinkbroadband.com/200MB.zip')
    os.remove(info.files[0].path)
