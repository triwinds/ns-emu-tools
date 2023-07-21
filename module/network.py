import urllib.request
from config import config, user_agent
import logging
import os
import requests_cache
from requests.adapters import HTTPAdapter
from gevent.lock import RLock
import random
from module.msg_notifier import send_notify
from urllib.parse import urlparse

logger = logging.getLogger(__name__)

url_override_map = {
    'https://archive.org/download/nintendo-switch-global-firmwares/': 'https://nsarchive.e6ex.com/nsfrp/',
    'https://api.github.com': 'https://cfrp.e6ex.com/ghapi',
    # 'https://aka.ms/vs': 'https://nsarchive.e6ex.com/msvc'
    'https://raw.githubusercontent.com': 'https://www.githubs.cn/raw-githubusercontent',
}


github_us_mirrors = [
    ['https://nsarchive.e6ex.com/gh', '美国', '[美国 Cloudflare CDN] - 自建代理服务器'],

    # https://github.com/XIU2/UserScript/blob/master/GithubEnhanced-High-Speed-Download.user.js
    ['https://gh.gh2233.ml/https://github.com', '美国', '[美国 Cloudflare CDN] - 该公益加速源由 [@X.I.U/XIU2] 提供'],
    ['https://gh.ddlc.top/https://github.com', '美国', '[美国 Cloudflare CDN] - 该公益加速源由 [@mtr-static-official] 提供'],
    ['https://ghdl.feizhuqwq.cf/https://github.com', '美国', '[美国 Cloudflare CDN] - 该公益加速源由 [feizhuqwq.com] 提供'],
    ['https://gh.flyinbug.top/gh/https://github.com', '美国', '[美国 Cloudflare CDN] - 该公益加速源由 [Mintimate] 提供'],
    # ['https://proxy.zyun.vip/https://github.com', '美国', '[美国 Cloudflare CDN] - 该公益加速源由 [知了小站] 提供'],
    # ['https://cors.isteed.cc/github.com', '美国', '[美国 Cloudflare CDN] - 该公益加速源由 [Lufs\'s] 提供'],
    ['https://hub.gitmirror.com/https://github.com', '美国', '[美国 Cloudflare CDN] - 该公益加速源由 [GitMirror] 提供'],
    ['https://js.xxooo.ml/https://github.com', '美国', '[美国 Cloudflare CDN] - 该公益加速源由 [饭太硬] 提供'],
    ['https://download.nuaa.cf', '美国', '[美国 Cloudflare CDN] - 该公益加速源由 [LibraryCloud] 提供']
]

github_other_mirrors = [
    # ['https://download.fastgit.org', '德国', '[德国] - 该公益加速源由 [FastGit] 提供'],
    ['https://ghproxy.net/https://github.com', '韩国',
     '[韩国 首尔] - 该公益加速源由 [ghproxy] 提供，有日本、韩国、德国、巴西等地区的服务器，不过国内一般分配为韩国'],
    ['https://ghproxy.com/https://github.com', '韩国',
     '[韩国 首尔] - 该公益加速源由 [ghproxy] 提供，有日本、韩国、德国、巴西等地区的服务器，不过国内一般分配为韩国'],
    ['https://kgithub.com', '新加坡', '[新加坡] - 该公益加速源由 [KGitHub] 提供']
]

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
        _durable_cache_session.headers.update({'User-Agent': user_agent})
        _durable_cache_session.mount('https://ghproxy.net', HTTPAdapter(max_retries=5))
        _durable_cache_session.mount('https://ghproxy.com', HTTPAdapter(max_retries=5))
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
    'split': '4',
    'max-connection-per-server': '4',
    'min-split-size': '12M',
}

chrome_ua = 'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) ' \
            'Chrome/113.0.0.0 Safari/537.36'
github_api_fallback_flag = False


def is_using_proxy():
    proxies = get_proxies()
    logger.info(f'current proxies: {proxies}')
    return proxies is not None and proxies != {}


def uri_validator(x):
    try:
        result = urlparse(x)
        return all([result.scheme, result.netloc])
    except:
        return False


def get_proxies():
    proxy = config.setting.network.proxy
    if proxy == 'system':
        return get_system_proxies()
    elif proxy is None or proxy.strip() == '':
        return {}
    elif uri_validator(proxy):
        return {'http': proxy, 'https': proxy}
    else:
        logger.info(f'unknown proxy: {proxy}')
        return {}


def get_system_proxies():
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


def get_github_mirrors():
    github_mirrors = [
        ['cloudflare_load_balance', '美国', '[美国 Cloudflare CDN] 随机选择 Cloudflare 服务器'],
        ['direct', '美国', '直连 GitHub']
    ]
    github_mirrors += github_us_mirrors
    github_mirrors += github_other_mirrors
    return github_mirrors


def get_github_download_url(origin_url: str):
    mirror = config.setting.network.githubDownloadMirror
    if not mirror or mirror == 'direct':
        logger.info(f'using origin url: {origin_url}')
        return origin_url
    if mirror == 'cloudflare_load_balance':
        choice = random.choice(github_us_mirrors)
        send_notify(f'使用 GitHub 镜像: {choice[2]}')
        mirror = choice[0]
    url = origin_url.replace('https://github.com', mirror)
    logger.info(f'using new url: {url}')
    return url


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
        return s.connect_ex(('127.0.0.1', port)) == 0


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


def test_github_us_mirrors():
    import requests
    invalid_mirrors = []
    test_mirrors = github_us_mirrors[1:]
    for mirror in test_mirrors:
        try:
            print(f'testing {mirror[2]}...', end='')
            url = f'{mirror[0]}/XIU2/CloudflareSpeedTest/releases/download/v2.2.2/CloudflareST_windows_amd64.zip'
            resp = requests.head(url, headers={'user-agent': chrome_ua})
            # print(resp.headers)
            if 'Content-Length' not in resp.headers:
                print(resp.headers)
                invalid_mirrors.append(mirror[2])
                print('failed')
            print('worked')
        except Exception as e:
            print('failed')
            print(e)
            invalid_mirrors.append(mirror[2])
    print(f'invalid_mirrors: {invalid_mirrors}')


if __name__ == '__main__':
    test_github_us_mirrors()
