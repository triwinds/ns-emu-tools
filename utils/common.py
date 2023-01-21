import re


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
