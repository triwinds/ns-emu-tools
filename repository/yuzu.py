from exception.common_exception import IgnoredException
from module.network import request_github_api, session
from repository.domain.release_info import from_github_api, ReleaseInfo, from_forgejo_api
from typing import List


def get_all_yuzu_release_versions(branch: str) -> List[str]:
    if branch == 'eden':
        return get_eden_all_release_versions()
    elif branch == 'citron':
        return get_citron_all_release_versions()
    return []


def get_eden_all_release_info() -> List[ReleaseInfo]:
    return [from_github_api(item)
            for item in
            request_github_api('https://api.github.com/repos/eden-emulator/Releases/releases')]


def get_eden_all_release_versions() -> List[str]:
    release_infos = get_eden_all_release_info()
    res = [item.tag_name for item in release_infos]
    return res


def get_citron_all_release_info() -> List[ReleaseInfo]:
    return [from_forgejo_api(item)
            for item in
            session.get('https://git.citron-emu.org/api/v1/repos/Citron/Emulator/releases').json()]


def get_citron_all_release_versions() -> List[str]:
    release_infos = get_citron_all_release_info()
    res = [item.tag_name for item in release_infos]
    return res


def get_yuzu_release_info_by_version(version, branch='eden') -> ReleaseInfo:
    if branch == 'eden':
        return get_eden_release_info_by_version(version)
    elif branch == 'citron':
        return get_citron_release_info_by_version(version)
    raise IgnoredException('Only support get yuzu release info on branch [eden/citron]')


def get_yuzu_all_release_info(branch: str) -> List[ReleaseInfo]:
    if branch == 'eden':
        return get_eden_all_release_info()
    elif branch == 'citron':
        return get_citron_all_release_info()
    raise IgnoredException('Only support get yuzu release info on branch [eden/citron]')


def get_eden_release_info_by_version(version) -> ReleaseInfo:
    url = f'https://api.github.com/repos/eden-emulator/Releases/releases/tags/{version}'
    data = request_github_api(url)
    return from_github_api(data)


def get_citron_release_info_by_version(version) -> ReleaseInfo:
    url = f'https://git.citron-emu.org/api/v1/repos/Citron/Emulator/releases/tags/{version}'
    data = session.get(url).json()
    if 'message' in data and '404' in data['message']:
        from exception.common_exception import VersionNotFoundException
        raise VersionNotFoundException(version, 'citron', 'yuzu')
    return from_forgejo_api(data)


def get_latest_change_log(branch:  str) -> str:
    release_infos = get_yuzu_all_release_info(branch)
    if len(release_infos) == 0:
        return f'无法获取 {branch} 最新版本变更信息'
    return release_infos[0].description


if __name__ == '__main__':
    # print(get_all_yuzu_release_versions('eden'))
    print('Latest 5 citron versions:', get_citron_all_release_versions()[:5])
    print('\nCitron 0.11.0 info:', get_citron_release_info_by_version('0.11.0'))