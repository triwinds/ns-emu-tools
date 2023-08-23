import logging
import os

import eel
import webview
from utils.webview2 import ensure_runtime_components
from config import config, shared, dump_config
from threading import Timer

logger = logging.getLogger(__name__)
default_page = f''
port = 0


def import_api_modules():
    import api


def check_webview_status():
    if 'ui_init_time' in shared:
        return
    config.setting.ui.mode = 'browser'
    dump_config()
    from tkinter import messagebox
    messagebox.showerror('未检测到活动的会话', '未能检测到活动的会话，这可能是由于当前系统的 webview2 组件存在问题造成的，'
                                      '已将 ui 切换至浏览器模式，请重新启动程序.')


def maximize_window():
    if os.name != 'nt':
        return
    from webview.platforms.winforms import WinForms, Func, Type, BrowserView

    def _maximize():
        BrowserView.instances['master'].WindowState = WinForms.FormWindowState.Maximized
    BrowserView.instances['master'].Invoke(Func[Type](_maximize))


def get_window_size():
    return webview.windows[0].width, webview.windows[0].height


def post_start(fullscreen):
    Timer(10.0, check_webview_status).start()
    if fullscreen:
        Timer(0.5, maximize_window).start()
    shared['mode'] = 'webview'
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
    url = f'http://127.0.0.1:{port}/{default_page}'
    logger.info(f'start webview with url: {url}')
    width, height = config.setting.ui.width, config.setting.ui.height
    sw, sh = webview.screens[0].width, webview.screens[0].height
    fullscreen = sw - width < 3 and height / sh > 0.9
    logger.info(f'window size: {(width, height)}, fullscreen: {fullscreen}')
    webview.create_window('NS EMU TOOLS', url, width=width, height=height, text_select=True)
    webview.start(func=post_start, args=[fullscreen])


if __name__ == '__main__':
    import gevent.monkey

    gevent.monkey.patch_ssl()
    gevent.monkey.patch_socket()
    main()
