import subprocess
import time
from typing import Optional
import logging
import os
import platform
import shutil
import urllib.parse

import aria2p
import requests
from pathlib import Path
from module.msg_notifier import send_notify
from config import config
from module.network import get_available_port, get_global_options, init_download_options_with_proxy
from exception.download_exception import *
from tqdm import tqdm

IS_WINDOWS = platform.system() == 'Windows'

aria2: Optional[aria2p.API] = None
aria2_process: Optional[subprocess.Popen] = None
download_path = Path('./download/')
aria2_path: Optional[str] = None
if not download_path.exists():
    download_path.mkdir()
logger = logging.getLogger(__name__)


def _ensure_aria2_path() -> Optional[str]:
    global aria2_path
    if aria2_path is not None:
        return aria2_path
    if IS_WINDOWS:
        bundled = Path(os.path.realpath(os.path.dirname(__file__))).joinpath('aria2c.exe')
        if bundled.exists():
            aria2_path = str(bundled)
        else:
            fallback = shutil.which('aria2c')
            if fallback:
                aria2_path = fallback
    else:
        aria2_path = shutil.which('aria2c')
    if aria2_path is None:
        logger.warning('aria2c executable not found. Falling back to simple downloader.')
        aria2_path = ''
    return aria2_path or None


class MyTqdm(tqdm):
    download_info: aria2p.Download
    def __init__(self, download_info: aria2p.Download, **kargs):
        self.download_info = download_info
        super().__init__(**kargs)
        self.total = download_info.total_length
        self.ncols = 25
        self.ascii = '.oO'
        self.bar_format = '{l_bar}{bar}'

    def display(self, msg=None, pos=None):
        d = self.format_dict
        di = self.download_info
        msg = self.format_meter(**d)
        msg += (f'|{di.completed_length_string()}/{di.total_length_string()} '
                f'[{di.eta_string()}, {di.download_speed_string()}]')
        print('\r' + msg, end='' if pos != 0 else '\n')
        send_notify('^' + msg)

    def update_process(self, download_info: aria2p.Download):
        self.download_info = download_info
        self.n = download_info.completed_length
        self.total = download_info.total_length
        self.refresh()


def _init_aria2():
    global aria2
    global aria2_process
    if aria2:
        return
    executable = _ensure_aria2_path()
    if not executable:
        raise RuntimeError('aria2 executable not available')
    port = get_available_port()
    send_notify(f'starting aria2 daemon at port {port}')
    logger.info(f'starting aria2 daemon at port {port}')
    if config.setting.download.removeOldAria2LogFile and os.path.exists('aria2.log'):
        try:
            logger.info('removing old aria2 logs.')
            os.remove('aria2.log')
        except:
            pass
    startupinfo = None
    if IS_WINDOWS:
        st_inf = subprocess.STARTUPINFO()
        st_inf.dwFlags = st_inf.dwFlags | subprocess.STARTF_USESHOWWINDOW
        startupinfo = st_inf
    cli = [executable, '--enable-rpc', '--rpc-listen-port', str(port), '--async-dns=true',
           '--rpc-secret', '123456', '--log', 'aria2.log', '--log-level=info', f'--stop-with-process={os.getpid()}']
    if config.setting.download.disableAria2Ipv6:
        cli.append('--disable-ipv6=true')
        if config.setting.network.useDoh:
            cli.append('--async-dns-server=223.5.5.5,119.29.29.29')
    elif config.setting.network.useDoh:
        cli.append('--async-dns-server=2400:3200::1,2402:4e00::,223.5.5.5,119.29.29.29')
    logger.info(f'aria2 cli: {cli}')
    aria2_process = subprocess.Popen(cli, stdout=subprocess.DEVNULL, stderr=subprocess.STDOUT, startupinfo=startupinfo)
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


def init_aria2():
    global aria2
    global aria2_process
    ex = None
    for _ in range(2):
        try:
            _init_aria2()
            return
        except Exception as e:
            ex = e
            logger.info(f'Fail in start aria2 daemon, trying to restart...')
            aria2 = aria2_process = None
    raise ex


def stop_download():
    if not aria2:
        return False
    return aria2.remove_all()


def pause_download():
    if not aria2:
        return False
    return aria2.pause_all(force=True)


def download(url, save_dir=None, options=None, download_in_background=False):
    origin_no_proxy = os.environ.get('no_proxy')
    os.environ['no_proxy'] = '127.0.0.1,localhost'
    try:
        resolved = _ensure_aria2_path()
        if aria2 or resolved:
            return _download(url, save_dir, options, download_in_background)
        if download_in_background:
            raise RuntimeError('download_in_background is not supported without aria2')
        return _simple_download(url, save_dir, options)
    finally:
        if origin_no_proxy is None:
            del os.environ['no_proxy']
        else:
            os.environ['no_proxy'] = origin_no_proxy


