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


def get_installed_software():
    import winreg

    def foo(hive, flag):
        aReg = winreg.ConnectRegistry(None, hive)
        aKey = winreg.OpenKey(aReg, r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall",
                              0, winreg.KEY_READ | flag)
        count_subkey = winreg.QueryInfoKey(aKey)[0]
        software_list = []
        for i in range(count_subkey):
            software = {}
            try:
                asubkey_name = winreg.EnumKey(aKey, i)
                asubkey = winreg.OpenKey(aKey, asubkey_name)
                software['name'] = winreg.QueryValueEx(asubkey, "DisplayName")[0]

                try:
                    software['version'] = winreg.QueryValueEx(asubkey, "DisplayVersion")[0]
                except EnvironmentError:
                    software['version'] = 'undefined'
                try:
                    software['publisher'] = winreg.QueryValueEx(asubkey, "Publisher")[0]
                except EnvironmentError:
                    software['publisher'] = 'undefined'
                software_list.append(software)
            except EnvironmentError:
                continue

        return software_list

    try:
        sl = (foo(winreg.HKEY_LOCAL_MACHINE, winreg.KEY_WOW64_32KEY) +
              foo(winreg.HKEY_LOCAL_MACHINE, winreg.KEY_WOW64_64KEY) +
              foo(winreg.HKEY_CURRENT_USER, 0))
        return sl
    except Exception as e:
        logger.info('Exception occurred in get_software_list, exception is: {}'.format(e))
        return []


def find_installed_software(name_pattern: str):
    import re
    software_list = get_installed_software()
    pattern = re.compile(name_pattern)
    final_list = [s for s in software_list if pattern.search(s['name']) is not None]
    return final_list


def is_newer_version(min_version, current_version):
    cur_range = current_version.split(".")
    min_range = min_version.split(".")
    for index in range(len(cur_range)):
        if len(min_range) > index:
            try:
                return int(cur_range[index]) >= int(min_range[index])
            except:
                return False
    return False


if __name__ == '__main__':
    from pprint import pp
    # from config import config
    # print(is_path_in_use(config.yuzu.yuzu_path))
    pp(get_installed_software())
    test_l = find_installed_software(r'Microsoft Visual C\+\+ .+ Redistributable')
    print(any(is_newer_version('14.34', s['version']) for s in test_l))
