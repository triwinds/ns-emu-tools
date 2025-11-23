from dataclasses import dataclass
from typing import List, Dict
import logging

from exception.common_exception import IgnoredException, VersionNotFoundException

logger = logging.getLogger(__name__)


@dataclass
class ReleaseAsset:
    name: str
    download_url: str

    def __init__(self, name, download_url):
        self.name = name
        self.download_url = download_url


@dataclass
class ReleaseInfo:
    name: str
    tag_name: str
    description: str
    assets: List[ReleaseAsset]


def from_forgejo_api(release_info):
    if 'assets' not in release_info:
        logger.error('No assets in response, release_info: %s', release_info)
        msg = release_info['message'] if 'message' in release_info else 'No assets in response'
        raise IgnoredException(msg)
    assets = []
    for asset in release_info['assets']:
        assets.append(ReleaseAsset(asset['name'], asset['browser_download_url']))
    return ReleaseInfo(
        name=release_info['name'],
        tag_name=release_info['tag_name'],
        description=release_info['body'],
        assets=assets
    )


def from_gitlab_api(release_info):
    if 'assets' not in release_info:
        logger.error('No assets in response, release_info: %s', release_info)
        msg = release_info['message'] if 'message' in release_info else 'No assets in response'
        raise IgnoredException(msg)
    assets = []
    for asset in release_info['assets']['links']:
        assets.append(ReleaseAsset(asset['name'], asset['url']))
    return ReleaseInfo(
        name=release_info['name'],
        tag_name=release_info['tag_name'],
        description=release_info['description'],
        assets=assets
    )


def from_github_api(release_info):
    if 'assets' not in release_info:
        logger.error('No assets in response, release_info: %s', release_info)
        raise IgnoredException('No assets in response')
    assets = []
    for asset in release_info['assets']:
        assets.append(ReleaseAsset(asset['name'], asset['browser_download_url']))
    return ReleaseInfo(
        name=release_info['name'],
        tag_name=release_info['tag_name'],
        description=release_info['body'],
        assets=assets
    )
