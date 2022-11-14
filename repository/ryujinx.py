from utils.network import get_finial_url, session


def get_all_ryujinx_release_infos():
    resp = session.get(get_finial_url('https://api.github.com/repos/Ryujinx/release-channel-master/releases'))
    return resp.json()


def get_latest_ryujinx_release_info():
    return get_all_ryujinx_release_infos()[0]


def get_ryujinx_release_info_by_version(version):
    url = get_finial_url(f'https://api.github.com/repos/Ryujinx/release-channel-master/releases/tags/{version}')
    return session.get(url).json()
