import os
from typing import Dict

import eel
from api.common_response import *
from config import current_version, shared
import logging
import time
from module.firmware import get_firmware_infos

logger = logging.getLogger(__name__)


@eel.expose
def get_available_firmware_infos():
    try:
        return success_response(get_firmware_infos())
    except Exception as e:
        return exception_response(e)


@generic_api
def get_available_firmware_sources():
    from module.firmware import get_available_firmware_sources
    return get_available_firmware_sources()


@eel.expose
def get_current_version():
    shared['ui_init_time'] = time.time()
    return success_response(current_version)


@eel.expose
def update_last_open_emu_page(page):
    from config import update_last_open_emu_page
    update_last_open_emu_page(page)


@eel.expose
def update_dark_state(dark):
    from config import update_dark_state
    update_dark_state(dark)


@eel.expose
def detect_firmware_version(emu_type: str):
    from module.firmware import detect_firmware_version
    try:
        detect_firmware_version(emu_type)
        return success_response()
    except Exception as e:
        return exception_response(e)


@eel.expose
def get_config():
    from config import config
    return success_response(config.to_dict())


@eel.expose
def open_url_in_default_browser(url):
    import webbrowser
    webbrowser.open(url, new=0, autoraise=True)


@eel.expose
def update_setting(setting: Dict[str, object]):
    from config import config, update_setting
    update_setting(setting)
    return success_response(config.to_dict())


@eel.expose
def get_net_release_info_by_tag(tag: str):
    from repository.my_info import get_release_info_by_tag
    try:
        return success_response(get_release_info_by_tag(tag))
    except Exception as e:
        return exception_response(e)


@eel.expose
def stop_download():
    from module.downloader import stop_download
    try:
        return success_response(stop_download())
    except Exception as e:
        return exception_response(e)


@eel.expose
def pause_download():
    from module.downloader import pause_download
    try:
        return success_response(pause_download())
    except Exception as e:
        return exception_response(e)


@eel.expose
def load_history_path(emu_type: str):
    from storage import storage
    from config import config
    emu_type = emu_type.lower()
    if emu_type == 'yuzu':
        return success_response(list(_merge_to_set(storage.yuzu_history.keys(), config.yuzu.yuzu_path)))
    else:
        return success_response(list(_merge_to_set(storage.ryujinx_history.keys(), config.ryujinx.path)))


@eel.expose
def delete_history_path(emu_type: str, path_to_delete: str):
    from storage import delete_history_path
    delete_history_path(emu_type, path_to_delete)
    return success_response()


@eel.expose
def get_github_mirrors():
    from module.network import get_github_mirrors
    try:
        return success_response(get_github_mirrors())
    except Exception as e:
        return exception_response(e)


@eel.expose
def update_window_size(width: int, height: int):
    from config import dump_config, config, shared
    if shared['mode'] == 'webview':
        from ui_webview import get_window_size
        width, height = get_window_size()
    if width == config.setting.ui.width and height == config.setting.ui.height:
        return success_response()
    config.setting.ui.width = width
    config.setting.ui.height = height
    logger.info(f'saving window size: {(config.setting.ui.width, config.setting.ui.height)}')
    dump_config()
    return success_response()


@generic_api
def get_storage():
    from storage import storage
    return storage.to_dict()


@generic_api
def delete_path(path: str):
    from module.common import delete_path
    return delete_path(path)


def _merge_to_set(*cols):
    from collections.abc import Iterable
    s = set()
    for c in cols:
        if isinstance(c, Iterable) and not isinstance(c, str):
            for i in c:
                s.add(i)
        else:
            s.add(c)
    return s
