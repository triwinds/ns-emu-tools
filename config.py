import json
import os
from dataclasses import dataclass, field
from typing import Optional, Dict
from pathlib import Path
from dataclasses_json import dataclass_json, Undefined
import logging
from logging.handlers import RotatingFileHandler


current_version = '0.4.7'
user_agent = f'ns-emu-tools/{current_version}'


console = logging.StreamHandler()
console.setLevel(logging.DEBUG)
# logging.getLogger("requests").setLevel(logging.WARNING)
logging.getLogger("urllib3").setLevel(logging.WARNING)
logging.getLogger("httpx").setLevel(logging.WARNING)
logging.getLogger("httpcore").setLevel(logging.WARNING)
# logging.getLogger("geventwebsocket.handler").setLevel(logging.WARNING)
logging.basicConfig(
    level=logging.DEBUG,
    format='%(asctime)s.%(msecs)03d|%(levelname)s|%(name)s|%(filename)s:%(lineno)s|%(funcName)s|%(message)s',
    datefmt='%Y-%m-%d %H:%M:%S',
    handlers=[RotatingFileHandler('ns-emu-tools.log', encoding='utf-8', maxBytes=10 * 1024 * 1024, backupCount=10),
              console]
)
logger = logging.getLogger(__name__)
config_path = Path('config.json')
config = None
shared = {}


def log_versions():
    import platform
    logger.info(f'system version: {platform.platform()}')
    logger.info(f'current version: {current_version}')


log_versions()


@dataclass_json
@dataclass
class YuzuConfig:
    yuzu_path: Optional[str] = 'D:\\Yuzu'
    yuzu_version: Optional[str] = None
    yuzu_firmware: Optional[str] = None
    branch: Optional[str] = 'ea'


@dataclass_json
@dataclass
class RyujinxConfig:
    path: Optional[str] = 'D:\\Ryujinx'
    version: Optional[str] = None
    firmware: Optional[str] = None
    branch: Optional[str] = 'mainline'


@dataclass_json
@dataclass
class NetworkSetting:
    firmwareDownloadSource: Optional[str] = 'github'
    githubApiMode: Optional[str] = 'direct'
    githubDownloadMirror: Optional[str] = 'cloudflare_load_balance'
    useDoh: Optional[bool] = True
    proxy: Optional[str] = 'system'


@dataclass_json
@dataclass
class DownloadSetting:
    autoDeleteAfterInstall: Optional[bool] = True
    disableAria2Ipv6: Optional[bool] = True
    removeOldAria2LogFile: Optional[bool] = True
    verifyFirmwareMd5: Optional[bool] = True


@dataclass_json
@dataclass
class UiSetting:
    lastOpenEmuPage: Optional[str] = 'ryujinx'
    dark: Optional[bool] = True
    mode: Optional[str] = 'auto'
    width: int = 1300
    height: int = 850


@dataclass_json
@dataclass
class OtherSetting:
    rename_yuzu_to_cemu: Optional[bool] = False


@dataclass_json(undefined=Undefined.EXCLUDE)
@dataclass
class CommonSetting:
    ui: UiSetting = field(default_factory=UiSetting)
    network: NetworkSetting = field(default_factory=NetworkSetting)
    download: DownloadSetting = field(default_factory=DownloadSetting)
    other: OtherSetting = field(default_factory=OtherSetting)


@dataclass_json(undefined=Undefined.EXCLUDE)
@dataclass
class Config:
    yuzu: YuzuConfig = field(default_factory=YuzuConfig)
    ryujinx: RyujinxConfig = field(default_factory=RyujinxConfig)
    setting: CommonSetting = field(default_factory=CommonSetting)


if os.path.exists(config_path):
    with open(config_path, 'r', encoding='utf-8') as f:
        config = Config.from_dict(json.load(f))
        config.yuzu.yuzu_path = str(Path(config.yuzu.yuzu_path).absolute())
        config.ryujinx.path = str(Path(config.ryujinx.path).absolute())
if not config:
    config = Config()
config.yuzu.branch = 'ea'


def dump_config():
    logger.info(f'saving config to {config_path.absolute()}')
    with open(config_path, 'w', encoding='utf-8') as f:
        f.write(config.to_json(ensure_ascii=False, indent=2))


def update_last_open_emu_page(page: str):
    if page == 'ryujinx':
        config.setting.ui.lastOpenEmuPage = 'ryujinx'
    else:
        config.setting.ui.lastOpenEmuPage = 'yuzu'
    logger.info(f'update lastOpenEmuPage to {config.setting.ui.lastOpenEmuPage}')
    dump_config()


def update_dark_state(dark: bool):
    if dark is None:
        dark = True
    config.setting.ui.dark = dark
    logger.info(f'update dark to {config.setting.ui.dark}')
    dump_config()


def update_setting(setting: Dict[str, object]):
    logger.info(f'updating settings: {setting}')
    config.setting = CommonSetting.from_dict(setting)
    dump_config()


__all__ = ['config', 'dump_config', 'YuzuConfig', 'current_version', 'RyujinxConfig', 'update_dark_state',
           'update_last_open_emu_page', 'update_setting', 'user_agent', 'shared']
