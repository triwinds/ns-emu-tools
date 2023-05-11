import logging
import eel
import webview
from utils.webview2 import ensure_runtime_components
from config import config

logger = logging.getLogger(__name__)
default_page = f'index.html'
port = 0


def import_api_modules():
    import api


def start_eel():
    eel.start(default_page, port=port, mode=False)


def close_all_windows():
    if webview.windows:
        logger.info('Closing all windows...')
        for win in webview.windows:
            win.destroy()


def main():
    if ensure_runtime_components():
        return
    global port
    import_api_modules()
    logger.info('eel init starting...')
    eel.init('vue/public') if port else eel.init("web")
    logger.info('eel init finished.')
    from module.msg_notifier import update_notifier
    update_notifier('eel-console')
    if port == 0:
        from module.network import get_available_port
        port = get_available_port()
    url = f'http://localhost:{port}/{default_page}'
    logger.info(f'start webview with url: {url}')
    webview.create_window('NS EMU TOOLS', url, width=config.setting.ui.width,
                          height=config.setting.ui.height, text_select=True)
    webview.start(func=start_eel)


if __name__ == '__main__':
    import gevent.monkey

    gevent.monkey.patch_ssl()
    gevent.monkey.patch_socket()
    main()
