from utils.network import request_github_api, session, get_finial_url


def get_all_ryujinx_release_infos():
    return request_github_api('https://api.github.com/repos/Ryujinx/release-channel-master/releases')


def get_latest_ryujinx_release_info():
    return get_all_ryujinx_release_infos()[0]


def get_ryujinx_release_info_by_version(version):
    return request_github_api(f'https://api.github.com/repos/Ryujinx/release-channel-master/releases/tags/{version}')


def load_ryujinx_change_log():
    resp = session.get(get_finial_url('https://raw.githubusercontent.com/wiki/Ryujinx/Ryujinx/Changelog.md'))
    return resp.text
