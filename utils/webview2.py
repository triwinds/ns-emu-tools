# modify from https://github.com/r0x0r/pywebview/blob/master/webview/platforms/winforms.py

import winreg
import logging
import os

logger = logging.getLogger(__name__)


def get_dot_net_version():
    net_key, version = None, None
    try:
        net_key = winreg.OpenKey(winreg.HKEY_LOCAL_MACHINE, r'SOFTWARE\Microsoft\NET Framework Setup\NDP\v4\Full')
        version, _ = winreg.QueryValueEx(net_key, 'Release')
    finally:
        if net_key:
            winreg.CloseKey(net_key)
    return version


def is_chromium(verbose=False):
    from utils.common import is_newer_version

    def edge_build(key_type, key, description=''):
        try:
            windows_key = None
            if key_type == 'HKEY_CURRENT_USER':
                path = rf'Microsoft\EdgeUpdate\Clients\{key}'
            else:
                path = rf'WOW6432Node\Microsoft\EdgeUpdate\Clients\{key}'
            with winreg.OpenKey(getattr(winreg, key_type), rf'SOFTWARE\{path}') as windows_key:
                build, _ = winreg.QueryValueEx(windows_key, 'pv')
                return str(build)
        except Exception as e:
            pass
        return '0'

    try:
        build_versions = [
            # runtime
            {'key': '{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}', 'description': 'Microsoft Edge WebView2 Runtime'},
            # beta
            {'key': '{2CD8A007-E189-409D-A2C8-9AF4EF3C72AA}', 'description': 'Microsoft Edge WebView2 Beta'},
            # dev
            {'key': '{0D50BFEC-CD6A-4F9A-964C-C7416E3ACB10}', 'description': 'Microsoft Edge WebView2 Developer'},
            # canary
            {'key': '{65C35B14-6C1D-4122-AC46-7148CC9D6497}', 'description': 'Microsoft Edge WebView2 Canary'},
        ]

        for item in build_versions:
            for key_type in ('HKEY_CURRENT_USER', 'HKEY_LOCAL_MACHINE'):
                build = edge_build(key_type, item['key'], item['description'])
                if is_newer_version('105.0.0.0', build):
                    if verbose:
                        logger.info(f'webview2 version: {build}, description: {item["description"]}')
                    return True

    except Exception as e:
        logger.exception(e)
    return False


def ensure_runtime_components():
    flag = False
    version = get_dot_net_version()
    logger.info(f'dot net version: {version}')
    if version < 394802:  # .NET 4.6.2
        install_dot_net()
        flag = True
    if not is_chromium(verbose=True):
        install_webview2()
        flag = True
    if flag:
        show_msgbox('重启程序', '组件安装完成后, 请重新启动程序.', 0)
    return flag


def can_use_webview():
    version = get_dot_net_version()
    return version >= 394802 and is_chromium()


def show_msgbox(title, content, style):
    import ctypes  # An included library with Python install.
    #  Styles:
    #  0 : OK
    #  1 : OK | Cancel
    #  2 : Abort | Retry | Ignore
    #  3 : Yes | No | Cancel
    #  4 : Yes | No
    #  5 : Retry | Cancel
    #  6 : Cancel | Try Again | Continue
    return ctypes.windll.user32.MessageBoxW(0, content, title, style)


def get_download_file_name(resp):
    if 'Content-Disposition' in resp.headers:
        import cgi
        value, params = cgi.parse_header(resp.headers['Content-Disposition'])
        return params['filename']
    if resp.url.find('/'):
        return resp.url.rsplit('/', 1)[1]
    return 'index'


def download_file(url):
    import requests
    resp = requests.get(url)
    local_filename = get_download_file_name(resp)
    with open(local_filename, 'wb') as f:
        f.write(resp.content)
    logger.info(f'[{local_filename}] download success.')
    return local_filename


def install_dot_net():
    ret = show_msgbox("运行组件缺失", "缺失 .NET Framework 组件, 是否下载安装?", 4)
    if ret == 7:
        raise RuntimeError('缺失 .NET Framework 组件')
    fn = download_file('https://go.microsoft.com/fwlink/?LinkId=2203304')
    logger.info('installing .NET Framework ...')
    os.system(fn)
    logger.info('removing .NET Framework installer.')
    os.remove(fn)


def install_webview2():
    ret = show_msgbox("运行组件缺失", "缺失 Microsoft Edge WebView2 组件, 是否下载安装?", 4)
    if ret == 7:
        raise RuntimeError('缺失 Microsoft Edge WebView2 组件')
    fn = download_file('https://go.microsoft.com/fwlink/p/?LinkId=2124703')
    logger.info('installing webview2...')
    os.system(fn)
    logger.info('removing webview2 installer.')
    os.remove(fn)


if __name__ == '__main__':
    import config

    # check_runtime_components()
    install_webview2()
