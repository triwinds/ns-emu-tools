import logging
import eel
from config import config
from utils.network import get_available_port

logger = logging.getLogger(__name__)


def can_use_chrome():
    """ Identify if Chrome is available for Eel to use """
    import os
    from eel import chrome
    chrome_instance_path = chrome.find_path()
    return chrome_instance_path is not None and os.path.exists(chrome_instance_path)


def can_use_edge():
    try:
        import winreg
        key = winreg.OpenKey(winreg.HKEY_CURRENT_USER, r'Software\Microsoft\Edge\BLBeacon', 0, winreg.KEY_READ)
        with key:
            version: str = winreg.QueryValueEx(key, 'version')[0]
            logger.info(f'Edge version: {version}')
            return int(version.split('.')[0]) > 70
    except:
        return False


def start_edge_in_app_mode(page, port, size=(1280, 720)):
    if port == 0:
        from utils.network import get_available_port
        port = get_available_port()
    url = f'http://localhost:{port}/{page}'
    import subprocess
    import sys
    subprocess.Popen(f'start msedge --app={url}',
                     stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, stdin=subprocess.DEVNULL, shell=True)
    eel.start(url, port=port, mode=False, size=size)


def import_api_modules():
    import api


def main(port=0, mode=None, dev=False):
    import_api_modules()
    logger.info('eel init starting...')
    eel.init('vue/public') if dev else eel.init("web")
    shutdown_delay = 114514 if dev else 1
    logger.info('eel init finished.')
    from module.msg_notifier import update_notifier
    default_page = f'index.html'
    update_notifier('eel-console')
    if mode is None:
        if can_use_chrome():
            mode = 'chrome'
        elif can_use_edge():
            mode = 'edge'
        else:
            mode = 'user default'
    size = (1440, 900)
    logger.info(f'browser mode: {mode}')
    if port == 0:
        port = get_available_port()
        logger.info(f'starting eel at port: {port}')
    if mode == 'edge':
        start_edge_in_app_mode(default_page, port, size)
    else:
        eel.start(default_page, port=port, size=size, mode=mode, shutdown_delay=shutdown_delay)


if __name__ == '__main__':
    import gevent.monkey

    gevent.monkey.patch_ssl()
    gevent.monkey.patch_socket()
    main(8888, False, True)
    # main(0, 'edge')
