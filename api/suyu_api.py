import eel
from api.common_response import success_response, exception_response, error_response
from config import config, dump_config
import logging

logger = logging.getLogger(__name__)


@eel.expose
def open_suyu_keys_folder():
    from module.suyu import open_suyu_keys_folder
    open_suyu_keys_folder()
    return success_response()


@eel.expose
def get_suyu_config():
    return config.suyu.to_dict()


@eel.expose
def ask_and_update_suyu_path():
    from module.dialogs import ask_folder
    folder = ask_folder()
    logger.info(f'select folder: {folder}')
    if folder:
        from module.suyu import update_suyu_path
        update_suyu_path(folder)
        return success_response(msg=f'修改 suyu 目录至 {folder}')
    else:
        return error_response(100, '修改已取消')


@eel.expose
def update_suyu_path(folder: str):
    from module.suyu import update_suyu_path
    update_suyu_path(folder)
    return success_response(msg=f'修改 suyu 目录至 {folder}')


# @eel.expose
# def detect_suyu_version():
#     try:
#         from module.suyu import detect_suyu_version
#         return success_response(detect_suyu_version())
#     except Exception as e:
#         return exception_response(e)


@eel.expose
def start_suyu():
    from module.suyu import start_suyu
    try:
        start_suyu()
        return success_response()
    except Exception as e:
        return exception_response(e)


@eel.expose
def install_suyu(version, branch):
    if not version or version == '':
        return error_response(404, f'无效的版本 {version}')
    from module.suyu import install_suyu
    try:
        return success_response(msg=install_suyu(version))
    except Exception as e:
        return exception_response(e)


@eel.expose
def install_suyu_firmware(version):
    if not version or version == '':
        return error_response(404, f'无效的版本 {version}')
    from module.suyu import install_firmware_to_suyu
    try:
        return success_response(msg=install_firmware_to_suyu(version))
    except Exception as e:
        return exception_response(e)


@eel.expose
def get_all_suyu_release_versions():
    from repository.suyu import get_all_suyu_release_versions
    try:
        return success_response(get_all_suyu_release_versions())
    except Exception as e:
        return exception_response(e)


# @eel.expose
# def get_suyu_commit_logs():
#     from module.suyu import get_suyu_commit_logs
#     try:
#         return success_response(get_suyu_commit_logs())
#     except Exception as e:
#         return exception_response(e)
