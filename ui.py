import logging
import gevent.monkey
gevent.monkey.patch_all(httplib=True, subprocess=False)
import eel
from config import config


logger = logging.getLogger(__name__)


def can_use_chrome():
    """ Identify if Chrome is available for Eel to use """
    import os
    from eel import chrome
    chrome_instance_path = chrome.find_path()
    return chrome_instance_path is not None and os.path.exists(chrome_instance_path)


def can_use_edge():
    from eel import edge
    return edge.find_path()


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
    import api.yuzu_api
    import api.common_api
    import api.ryujinx_api
    import api.cheats_api


def main(port=0, mode=None):
    import_api_modules()
    logger.info('eel init starting...')
    eel.init('vue/src') if port else eel.init("web")
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
    size = (1440, 850)
    if mode == 'edge':
        start_edge_in_app_mode(default_page, port, size)
    else:
        eel.start(default_page, port=port, size=size, mode=mode)


if __name__ == '__main__':
    main(8888, False)
    # main(0, 'edge')
