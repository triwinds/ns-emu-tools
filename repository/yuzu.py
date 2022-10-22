import requests


def get_latest_yuzu_release_info():
    resp = requests.get('https://api.github.com/repos/pineappleEA/pineapple-src/releases')
    for item in resp.json():
        if item['author']['login'] == 'pineappleEA':
            return item


def get_yuzu_release_info_by_version(version):
    url = f'https://api.github.com/repos/pineappleEA/pineapple-src/releases/tags/EA-{version}'
    return requests.get(url).json()


if __name__ == '__main__':
    print(get_latest_yuzu_release_info())
