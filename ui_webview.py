import logging

import gevent.monkey

gevent.monkey.patch_ssl()
gevent.monkey.patch_socket()
import eel

logger = logging.getLogger(__name__)
default_page = f'index.html'
port = 0


def import_api_modules():
    import api


def start_eel():
    eel.start(default_page, port=port, mode=False)


def main():
    global port
    import_api_modules()
    logger.info('eel init starting...')
    eel.init('vue/src') if port else eel.init("web")
    logger.info('eel init finished.')
    from module.msg_notifier import update_notifier
    update_notifier('eel-console')
    if port == 0:
        from utils.network import get_available_port
        port = get_available_port()
    url = f'http://localhost:{port}/{default_page}'
    logger.info(f'start webview with url: {url}')
    import webview
    webview.create_window('NS EMU TOOLS', url, width=1440, height=850, resizable=False)
    webview.start(func=start_eel)


if __name__ == '__main__':
    main()
