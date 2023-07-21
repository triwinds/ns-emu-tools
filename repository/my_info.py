from module.network import request_github_api, session, get_finial_url


def get_all_release():
    with session.cache_disabled():
        return request_github_api('https://api.github.com/repos/MengNianxiaoyao/ns-emu-tools/releases')


def get_latest_release(prerelease=False):
    with session.cache_disabled():
        data = get_all_release()
        release_list = data if prerelease else [i for i in data if i['prerelease'] is False]
        return release_list[0]


def get_release_info_by_tag(tag: str):
    return request_github_api(f'https://api.github.com/repos/MengNianxiaoyao/ns-emu-tools/releases/tags/{tag}')


def load_change_log():
    resp = session.get(get_finial_url('https://raw.githubusercontent.com/MengNianxiaoyao/ns-emu-tools/main/changelog.md'))
    return resp.text
