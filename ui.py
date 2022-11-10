import gevent.monkey
gevent.monkey.patch_all(httplib=True, subprocess=False)
import eel





def can_use_chrome():
    """ Identify if Chrome is available for Eel to use """
    import os
    from eel import chrome
    chrome_instance_path = chrome.find_path()
    return chrome_instance_path is not None and os.path.exists(chrome_instance_path)


def import_api_modules():
    import api.yuzu_api
    import api.common_api
    import api.ryujinx_api


def main(port=0, mode=None):
    import_api_modules()
    if port:
        eel.init('vue')
    else:
        eel.init("web")
    from module.msg_notifier import update_notifier
    from config import config
    default_page = f'index.html'
    update_notifier('eel-console')
    if mode is None:
        mode = 'chrome' if can_use_chrome() else 'user default'
    eel.start(default_page, port=port, size=(1280, 720), mode=mode)


if __name__ == '__main__':
    main(8888, False)
