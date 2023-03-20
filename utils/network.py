import urllib.request
from config import config, user_agent
import logging
import os
import requests_cache
from requests.adapters import HTTPAdapter
from gevent.lock import RLock

logger = logging.getLogger(__name__)

url_override_map = {
    'https://archive.org/download/nintendo-switch-global-firmwares/': 'https://nsarchive.e6ex.com/nsfrp/',
    'https://api.github.com': 'https://cfrp.e6ex.com/ghapi',
    # 'https://aka.ms/vs': 'https://nsarchive.e6ex.com/msvc'
    'https://raw.githubusercontent.com': 'https://www.githubs.cn/raw-githubusercontent',
}

github_override_map = {
    'self': 'https://nsarchive.e6ex.com/gh',
    'ghproxy': 'https://ghproxy.com/https://github.com',
    'zhiliao': 'https://proxy.zyun.vip/https://github.com',
    'nuaa': 'https://download.nuaa.cf',
}

if config.setting.network.useDoh:
    from utils.doh import install_doh
    install_doh()

session = requests_cache.CachedSession(cache_control=True, backend='memory')
session.headers.update({'User-Agent': user_agent})
session.mount('https://cfrp.e6ex.com', HTTPAdapter(max_retries=5))
session.mount('https://nsarchive.e6ex.com', HTTPAdapter(max_retries=5))
session.mount('https://api.github.com', HTTPAdapter(max_retries=5))


_durable_cache_session = None
request_lock = RLock()


def get_durable_cache_session():
    global _durable_cache_session
    if not _durable_cache_session:
        _durable_cache_session = requests_cache.CachedSession(cache_control=True)
        _durable_cache_session.mount('https://cdn.jsdelivr.net', HTTPAdapter(max_retries=5))
        _durable_cache_session.mount('https://nsarchive.e6ex.com', HTTPAdapter(max_retries=5))
        origin_get = _durable_cache_session.get

        def sync_get(url: str, params=None, **kwargs):
            request_lock.acquire()
            try:
                return origin_get(url, params, **kwargs)
            finally:
                request_lock.release()

        _durable_cache_session.get = sync_get
    return _durable_cache_session


options_on_proxy = {
    'split': '16',
    'max-connection-per-server': '16',
    'min-split-size': '4M',
}

options_on_cdn = {
    'split': '8',
    'max-connection-per-server': '8',
    'min-split-size': '12M',
}

chrome_ua = 'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) ' \
            'Chrome/110.0.0.0 Safari/537.36'
github_api_fallback_flag = False


def is_using_proxy():
    proxies = get_proxies()
    logger.info(f'current proxies: {proxies}')
    return proxies is not None and proxies != {}


def get_proxies():
    proxies = {}
    if os.name == 'nt':
        proxies.update(urllib.request.getproxies_registry())
    proxies.update(urllib.request.getproxies())
    if 'no' in proxies:
        del proxies['no']
    return proxies


def get_global_options():
    return {}


def init_download_options_with_proxy(url):
    options = {'user-agent': user_agent if 'e6ex.com' in url else chrome_ua}
    if is_using_proxy():
        options['all-proxy'] = iter(get_proxies().values()).__next__()
        options.update(options_on_proxy)
    else:
        options.update(options_on_cdn)
    return options


def get_github_download_url(origin_url: str):
    source = config.setting.network.githubDownloadSource
    if source in github_override_map:
        prefix = github_override_map[source]
        url = origin_url.replace('https://github.com', prefix)
        logger.info(f'using new url: {url}')
        return url
    logger.info(f'using origin url: {origin_url}')
    return origin_url


def get_finial_url(origin_url: str):
    network_setting = config.setting.network
    if origin_url.startswith('https://api.github.com'):
        return get_finial_url_with_mode(origin_url, network_setting.githubApiMode)
    return get_finial_url_with_mode(origin_url, network_setting.firmwareSource)


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


def request_github_api(url: str):
    global github_api_fallback_flag
    logger.info(f'requesting github api: {url}')
    from module.msg_notifier import send_notify
    if config.setting.network.githubApiMode != 'cdn' and not github_api_fallback_flag:
        try:
            resp = session.get(url, timeout=5)
            data = resp.json()
            if isinstance(data, dict) and 'message' in data and 'API rate limit exceeded' in data["message"]:
                logger.warning(f'GitHub API response message: {data["message"]}')
                send_notify(f'GitHub API response message: {data["message"]}')
                send_notify(f'当前 IP 可能已达到 GitHub api 当前时段的使用上限, 尝试转用 CDN')
                send_notify(f'如果在多次使用中看到这个提示，可以直接在设置中将 GitHub api 设置为使用 cdn，以避免不必要的重试')
                github_api_fallback_flag = True
            else:
                return data
        except Exception as e:
            logger.warning(f'Error occur when requesting github api, msg: {str(e)}')
            send_notify(f'直连 GitHub api 时出现异常, 尝试转用 CDN')
            send_notify(f'如果在多次使用中看到这个提示，可以直接在设置中将 GitHub api 设置为使用 cdn，以避免不必要的重试')
            github_api_fallback_flag = True
    url = get_override_url(url)
    return session.get(url).json()
