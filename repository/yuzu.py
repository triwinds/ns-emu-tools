import requests
from utils.network import get_finial_url


def get_all_yuzu_release_infos():
    resp = requests.get(get_finial_url('https://api.github.com/repos/pineappleEA/pineapple-src/releases'))
    res = [item for item in resp.json() if item['author']['login'] == 'pineappleEA']
    return res


def get_all_yuzu_release_versions(branch: str):
    res = []
    if branch.lower() == 'mainline':
        resp = requests.get(get_finial_url('https://api.github.com/repos/yuzu-emu/yuzu-mainline/releases'))
        for item in resp.json():
            res.append(item['tag_name'][11:])
    else:
        resp = requests.get(get_finial_url('https://api.github.com/repos/pineappleEA/pineapple-src/releases'))
        for item in resp.json():
            if item['author']['login'] == 'pineappleEA':
                res.append(item['tag_name'][3:])
    return res


def get_latest_yuzu_release_info():
    return get_all_yuzu_release_infos()[0]


def get_yuzu_release_info_by_version(version, branch='ea'):
    if branch.lower() == 'mainline':
        url = get_finial_url(f'https://api.github.com/repos/yuzu-emu/yuzu-mainline/releases/tags/mainline-0-{version}')
    else:
        url = get_finial_url(f'https://api.github.com/repos/pineappleEA/pineapple-src/releases/tags/EA-{version}')
    return requests.get(url).json()


if __name__ == '__main__':
    print(get_all_yuzu_release_versions('mainline'))