def _download(url, save_dir=None, options=None, download_in_background=False):
    init_aria2()
    send_notify('如果遇到下载失败或卡住的问题, 可以尝试在设置中换个下载源, 如果还是不行就挂个梯子')
    send_notify('如果你的网络支持 IPv6, 也可以尝试在设置中允许 aria2 使用 IPv6, 看看能不能解决问题')
    tmp = init_download_options_with_proxy(url)
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
    pbar = MyTqdm(info)
    while info.is_active:
        pbar.update_process(info)
        time.sleep(0.3)
        try:
            info = aria2.get_download(info.gid)
        except Exception as e:
            retry_count += 1
            if retry_count > 15:
                raise e
    pbar.update_process(info)
    pbar.close()
    if info.is_paused:
        raise DownloadPaused()
    if info.error_code != '0':
        if info.error_code == '13':
            logger.info('file already exist.')
            send_notify('文件已存在, 跳过下载.')
            return info
        elif info.error_code == '31':
            if not info.is_complete:
                logger.info(f'remove downloading files due to download interrupted.')
                for file in info.files:
                    if file.path.exists() and file.path.is_file():
                        logger.debug(f'remove file: {file.path}')
                        os.remove(file.path)
            raise DownloadInterrupted()
        else:
            logger.info(f'info.error_code: {info.error_code}, error message: {info.error_message}')
            raise RuntimeError(f'下载出错, error_code: {info.error_code}, error message: {info.error_message}')
    else:
        logger.info(f'progress: {info.progress_string()}, total size: {info.total_length_string()}')
    if not info.is_complete:
        raise DownloadNotCompleted(info.name, info.status)
    send_notify('下载完成')
    aria2.purge()
    return info


class SimpleDownloadFile:
    def __init__(self, path: Path):
        self.path = Path(path)


class SimpleDownloadInfo:
    def __init__(self, path: Path):
        self.files = [SimpleDownloadFile(path)]
        self.is_complete = True
        self.error_code = '0'
        self.error_message = ''
        self.status = 'complete'
        self.name = Path(path).name


def _resolve_simple_filename(url: str, response: requests.Response, options) -> str:
    if isinstance(options, dict):
        specified = options.get('out')
        if specified:
            return specified
    cd = response.headers.get('content-disposition')
    filename = None
    if cd:
        if 'filename*=' in cd:
            filename = cd.split('filename*=', 1)[1].split(';')[0]
            if "''" in filename:
                filename = filename.split("''", 1)[1]
            filename = urllib.parse.unquote(filename.strip().strip('"\''))
        if not filename and 'filename=' in cd:
            filename = cd.split('filename=', 1)[1].split(';')[0]
            filename = urllib.parse.unquote(filename.strip().strip('"\''))
    if not filename:
        path_part = urllib.parse.urlparse(url).path
        filename = os.path.basename(path_part)
        filename = urllib.parse.unquote(filename)
    if not filename:
        filename = f'download_{int(time.time() * 1000)}'
    return filename


def _simple_download(url, save_dir=None, options=None):
    target_dir = Path(save_dir) if save_dir else download_path
    target_dir = Path(target_dir)
    target_dir.mkdir(parents=True, exist_ok=True)
    file_path: Optional[Path] = None
    send_notify('aria2 不可用，使用内置下载器下载文件...')
    try:
        with requests.get(url, stream=True, timeout=30) as response:
            response.raise_for_status()
            filename = _resolve_simple_filename(url, response, options)
            file_path = target_dir.joinpath(filename)
            if file_path.exists():
                file_path.unlink()
            chunk_size = 1024 * 1024
            send_notify(f'正在下载 {filename} ...')
            with file_path.open('wb') as f:
                for chunk in response.iter_content(chunk_size=chunk_size):
                    if not chunk:
                        continue
                    f.write(chunk)
        send_notify('下载完成')
        return SimpleDownloadInfo(file_path)
    except Exception as exc:
        logger.error('Fail to download file via simple downloader', exc_info=True)
        if file_path and file_path.exists():
            file_path.unlink()
        raise RuntimeError(f'下载失败: {exc}') from exc


if __name__ == '__main__':
    info = download('http://ipv4.download.thinkbroadband.com/200MB.zip')
    os.remove(info.files[0].path)
