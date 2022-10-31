import eel
from api.common_response import success_response, exception_response, error_response
from repository.ryujinx import get_all_ryujinx_release_infos
from config import config
import logging


logger = logging.getLogger(__name__)


@eel.expose
def open_ryujinx_keys_folder():
    from module.ryujinx import open_ryujinx_keys_folder
    open_ryujinx_keys_folder()
    return success_response()


@eel.expose
def get_ryujinx_config():
    return config.ryujinx.to_dict()


@eel.expose
def ask_and_update_ryujinx_path():
    from module.dialogs import ask_folder
    folder = ask_folder()
    logger.info(f'select folder: {folder}')
    if folder:
        from config import update_ryujinx_path
        update_ryujinx_path(folder)
        return success_response(msg=f'修改 ryujinx 目录至 {folder}')
    else:
        return error_response(100, '修改已取消')


@eel.expose
def get_ryujinx_release_infos():
    try:
        return success_response(get_all_ryujinx_release_infos())
    except Exception as e:
        return exception_response(e)


@eel.expose
def detect_ryujinx_version():
    try:
        from module.ryujinx import detect_ryujinx_version
        return success_response(detect_ryujinx_version())
    except Exception as e:
        return exception_response(e)


@eel.expose
def start_ryujinx():
    from module.ryujinx import start_ryujinx
    try:
        start_ryujinx()
        return success_response()
    except Exception as e:
        return exception_response(e)


@eel.expose
def install_ryujinx(version):
    if not version or version == '':
        return {'msg': f'无效的版本 {version}'}
    from module.ryujinx import install_ryujinx_by_version
    return {'msg': install_ryujinx_by_version(version)}


@eel.expose
def install_ryujinx_firmware(version):
    if not version or version == '':
        return {'msg': f'无效的版本 {version}'}
    from module.ryujinx import install_firmware_to_ryujinx
    return {'msg': install_firmware_to_ryujinx(version)}

