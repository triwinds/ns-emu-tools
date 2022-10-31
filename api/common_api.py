import eel
from api.common_response import success_response, exception_response, error_response
from config import config, current_version
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
def get_current_version():
    return success_response(current_version)


@eel.expose
def check_update():
    from module.common import check_update
    has_update, latest_version = check_update()
    return success_response(has_update, latest_version)
