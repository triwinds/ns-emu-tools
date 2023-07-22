import logging
import os
import subprocess
import time
from pathlib import Path
from typing import List

import psutil

from config import config
from module.downloader import download
from module.msg_notifier import send_notify
from module.network import get_github_download_url
from module.hosts import Hosts
from exception.common_exception import IgnoredException

logger = logging.getLogger(__name__)
script_template = """@echo off
cd /d <cfst_path>
CloudflareST.exe -p 0 -url "https://cloudflaremirrors.com/archlinux/images/latest/Arch-Linux-x86_64-basic.qcow2"
"""
target_cfst_version = 'v2.2.2'
version_file = Path('CloudflareSpeedTest/cfst_version')


def download_cfst():
    filepath = Path('download/CloudflareST_windows_amd64.zip')
    if not filepath.exists():
        logger.info('downloading CloudflareSpeedTest...')
        send_notify('开始下载 CloudflareSpeedTest...')
        url = get_github_download_url('https://github.com/XIU2/CloudflareSpeedTest/releases/download'
                                      f'/{target_cfst_version}/CloudflareST_windows_amd64.zip')
        info = download(url, 'CloudflareSpeedTest')
        filepath = info.files[0].path
    import zipfile
    logger.info('unzip CloudflareSpeedTest...')
    send_notify('正在解压 CloudflareSpeedTest...')
    with zipfile.ZipFile(filepath, 'r') as zf:
        zf.extractall('CloudflareSpeedTest')
    os.remove(filepath)
    send_notify('解压完成')
    with version_file.open('w') as f:
        f.write(target_cfst_version)


def run_cfst():
    exe_path = Path('CloudflareSpeedTest/CloudflareST.exe')
    if not exe_path.exists():
        logger.info('CloudflareSpeedTest not exist.')
        send_notify('CloudflareSpeedTest not exist.')
        raise IgnoredException('CloudflareSpeedTest not exist.')
    logger.info('starting CloudflareSpeedTest...')
    send_notify('正在运行 CloudflareSpeedTest...')
    script_path = Path('CloudflareSpeedTest/cfst.bat')
    with open(script_path, 'w') as f:
        f.write(script_template.replace('<cfst_path>', str(exe_path.absolute().parent)))
    subprocess.Popen(f'start cmd /c {str(script_path.absolute())}', shell=True)
    time.sleep(1)
    for p in psutil.process_iter():
        if p.name() == 'CloudflareST.exe':
            p.wait()


def get_fastest_ip_from_result():
    result_path = Path('CloudflareSpeedTest/result.csv')
    if not result_path.exists():
        logger.info('CloudflareSpeedTest result not exist.')
        send_notify('未能检测到 CloudflareSpeedTest 结果, 请先运行一次测试.')
        raise IgnoredException('未能检测到 CloudflareSpeedTest 结果, 请先运行一次测试.')
    with open(result_path, 'r', encoding='utf-8') as f:
        lines = f.readlines()
    if len(lines) < 2:
        logger.info('Fail to parse CloudflareSpeedTest result.')
        send_notify('无法解析 CloudflareSpeedTest 结果, 请先运行一次测试.')
        raise IgnoredException('无法解析 CloudflareSpeedTest 结果, 请先运行一次测试.')
    ip = lines[1].split(',', 1)[0]
    logger.info(f'fastest ip from result: {ip}')
    return ip


def show_result():
    result_path = Path('CloudflareSpeedTest/result.csv')
    if not result_path.exists():
        logger.info('CloudflareSpeedTest result not exist.')
        send_notify('未能检测到 CloudflareSpeedTest 结果, 请先运行一次测试.')
        raise IgnoredException('未能检测到 CloudflareSpeedTest 结果, 请先运行一次测试.')
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
        write_hosts(hosts)
        subprocess.Popen(['ipconfig', '/flushdns'], stdout=subprocess.DEVNULL).wait()
        send_notify('hosts 文件更新完成, 请重启程序使修改生效.')
    except Exception as e:
        logger.error(f'fail in update hosts, exception: {str(e)}')
        send_notify('hosts 文件更新失败, 请使用管理员权限重新启动程序.')


def get_current_cfst_version():
    if not version_file.exists():
        return
    with version_file.open() as f:
        return f.read()


def get_cf_hostnames():
    default_list = ['nsarchive.e6ex.com']
    hostnames = get_override_host_names()
    for hn in default_list:
        if hn not in hostnames:
            hostnames.append(hn)
    return hostnames


def optimize_cloudflare_hosts():
    if target_cfst_version != get_current_cfst_version():
        logger.info(f'cfst version changed, target version: {target_cfst_version}, '
                    f'current version: {get_current_cfst_version()}')
        logger.info(f'removing old cfst...')
        send_notify('CloudflareSpeedTest 版本已更新, 正在切换至新版本')
        import shutil
        shutil.rmtree('CloudflareSpeedTest', ignore_errors=True)
    exe_path = Path('CloudflareSpeedTest/CloudflareST.exe')
    if not exe_path.exists():
        download_cfst()
    run_cfst()
    show_result()
    fastest_ip = get_fastest_ip_from_result()
    install_ip_to_hosts(fastest_ip, get_cf_hostnames())


def remove_cloudflare_hosts():
    try:
        logger.info('removing ip from hosts...')
        send_notify('正在删除 hosts 文件中的相关配置...')
        from module.hosts import Hosts, HostsEntry
        hosts = Hosts()
        hostnames = get_cf_hostnames()
        for hn in hostnames:
            hosts.remove_all_matching(name=hn)
        write_hosts(hosts)
        subprocess.Popen(['ipconfig', '/flushdns'], stdout=subprocess.DEVNULL).wait()
        send_notify('hosts 文件更新完成, 请重启程序使修改生效.')
    except Exception as e:
        logger.error(f'fail in update hosts, exception: {str(e)}')
        send_notify('hosts 文件更新失败, 请使用管理员权限重新启动程序.')


def write_hosts(hosts: Hosts):
    import os
    from utils.admin import check_is_admin
    if check_is_admin():
        hosts.write()
        logger.info(f'updated hosts: {hosts}')
        return
    elif os.name == 'nt':
        from utils.admin import run_with_admin_privilege
        tmp_hosts = str(Path('tmp_hosts').absolute())
        hosts.write(tmp_hosts)
        sys_hosts = str(Path(hosts.determine_hosts_path()).absolute())
        ret = run_with_admin_privilege('cmd', f'/c move "{tmp_hosts}" "{sys_hosts}"')
        if ret == 42:
            logger.info(f'updated hosts: {hosts}')
            return
    raise IgnoredException(f'Unable to write hosts file.')


if __name__ == '__main__':
    # run_cfst()
    optimize_cloudflare_hosts()
    # print(check_is_admin())
    # remove_cloudflare_hosts()
    # install_ip_to_hosts(get_fastest_ip_from_result(), get_override_host_names())
