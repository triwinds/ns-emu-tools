

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
