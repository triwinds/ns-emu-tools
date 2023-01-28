from typing import Dict

import eel
from api.common_response import success_response, exception_response, error_response
from config import current_version
import logging
from module.common import get_firmware_infos

logger = logging.getLogger(__name__)


@eel.expose
def get_available_firmware_infos():
    try:
        return success_response(get_firmware_infos())
    except Exception as e:
        return exception_response(e)


@eel.expose
def get_current_version():
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
def load_history_path(emu_type: str):
    from storage import storage
    from config import config
    emu_type = emu_type.lower()
    if emu_type == 'yuzu':
        return success_response(list(_merge_to_set(storage.yuzu_history.keys(), config.yuzu.yuzu_path)))
    else:
        return success_response(list(_merge_to_set(storage.ryujinx_history.keys(), config.ryujinx.path)))


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
