import gevent.monkey
gevent.monkey.patch_all(httplib=True, subprocess=False)
import eel


eel.init("web")


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


def main():
    import_api_modules()
    from module.msg_notifier import update_notifier
    from config import config
    default_page = f'index_{config.setting.lastOpenEmuPage}.html'
    update_notifier('eel')
    if can_use_chrome():
        eel.start(default_page, port=0)
    else:
        eel.start(default_page, port=0, mode='user default')


if __name__ == '__main__':
    main()
