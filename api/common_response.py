import logging


logger = logging.getLogger(__name__)


def success_response(data=None, msg=None):
    return {'code': 0, 'data': data, 'msg': msg}


def exception_response(ex):
    logger.error(ex, exc_info=True)
    return error_response(999, str(ex))


def error_response(code, msg):
    return {'code': code, 'msg': msg}