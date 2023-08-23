import logging
from typing import Optional
import eel
from config import config, dump_config, shared

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
            return int(version.split('.')[0]) > 70 and _find_edge_win() is not None
    except:
        return False


def _find_edge_win() -> Optional[str]:
    import winreg as reg
    import os
    reg_path = r'SOFTWARE\Microsoft\Windows\CurrentVersion\App Paths\msedge.exe'
    for install_type in reg.HKEY_CURRENT_USER, reg.HKEY_LOCAL_MACHINE:
        try:
            reg_key = reg.OpenKey(install_type, reg_path, 0, reg.KEY_READ)
            edge_path = reg.QueryValue(reg_key, None)
            reg_key.Close()
            if not os.path.isfile(edge_path):
                continue
        except WindowsError:
            edge_path = None
        else:
            break
    return edge_path


def start_edge_in_app_mode(page, port, size=(1280, 720)):
    if port == 0:
        from module.network import get_available_port
        port = get_available_port()
    url = f'http://127.0.0.1:{port}/{page}'
    import subprocess
    try:
        subprocess.Popen(f'"{_find_edge_win()}" --app={url}',
                         stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, stdin=subprocess.DEVNULL, shell=True)
    except Exception as e:
        logger.info(f'Fail to start Edge with full path, fallback with "start" command, exception: {str(e)}')
        subprocess.Popen(f'start msedge --app={url}',
                         stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, stdin=subprocess.DEVNULL, shell=True)
    eel.start(url, port=port, mode=False, size=size)


def import_api_modules():
    import api


def main(port=0, mode=None, dev=False):
    import_api_modules()
    logger.info('eel init starting...')
    # eel.init('vue/public') if dev else eel.init("web")
    eel.init("web")
    shutdown_delay = 114514 if dev else 1
    logger.info('eel init finished.')
    from module.msg_notifier import update_notifier
    default_page = f''
    update_notifier('eel-console')
    if mode is None:
        if can_use_chrome():
            mode = 'chrome'
        elif can_use_edge():
            mode = 'edge'
        else:
            mode = 'user default'
    size = (config.setting.ui.width, config.setting.ui.height)
    logger.info(f'browser mode: {mode}, size: {size}')
    if port == 0:
        from module.network import get_available_port
        port = get_available_port()
        logger.info(f'starting eel at port: {port}')
    if mode == 'edge':
        try:
            shared['mode'] = mode
            start_edge_in_app_mode(default_page, port, size)
        except Exception as e:
            logger.info(f'Fail to start with Edge, fallback to default browser, exception: {str(e)}')
            mode = 'user default'
            config.setting.ui.mode = mode
            dump_config()
            shared['mode'] = mode
            eel.start(default_page, port=port, size=size, mode=mode, shutdown_delay=shutdown_delay)
    else:
        shared['mode'] = mode
        eel.start(default_page, port=port, size=size, mode=mode, shutdown_delay=shutdown_delay)


if __name__ == '__main__':
    import gevent.monkey

    gevent.monkey.patch_ssl()
    gevent.monkey.patch_socket()
    main(8888, False, True)
