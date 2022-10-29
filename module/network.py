import requests
from utils.common import is_using_proxy, get_proxies


url_override_map = {
    'https://github.com': 'https://ghproxy.com/https://github.com',
    'https://archive.org/download/nintendo-switch-global-firmwares/': 'https://nsarchive.e6ex.com/nsfrp/',
}


def get_global_options():
    if is_using_proxy():
        global_options = {
            'split': '16',
            'max-connection-per-server': '16',
            'min-split-size': '1M',
        }
    else:
        global_options = {
            'split': '16',
            'max-connection-per-server': '16',
            'min-split-size': '4M',
        }
    return global_options


def init_download_options_with_proxy():
    if is_using_proxy():
        return {'all-proxy': iter(get_proxies().values()).__next__()}
    else:
        return {}


def get_finial_url(origin_url: str):
    if is_using_proxy():
        return origin_url
    for k in url_override_map:
        if origin_url.startswith(k):
            return origin_url.replace(k, url_override_map[k])
    return origin_url


def is_port_in_use(port: int) -> bool:
    import socket
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
        return s.connect_ex(('localhost', port)) == 0


def get_available_port() -> int:
    import random
    while True:
        port = random.randint(20000, 60000)
        if not is_port_in_use(port):
            break
    return port
