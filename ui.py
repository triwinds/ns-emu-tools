import gevent.monkey
gevent.monkey.patch_ssl()
gevent.monkey.patch_socket()
import eel
from config import config, current_version
from repository.yuzu import get_all_yuzu_release_infos
from module.common import get_firmware_infos
import logging

eel.init("web")
logger = logging.getLogger(__name__)


def success_response(data=None, msg=None):
    return {'code': 0, 'data': data, 'msg': msg}


def exception_response(ex):
    logger.error(ex, exc_info=True)
    return error_response(999, str(ex))


def error_response(code, msg):
    return {'code': code, 'msg': msg}


@eel.expose
def get_yuzu_config():
    return config.yuzu.to_dict()


@eel.expose
def ask_and_update_yuzu_path():
    from module.dialogs import ask_folder
    folder = ask_folder()
    logger.info(f'select folder: {folder}')
    if folder:
        from config import update_yuzu_path
        update_yuzu_path(folder)
        return success_response(msg=f'修改 yuzu 目录至 {folder}')
    else:
        return error_response(100, '修改已取消')


@eel.expose
def get_yuzu_release_infos():
    try:
        return success_response(get_all_yuzu_release_infos())
    except Exception as e:
        return exception_response(e)


@eel.expose
def get_available_firmware_infos():
    try:
        return success_response(get_firmware_infos())
    except Exception as e:
        return exception_response(e)


@eel.expose
def install_yuzu(version):
    if not version or version == '':
        return {'msg': f'无效的版本 {version}'}
    from module.yuzu import install_yuzu
    return {'msg': install_yuzu(version)}


@eel.expose
def install_firmware(version):
    if not version or version == '':
        return {'msg': f'无效的版本 {version}'}
    from module.yuzu import install_firmware_to_yuzu
    return {'msg': install_firmware_to_yuzu(version)}


@eel.expose
def get_available_keys_info():
    from module.common import get_keys_info
    try:
        return success_response(get_keys_info())
    except Exception as e:
        return exception_response(e)


@eel.expose
def install_keys(name):
    if not name or name == '':
        return {'msg': f'无效的 key {name}'}
    from module.yuzu import install_key_to_yuzu
    return success_response(msg=install_key_to_yuzu(name))


@eel.expose
def detect_yuzu_version():
    try:
        from module.yuzu import detect_yuzu_version
        return success_response(detect_yuzu_version())
    except Exception as e:
        return exception_response(e)


@eel.expose
def start_yuzu():
    from module.yuzu import start_yuzu
    try:
        start_yuzu()
        return success_response()
    except Exception as e:
        return exception_response(e)


@eel.expose
def get_current_version():
    return success_response(current_version)


@eel.expose
def check_update():
    from module.common import check_update
    has_update, latest_version = check_update()
    return success_response(has_update, latest_version)


@eel.expose
def open_yuzu_keys_folder():
    from module.yuzu import open_yuzu_keys_folder
    open_yuzu_keys_folder()
    return success_response()


def can_use_chrome():
    """ Identify if Chrome is available for Eel to use """
    import os
    from eel import chrome
    chrome_instance_path = chrome.find_path()
    return chrome_instance_path is not None and os.path.exists(chrome_instance_path)


def main():
    from module.msg_notifier import update_notifier
    update_notifier('eel')
    if can_use_chrome():
        eel.start("index.html", port=0)
    else:
        eel.start("index.html", port=0, mode='user default')


if __name__ == '__main__':
    main()
