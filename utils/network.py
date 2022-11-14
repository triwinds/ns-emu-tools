import urllib.request
from config import config
import logging


logger = logging.getLogger(__name__)


url_override_map = {
    'https://github.com': 'https://nsarchive.e6ex.com/gh',
    'https://archive.org/download/nintendo-switch-global-firmwares/': 'https://nsarchive.e6ex.com/nsfrp/',
    'https://api.github.com': 'https://cfrp.e6ex.com/ghapi',
    'https://aka.ms/vs': 'https://nsarchive.e6ex.com/msvc'
}


def is_using_proxy():
    proxies = get_proxies()
    logger.info(f'current proxies: {proxies}')
    return proxies is not None and proxies != {}


def get_proxies():
    return urllib.request.getproxies()


def get_proxy_option():
    return {'all-proxy': iter(get_proxies().values()).__next__()}


def get_global_options():
    if is_using_proxy():
        global_options = {
            'split': '16',
            'max-connection-per-server': '16',
            'min-split-size': '4M',
        }
    else:
        global_options = {
            'split': '4',
            'max-connection-per-server': '4',
            'min-split-size': '12M',
        }
    return global_options


def init_download_options_with_proxy():
    if is_using_proxy():
        return {'all-proxy': iter(get_proxies().values()).__next__()}
    else:
        return {}


def get_finial_url(origin_url: str):
    network_setting = config.setting.network
    if origin_url.startswith('https://api.github.com'):
        return get_finial_url_with_mode(origin_url, network_setting.githubApiMode)
    return get_finial_url_with_mode(origin_url, network_setting.cdnMode)


def get_finial_url_with_mode(origin_url: str, mode: str):
    """
    get_finial_url_with_mode
    :param origin_url: origin_url
    :param mode: auto-detect, cdn, direct
    :return: url
    """
    if mode == 'direct':
        logger.info(f'using origin url: {origin_url}')
        return origin_url
    elif mode == 'cdn':
        return get_override_url(origin_url)
    else:
        if is_using_proxy():
            logger.info(f'using origin url: {origin_url}')
            return origin_url
        else:
            return get_override_url(origin_url)


def get_override_url(origin_url):
    for k in url_override_map:
        if origin_url.startswith(k):
            new_url = origin_url.replace(k, url_override_map[k])
            logger.info(f'using new url: {new_url}')
            return new_url
    logger.info(f'using origin url: {origin_url}')
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
