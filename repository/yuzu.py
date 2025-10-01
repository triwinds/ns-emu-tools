from module.network import request_github_api
from repository.domain.release_info import from_github_api, ReleaseInfo
from typing import List


def get_all_yuzu_release_versions(branch: str) -> List[str]:
    res = []
    if branch != 'eden':
        return []
    data = request_github_api('https://api.github.com/repos/eden-emulator/Releases/releases')
    for item in data:
        res.append(item['tag_name'])
    return res



def get_yuzu_release_info_by_version(version, branch='eden') -> ReleaseInfo:
    url = f'https://api.github.com/repos/eden-emulator/Releases/releases/tags/{version}'
    data = request_github_api(url)
    return from_github_api(data)


if __name__ == '__main__':
    print(get_all_yuzu_release_versions('eden'))