from module.network import request_github_api, session
from repository.domain.release_info import from_github_api, ReleaseInfo, from_gitlab_api
from typing import List


def get_all_yuzu_release_versions(branch: str) -> List[str]:
    if branch == 'eden':
        return get_eden_all_release_versions()
    elif branch == 'citron':
        return get_citron_all_release_versions()
    return []


def get_eden_all_release_versions() -> List[str]:
    res = []
    data = request_github_api('https://api.github.com/repos/eden-emulator/Releases/releases')
    for item in data:
        res.append(item['tag_name'])
    return res


def get_citron_all_release_versions() -> List[str]:
    res = []
    resp = session.get('https://git.citron-emu.org/api/v4/projects/1/releases')
    data = resp.json()
    for item in data:
        res.append(item['tag_name'])
    return res


def get_yuzu_release_info_by_version(version, branch='eden') -> ReleaseInfo:
    if branch == 'eden':
        return get_eden_release_info_by_version(version)
    elif branch == 'citron':
        return get_citron_release_info_by_version(version)
    return ReleaseInfo()


def get_eden_release_info_by_version(version) -> ReleaseInfo:
    url = f'https://api.github.com/repos/eden-emulator/Releases/releases/tags/{version}'
    data = request_github_api(url)
    return from_github_api(data)


def get_citron_release_info_by_version(version) -> ReleaseInfo:
    url = f'https://git.citron-emu.org/api/v4/projects/1/releases/{version}'
    data = session.get(url).json()
    if 'message' in data and '404' in data['message']:
        from exception.common_exception import VersionNotFoundException
        raise VersionNotFoundException(version, 'citron', 'yuzu')
    return from_gitlab_api(data)


if __name__ == '__main__':
    # print(get_all_yuzu_release_versions('eden'))
    # print(get_citron_all_release_versions())
    print(get_citron_release_info_by_version('0.1.1'))