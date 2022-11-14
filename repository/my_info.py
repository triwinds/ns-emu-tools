from utils.network import get_finial_url, session


def get_all_release():
    with session.cache_disabled():
        resp = session.get(get_finial_url('https://api.github.com/repos/triwinds/ns-emu-tools/releases'))
        return resp.json()


def get_latest_release(prerelease=False):
    with session.cache_disabled():
        data = get_all_release()
        release_list = data if prerelease else [i for i in data if i['prerelease'] is False]
        return release_list[0]
