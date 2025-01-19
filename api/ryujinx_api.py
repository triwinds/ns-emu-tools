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
        from module.ryujinx import update_ryujinx_path
        update_ryujinx_path(folder)
        return success_response(msg=f'修改 ryujinx 目录至 {folder}')
    else:
        return error_response(100, '修改已取消')


@eel.expose
def update_ryujinx_path(folder: str):
    from module.ryujinx import update_ryujinx_path
    update_ryujinx_path(folder)
    return success_response(msg=f'修改 ryujinx 目录至 {folder}')


@eel.expose
def get_ryujinx_release_infos():
    try:
        print(config.ryujinx.branch)
        return success_response(get_all_ryujinx_release_infos(config.ryujinx.branch))
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
def install_ryujinx(version, branch):
    if not version or version == '':
        return {'msg': f'无效的版本 {version}'}
    from module.ryujinx import install_ryujinx_by_version
    try:
        return success_response(msg=install_ryujinx_by_version(version, branch))
    except Exception as e:
        return exception_response(e)


@eel.expose
def install_ryujinx_firmware(version):
    if not version or version == '':
        return {'msg': f'无效的版本 {version}'}
    from module.ryujinx import install_firmware_to_ryujinx
    try:
        return success_response(msg=install_firmware_to_ryujinx(version))
    except Exception as e:
        return exception_response(e)


@eel.expose
def switch_ryujinx_branch(branch: str):
    from config import dump_config
    if branch not in {'mainline', 'canary'}:
        return error_response(-1, f'Invalidate branch: {branch}')
    target_branch = branch
    logger.info(f'switch ryujinx branch to {target_branch}')
    config.ryujinx.branch = target_branch
    dump_config()
    return success_response(config.ryujinx.to_dict())


@eel.expose
def load_ryujinx_change_log():
    from repository.ryujinx import load_ryujinx_change_log
    try:
        return success_response(load_ryujinx_change_log())
    except Exception as e:
        return exception_response(e)

