from module.network import request_github_api, session, get_finial_url


# They move them codes to https://git.ryujinx.app/ryubing/ryujinx
# the api of releases https://git.ryujinx.app/api/v4/projects/1/releases  (using GitLab)


def get_all_ryujinx_release_infos(branch='mainline'):
    if branch == 'canary':
        return get_all_canary_ryujinx_release_infos()
    return request_github_api('https://api.github.com/repos/Ryubing/Stable-Releases/releases')


def get_all_canary_ryujinx_release_infos():
    return request_github_api('https://api.github.com/repos/Ryubing/Canary-Releases/releases')


def get_latest_ryujinx_release_info():
    return get_all_ryujinx_release_infos()[0]


def get_ryujinx_release_info_by_version(version, branch='mainline'):
    if branch == 'canary':
        return get_canary_ryujinx_release_info_by_version(version)
    return request_github_api(f'https://api.github.com/repos/Ryubing/Stable-Releases/releases/tags/{version}')


def get_canary_ryujinx_release_info_by_version(version):
    return request_github_api(f'https://api.github.com/repos/Ryubing/Canary-Releases/releases/tags/{version}')


def load_ryujinx_change_log(branch: str):
    if branch == 'canary':
        resp = request_github_api('https://api.github.com/repos/Ryubing/Canary-Releases/releases')
    else:
        resp = request_github_api('https://api.github.com/repos/Ryubing/Stable-Releases/releases')
    return resp[0].get('body')
