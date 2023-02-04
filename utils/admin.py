import logging


logger = logging.getLogger(__name__)


def run_with_admin_privilege(executable, argument_line):
    import ctypes
    ret = ctypes.windll.shell32.ShellExecuteW(None, u"runas", executable, argument_line, None, 1)
    logger.info(f'run_with_admin_privilege ret code: {ret}')
    return ret


def check_is_admin():
    import ctypes
    import os
    try:
        return os.getuid() == 0
    except AttributeError:
        return ctypes.windll.shell32.IsUserAnAdmin() != 0


if __name__ == '__main__':
    run_with_admin_privilege('cmd', r'/c copy D:\py\ns-emu-tools\test.json D:\py\ns-emu-tools\test2.json')

