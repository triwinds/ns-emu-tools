import os
import shutil
import subprocess
from functools import lru_cache
from pathlib import Path
from module.msg_notifier import send_notify
from config import config
import bs4
from utils.network import get_finial_url, session
import logging
from module.downloader import download

logger = logging.getLogger(__name__)


@lru_cache(1)
def get_firmware_infos():
    base_url = 'https://archive.org/download/nintendo-switch-global-firmwares/'
    resp = session.get(get_finial_url(base_url))
    soup = bs4.BeautifulSoup(resp.text, features="html.parser")
    a_tags = soup.select('#maincontent > div > div > pre > table > tbody > tr > td > a')
    archive_versions = []
    for a in a_tags:
        name = a.text
        if name.startswith('Firmware ') and name.endswith('.zip'):
            size = a.parent.next_sibling.next_sibling.next_sibling.next_sibling.text
            version = name[9:-4]
            version_num = 0
            for num in version.split('.'):
                version_num *= 100
                version_num += int(''.join(ch for ch in num if ch.isdigit()))
            archive_versions.append({
                'name': name,
                'version': version,
                'size': size,
                'url': base_url + a.attrs['href'],
                'version_num': version_num,
            })
    archive_versions = sorted(archive_versions, key=lambda x: x['version_num'], reverse=True)
    return archive_versions


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


def check_update(prerelease=False):
    from repository.my_info import get_all_release
    from config import current_version
    release_infos = get_all_release()
    latest_tag_name = None
    if prerelease:
        latest_tag_name = release_infos[0]['tag_name']
    else:
        for ri in release_infos:
            if not ri['prerelease']:
                latest_tag_name = ri['tag_name']
                break
    if not latest_tag_name:
        latest_tag_name = release_infos[0]['tag_name']
    return current_version != latest_tag_name, latest_tag_name


def install_firmware(firmware_version, target_firmware_path):
    send_notify('正在获取固件信息...')
    firmware_infos = get_firmware_infos()
    target_info = None
    if firmware_version:
        firmware_map = {fi['version']: fi for fi in firmware_infos}
        target_info = firmware_map.get(firmware_version)
    if not target_info:
        logger.info(f'Target firmware version [{firmware_version}] not found, skip install.')
        send_notify(f'Target firmware version [{firmware_version}] not found, skip install.')
        return
    url = get_finial_url(target_info['url'])
    send_notify(f'开始下载固件...')
    logger.info(f"downloading firmware of [{firmware_version}] from {url}")
    info = download(url)
    file = info.files[0]
    import zipfile
    with zipfile.ZipFile(file.path, 'r') as zf:
        firmware_path = target_firmware_path
        shutil.rmtree(firmware_path, ignore_errors=True)
        firmware_path.mkdir(parents=True, exist_ok=True)
        send_notify(f'开始解压安装固件...')
        logger.info(f'Unzipping firmware files to {firmware_path}')
        zf.extractall(firmware_path)
        logger.info(f'Firmware of [{firmware_version}] install successfully.')
    if config.setting.download.autoDeleteAfterInstall:
        os.remove(file.path)
    return firmware_version


def download_net_by_tag(tag: str):
    from repository.my_info import get_release_info_by_tag
    from utils.network import get_github_download_url
    import sys
    release_info = get_release_info_by_tag(tag)
    logger.info(f'start download NET release by tag: {tag}, release name: {release_info.get("name")}')
    execute_path = Path(sys.argv[0])
    logger.info(f'execute_path: {execute_path}')
    target_file_name = execute_path.name if execute_path.name == 'NsEmuTools-console.exe' else 'NsEmuTools.exe'
    logger.info(f'target_file_name: {target_file_name}')
    for asset in release_info['assets']:
        if target_file_name == asset['name']:
            logger.info(f'start download {target_file_name}, version: [{tag}]')
            send_notify(f'开始下载 {target_file_name}, 版本: [{tag}]')
            info = download(get_github_download_url(asset['browser_download_url']), options={'allow-overwrite': 'true'})
            filepath = info.files[0].path.absolute()
            logger.info(f'{target_file_name} of [{tag}] downloaded to {filepath}')
            send_notify(f'{target_file_name} 版本: [{tag}] 已下载至')
            send_notify(f'{filepath}')
            return
    logger.warning(f'{target_file_name} not found in release_info')
    send_notify(f'未能在 Release 中找到相应的文件')


if __name__ == '__main__':
    # infos = get_firmware_infos()
    # for info in infos:
    #     print(info)
    # check_and_install_msvc()
    print(check_update())
