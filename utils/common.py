import urllib.request


def is_using_proxy():
    proxies = get_proxies()
    print(f'proxies: {proxies}')
    return proxies is not None and proxies != {}


def get_proxies():
    return urllib.request.getproxies()


def get_proxy_option():
    return {'all-proxy': iter(get_proxies().values()).__next__()}


if __name__ == '__main__':
    print(is_using_proxy())
