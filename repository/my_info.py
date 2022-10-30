import requests


def get_all_release():
    resp = requests.get('https://cfrp.e6ex.com/ghapi/repos/triwinds/ns-emu-tools/releases')
    return resp.json()


def get_latest_release(prerelease=False):
    data = get_all_release()
    release_list = data if prerelease else [i for i in data if i['prerelease'] is False]
    return release_list[0]
