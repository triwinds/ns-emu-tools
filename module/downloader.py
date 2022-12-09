import subprocess
import time
from typing import Optional
import logging
import aria2p
from pathlib import Path
import os
from module.msg_notifier import send_notify
from config import config
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
    cli = [aria2_path, '--enable-rpc', '--rpc-listen-port', str(port),
           '--rpc-secret', '123456', '--log', 'aria2.log', '--log-level=info']
    if config.setting.download.disableAria2Ipv6:
        cli.append('--disable-ipv6=true')
    logger.info(f'aria2 cli: {cli}')
    aria2_process = subprocess.Popen(cli, stdout=subprocess.DEVNULL, stderr=subprocess.STDOUT, startupinfo=st_inf)
    aria2 = aria2p.API(
        aria2p.Client(
            host="http://127.0.0.1",
            port=port,
            secret="123456",
            timeout=10
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
    tmp['auto-file-renaming'] = 'false'
    tmp['allow-overwrite'] = 'false'
    if options is not None:
        tmp.update(options)
    options = tmp
    if save_dir is not None:
        options['dir'] = save_dir
    else:
        options['dir'] = str(download_path)
    info = aria2.add_uris([url], options=options)
    if download_in_background:
        return info
    info = aria2.get_download(info.gid)
    retry_count = 0
    while info.is_active:
        print(f'\rprogress: {info.progress_string()}, '
                    f'connections: {info.connections}, '
                    f'{info.completed_length_string()}/{info.total_length_string()} , '
                    f'download speed: {info.download_speed_string()}, eta: {info.eta_string()}', end='')
        send_notify(f'下载速度: {info.download_speed_string()}, '
                    f'{info.completed_length_string()}/{info.total_length_string()}')
        time.sleep(0.3)
        try:
            info = aria2.get_download(info.gid)
        except Exception as e:
            retry_count += 1
            if retry_count > 15:
                raise e
    print('\r')
    if info.error_code != '0':
        if info.error_code == '13':
            logger.info('file already exist.')
            send_notify('文件已存在, 跳过下载.')
        else:
            logger.error(f'info.error_code: {info.error_code}, error message: {info.error_message}')
            raise RuntimeError(f'下载出错, error_code: {info.error_code}, error message: {info.error_message}')
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
