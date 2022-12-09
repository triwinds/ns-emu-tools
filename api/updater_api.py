from config import config
from api.common_response import *

import eel


@eel.expose
def check_update():
    from module.updater import check_update
    has_update, latest_version = check_update()
    return success_response(has_update, latest_version)


@eel.expose
def download_net_by_tag(tag: str):
    from module.updater import download_net_by_tag
    try:
        return success_response(download_net_by_tag(tag))
    except Exception as e:
        return exception_response(e)


@eel.expose
def update_net_by_tag(tag: str):
    from module.updater import update_self_by_tag
    try:
        return success_response(update_self_by_tag(tag))
    except Exception as e:
        return exception_response(e)


@eel.expose
def load_change_log():
    from repository.my_info import load_change_log
    try:
        return success_response(load_change_log())
    except Exception as e:
        exception_response(e)
