import logging
import subprocess
from pathlib import Path
from typing import List

from config import config
from module.downloader import download
from module.msg_notifier import send_notify
from utils.network import get_github_download_url

logger = logging.getLogger(__name__)


def download_cfst():
    filepath = Path('download/CloudflareST_windows_amd64.zip')
    if not filepath.exists():
        logger.info('downloading CloudflareSpeedTest...')
        send_notify('开始下载 CloudflareSpeedTest...')
        url = get_github_download_url('https://github.com/XIU2/CloudflareSpeedTest/releases/download'
                                      '/v2.1.0/CloudflareST_windows_amd64.zip')
        info = download(url)
        filepath = info.files[0].path
    import zipfile
    logger.info('unzip CloudflareSpeedTest...')
    send_notify('正在解压 CloudflareSpeedTest...')
    with zipfile.ZipFile(filepath, 'r') as zf:
        zf.extractall('CloudflareSpeedTest')


def run_cfst():
    exe_path = Path('CloudflareSpeedTest/CloudflareST.exe')
    if not exe_path.exists():
        logger.info('CloudflareSpeedTest not exist.')
        send_notify('CloudflareSpeedTest not exist.')
        raise RuntimeError('CloudflareSpeedTest not exist.')
    logger.info('starting CloudflareSpeedTest...')
    send_notify('正在运行 CloudflareSpeedTest...')
    p = subprocess.Popen(['CloudflareSpeedTest/CloudflareST.exe', '-p', '0', '-url',
                          'https://cloudflaremirrors.com/archlinux/images/latest/Arch-Linux-x86_64-basic.qcow2'],
                         cwd='./CloudflareSpeedTest',
                         creationflags=subprocess.CREATE_NEW_CONSOLE)
    p.wait()


def get_fastest_ip_from_result():
    result_path = Path('CloudflareSpeedTest/result.csv')
    if not result_path.exists():
        logger.info('CloudflareSpeedTest result not exist.')
        send_notify('未能检测到 CloudflareSpeedTest 结果, 请先运行一次测试.')
        raise RuntimeError('未能检测到 CloudflareSpeedTest 结果, 请先运行一次测试.')
    with open(result_path, 'r', encoding='utf-8') as f:
        lines = f.readlines()
    if len(lines) < 2:
        logger.info('Fail to parse CloudflareSpeedTest result.')
        send_notify('无法解析 CloudflareSpeedTest 结果, 请先运行一次测试.')
        raise RuntimeError('无法解析 CloudflareSpeedTest 结果, 请先运行一次测试.')
    ip = lines[1].split(',', 1)[0]
    logger.info(f'fastest ip from result: {ip}')
    return ip


def show_result():
    result_path = Path('CloudflareSpeedTest/result.csv')
    if not result_path.exists():
        logger.info('CloudflareSpeedTest result not exist.')
        send_notify('未能检测到 CloudflareSpeedTest 结果, 请先运行一次测试.')
        raise RuntimeError('未能检测到 CloudflareSpeedTest 结果, 请先运行一次测试.')
    with open(result_path, 'r', encoding='utf-8') as f:
        lines = f.readlines()
    send_notify('===============测速结果===============')
    for line in lines:
        send_notify(line)


def get_override_host_names():
    return [s.strip() for s in config.setting.cfst.override_hostnames.split(',')]


def install_ip_to_hosts(ip: str, host_names: List[str]):
    logger.info('writing hosts...')
    send_notify('正在更新 hosts 文件...')
    try:
        from module.hosts import Hosts, HostsEntry
        hosts = Hosts()
        new_entry = HostsEntry(entry_type='ipv4', address=ip, names=host_names)
        logger.info(f'new_entry: {new_entry}')
        send_notify(f'使用 ip: {ip}')
        hosts.add([new_entry], force=True)
        hosts.write()
        subprocess.Popen(['ipconfig', '/flushdns'], stdout=subprocess.DEVNULL).wait()
        logger.info(f'updated hosts: {hosts}')
        send_notify('hosts 文件更新完成, 请重启程序使修改生效.')
    except Exception as e:
        logger.error(f'fail in update hosts, exception: {str(e)}')
        send_notify('hosts 文件更新失败, 请使用管理员权限重新启动程序.')


def optimize_cloudflare_hosts():
    if not check_is_admin():
        raise RuntimeError('更新 hosts 需要管理员权限，请使用管理员权限重新启动程序.')
    exe_path = Path('CloudflareSpeedTest/CloudflareST.exe')
    if not exe_path.exists():
        download_cfst()
    run_cfst()
    show_result()
    fastest_ip = get_fastest_ip_from_result()
    host_names = get_override_host_names()
    if not host_names:
        host_names = ['nsarchive.e6ex.com']
    install_ip_to_hosts(fastest_ip, host_names)


def remove_cloudflare_hosts():
    if not check_is_admin():
        raise RuntimeError('更新 hosts 需要管理员权限，请使用管理员权限重新启动程序.')
    try:
        logger.info('removing ip from hosts...')
        send_notify('正在删除 hosts 文件中的相关配置...')
        from module.hosts import Hosts, HostsEntry
        hosts = Hosts()
        host_names = get_override_host_names()
        for hn in host_names:
            hosts.remove_all_matching(name=hn)
        hosts.write()
        subprocess.Popen(['ipconfig', '/flushdns'], stdout=subprocess.DEVNULL).wait()
        logger.info(f'updated hosts: {hosts}')
        send_notify('hosts 文件更新完成, 请重启程序使修改生效.')
    except Exception as e:
        logger.error(f'fail in update hosts, exception: {str(e)}')
        send_notify('hosts 文件更新失败, 请使用管理员权限重新启动程序.')


def check_is_admin():
    import ctypes
    import os
    try:
        return os.getuid() == 0
    except AttributeError:
        return ctypes.windll.shell32.IsUserAnAdmin() != 0


if __name__ == '__main__':
    # optimize_cloudflare_hosts()
    # print(check_is_admin())
    remove_cloudflare_hosts()
    install_ip_to_hosts(get_fastest_ip_from_result(), get_override_host_names())
