import urllib.request


def is_using_proxy():
    proxies = urllib.request.getproxies()
    return proxies is not None and proxies != {}


if __name__ == '__main__':
    print(is_using_proxy())
