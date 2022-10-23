import subprocess
import time
from typing import Optional
import urllib
import aria2p
from pathlib import Path
import os
from module.msg_notifier import send_notify


aria2: Optional[aria2p.API] = None
aria2_process: Optional[subprocess.Popen] = None
download_path = Path(os.path.realpath(os.path.dirname(__file__))).parent.joinpath('download')
aria2_path = Path(os.path.realpath(os.path.dirname(__file__))).joinpath('aria2c.exe')
if not download_path.exists():
    download_path.mkdir()


def is_port_in_use(port: int) -> bool:
    import socket
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
        return s.connect_ex(('localhost', port)) == 0


def init_aria2():
    global aria2
    global aria2_process
    if aria2:
        return
    import random
    while True:
        port = random.randint(20000, 60000)
        if not is_port_in_use(port):
            break
    print(f'starting aria2 daemon at port {port}')
    aria2_process = subprocess.Popen([aria2_path, '--enable-rpc', '--rpc-listen-port', str(port),
                                      '--rpc-secret', '123456'], stdout=subprocess.DEVNULL, stderr=subprocess.STDOUT)
    aria2 = aria2p.API(
        aria2p.Client(
            host="http://localhost",
            port=port,
            secret="123456"
        )
    )
    global_options = {
        'split': '16',
        'max-connection-per-server': '16',
        'min-split-size': '1M',
    }
    proxies = urllib.request.getproxies()
    if proxies:
        global_options['all-proxy'] = iter(proxies.values()).__next__()
    print(f'aria2 global options: {global_options}')
    aria2.set_global_options(global_options)
    import atexit
    atexit.register(shutdown_aria2)


def download(url, save_dir=None, options=None, download_in_background=False):
    init_aria2()
    if options is None:
        options = {}
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
        send_notify(f'下载进度: {info.progress_string()}, 下载速度: {info.download_speed_string()}, '
                    f'{info.completed_length_string()}/{info.total_length_string()}')
        time.sleep(0.3)
        info = aria2.get_download(info.gid)
    if info.error_code != '0':
        if info.error_code == '13':
            print('\rfile already exist.')
        else:
            print(f'\rinfo.error_code: {info.error_code}')
    else:
        print(f'\rprogress: {info.progress_string()}, total size: {info.total_length_string()},')
    send_notify('下载完成')
    aria2.autopurge()
    return info


def shutdown_aria2():
    if aria2_process:
        # print('Shutdown aria2...')
        aria2_process.kill()


if __name__ == '__main__':
    info = download('http://ipv4.download.thinkbroadband.com/200MB.zip')
    os.remove(info.files[0].path)
