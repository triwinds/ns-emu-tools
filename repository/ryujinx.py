from module.network import request_github_api, session, get_finial_url
from repository.domain.release_info import *


# They move them codes to https://git.ryujinx.app/ryubing/ryujinx
# the api of releases https://git.ryujinx.app/api/v4/projects/1/releases  (using GitLab)
# Gitlab Canary releases: https://git.ryujinx.app/api/v4/projects/68/releases


def get_all_ryujinx_release_infos(branch='mainline')-> List[ReleaseInfo]:
    if branch == 'canary':
        return get_all_canary_ryujinx_release_infos()
    # return request_github_api('https://api.github.com/repos/Ryubing/Stable-Releases/releases')
    resp = session.get('https://git.ryujinx.app/api/v4/projects/1/releases').json()
    res =  [from_gitlab_api(item) for item in resp]
    return res


def get_all_canary_ryujinx_release_infos() -> List[ReleaseInfo]:
    resp = session.get('https://git.ryujinx.app/api/v4/projects/68/releases').json()
    return [from_gitlab_api(item) for item in resp]


def get_latest_ryujinx_release_info() -> ReleaseInfo:
    return get_all_ryujinx_release_infos()[0]


def get_ryujinx_release_info_by_version(version, branch='mainline') -> ReleaseInfo:
    if branch == 'canary':
        return get_canary_ryujinx_release_info_by_version(version)
#     get from gitlab
    return from_gitlab_api(session.get(f'https://git.ryujinx.app/api/v4/projects/1/releases/{version}').json())



def get_canary_ryujinx_release_info_by_version(version) -> ReleaseInfo:
    return from_gitlab_api(session.get(f'https://git.ryujinx.app/api/v4/projects/68/releases/{version}').json())


def load_ryujinx_change_log(branch: str) -> str:
    infos = get_all_ryujinx_release_infos(branch)
    return infos[0].description
