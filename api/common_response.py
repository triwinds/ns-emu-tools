import logging
from module.msg_notifier import send_notify


logger = logging.getLogger(__name__)


def success_response(data=None, msg=None):
    return {'code': 0, 'data': data, 'msg': msg}


def exception_response(ex):
    import traceback
    logger.error(ex, exc_info=True)
    traceback_str = "\n".join(traceback.format_exception(ex))
    send_notify(f'出现异常, {traceback_str}')
    return error_response(999, str(ex))


def error_response(code, msg):
    return {'code': code, 'msg': msg}


__all__ = ['success_response', 'exception_response', 'error_response']
