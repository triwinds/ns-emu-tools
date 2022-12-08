from utils.network import request_github_api, session


def get_all_release():
    with session.cache_disabled():
        return request_github_api('https://api.github.com/repos/triwinds/ns-emu-tools/releases')


def get_latest_release(prerelease=False):
    with session.cache_disabled():
        data = get_all_release()
        release_list = data if prerelease else [i for i in data if i['prerelease'] is False]
        return release_list[0]


def get_release_info_by_tag(tag: str):
    return request_github_api(f'https://api.github.com/repos/triwinds/ns-emu-tools/releases/tags/{tag}')
