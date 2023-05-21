import re
import time
from pathlib import Path
from module.msg_notifier import send_notify
import logging


logger = logging.getLogger(__name__)
path_unicode_re = re.compile(r'\\x([\da-f]{4})')


def callback(hwnd, strings):
    from win32 import win32gui
    window_title = win32gui.GetWindowText(hwnd)
    # left, top, right, bottom = win32gui.GetWindowRect(hwnd)
    if window_title:
        strings.append(window_title)
    return True


def get_all_window_name():
    from win32 import win32gui
    win_list = []  # list of strings containing win handles and window titles
    win32gui.EnumWindows(callback, win_list)  # populate list
    return win_list


def decode_yuzu_path(raw_path_in_config: str):
    # raw_path_in_config = raw_path_in_config.replace("'", "\'")
    raw_path_in_config = path_unicode_re.sub(r'\\u\1', raw_path_in_config)
    # return eval(f"'{raw_path_in_config}'")
    return raw_path_in_config.encode().decode("unicode-escape")


def find_all_instances(process_name: str, exe_path: Path = None):
    import psutil
    result = []
    for p in psutil.process_iter():
        if p.name().startswith(process_name):
            if exe_path is not None:
                process_path = Path(p.exe()).parent.absolute()
                if exe_path.absolute() != process_path:
                    continue
            result.append(p)
    return result


def kill_all_instances(process_name: str, exe_path: Path = None):
    processes = find_all_instances(process_name, exe_path)
    if processes:
        for p in processes:
            send_notify(f'关闭进程 {p.name()} [{p.pid}]')
            p.kill()
        time.sleep(1)


def is_path_in_use(file_path):
    # Only works under windows
    if isinstance(file_path, Path):
        path = file_path
    else:
        path = Path(file_path)
    if not path.exists():
        return False
    try:
        path.rename(path)
    except PermissionError:
        return True
    else:
        return False


if __name__ == '__main__':
    from config import config
    print(is_path_in_use(config.yuzu.yuzu_path))
