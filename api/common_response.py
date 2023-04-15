import logging
from module.msg_notifier import send_notify
from exception.common_exception import *
from exception.download_exception import *
from exception.install_exception import *


logger = logging.getLogger(__name__)


def success_response(data=None, msg=None):
    return {'code': 0, 'data': data, 'msg': msg}


def exception_response(ex):
    import traceback
    if type(ex) in exception_handler_map:
        return exception_handler_map[type(ex)](ex)
    logger.error(ex, exc_info=True)
    traceback_str = "\n".join(traceback.format_exception(ex))
    send_notify(f'出现异常, {traceback_str}')
    return error_response(999, str(ex))


def version_not_found_handler(ex: VersionNotFoundException):
    logger.error(f'{str(ex)}')
    send_notify(f'无法获取 {ex.branch} 分支的 [{ex.target_version}] 版本信息')
    return error_response(404, str(ex))


def md5_not_found_handler(ex: Md5NotMatchException):
    logger.error(f'{str(ex)}')
    send_notify(f'固件文件 md5 不匹配, 请重新下载')
    return error_response(501, str(ex))


def download_interrupted_handler(ex: DownloadInterrupted):
    logger.info(f'{str(ex)}')
    send_notify(f'下载任务被终止')
    return error_response(601, str(ex))


def download_paused_handler(ex: DownloadPaused):
    logger.info(f'{str(ex)}')
    send_notify(f'下载任务被暂停')
    return error_response(602, str(ex))


def download_not_completed_handler(ex: DownloadNotCompleted):
    logger.info(f'{str(ex)}')
    send_notify(f'下载任务 [{ex.name}] 未完成, 状态: {ex.status}')
    return error_response(603, str(ex))


def fail_to_copy_files_handler(ex: FailToCopyFiles):
    logger.exception(ex.raw_exception)
    send_notify(f'{ex.msg}, 这可能是由于相关文件被占用或者没有相关目录的写入权限造成的')
    send_notify(f'请检查相关程序是否已经关闭, 或者重启一下系统试试')
    return error_response(701, str(ex))


def ignored_exception_handler(ex: IgnoredException):
    logger.info(f'{str(ex)}')
    return error_response(801, str(ex))


exception_handler_map = {
    VersionNotFoundException: version_not_found_handler,
    Md5NotMatchException: md5_not_found_handler,
    DownloadInterrupted: download_interrupted_handler,
    DownloadPaused: download_paused_handler,
    DownloadNotCompleted: download_not_completed_handler,
    FailToCopyFiles: fail_to_copy_files_handler,
    IgnoredException: ignored_exception_handler,
}


def error_response(code, msg):
    return {'code': code, 'msg': msg}


__all__ = ['success_response', 'exception_response', 'error_response']
