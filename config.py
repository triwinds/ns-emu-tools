import json
import os
from dataclasses import dataclass
from typing import Optional
from pathlib import Path
from dataclasses_json import dataclass_json, Undefined
import logging
from logging.handlers import RotatingFileHandler
import sys


current_version = '0.1.5'


console = logging.StreamHandler()
console.setLevel(logging.DEBUG)
# logging.getLogger("requests").setLevel(logging.WARNING)
logging.getLogger("urllib3").setLevel(logging.WARNING)
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


# def log_exception(exctype, value, traceback):
#     logger.error(f'error: {exctype, value, traceback}')
#
#
# sys.excepthook = log_exception


@dataclass_json
@dataclass
class YuzuConfig:
    yuzu_path: Optional[str] = 'D:/Yuzu'
    yuzu_version: Optional[str] = None
    yuzu_firmware: Optional[str] = None
    branch: Optional[str] = 'ea'


@dataclass_json
@dataclass
class RyujinxConfig:
    path: Optional[str] = 'D:/Ryujinx'
    version: Optional[str] = None
    firmware: Optional[str] = None
    branch: Optional[str] = 'ava'


@dataclass_json
@dataclass
class CommonSetting:
    lastOpenEmuPage: Optional[str] = 'yuzu'


@dataclass_json(undefined=Undefined.EXCLUDE)
@dataclass
class Config:
    yuzu: YuzuConfig = YuzuConfig()
    ryujinx: RyujinxConfig = RyujinxConfig()
    setting: CommonSetting = CommonSetting()


if os.path.exists(config_path):
    with open(config_path, 'r', encoding='utf-8') as f:
        config = Config.from_dict(json.load(f))
if not config:
    config = Config()


def dump_config():
    logger.info(f'saving config to {config_path.absolute()}')
    with open(config_path, 'w', encoding='utf-8') as f:
        f.write(config.to_json(ensure_ascii=False, indent=2))


def update_yuzu_path(new_yuzu_path: str):
    new_path = Path(new_yuzu_path)
    if not new_path.exists():
        logger.info(f'create directory: {new_path}')
        new_path.mkdir(parents=True, exist_ok=True)
    if new_path.absolute() == Path(config.yuzu.yuzu_path).absolute():
        logger.info(f'No different with old yuzu path, skip update.')
        return
    logger.info(f'setting yuzu path to {new_path}')
    cfg = YuzuConfig()
    cfg.yuzu_path = str(new_path.absolute())
    config.yuzu = cfg
    dump_config()


def update_ryujinx_path(new_ryujinx_path: str):
    new_path = Path(new_ryujinx_path)
    if not new_path.exists():
        logger.info(f'create directory: {new_path}')
        new_path.mkdir(parents=True, exist_ok=True)
    if new_path.absolute() == Path(config.ryujinx.path).absolute():
        logger.info(f'No different with old ryujinx path, skip update.')
        return
    logger.info(f'setting ryujinx path to {new_path}')
    cfg = RyujinxConfig()
    cfg.path = str(new_path.absolute())
    config.ryujinx = cfg
    dump_config()


def update_last_open_emu_page(page: str):
    if page == 'ryujinx':
        config.setting.lastOpenEmuPage = 'ryujinx'
    else:
        config.setting.lastOpenEmuPage = 'yuzu'
    logger.info(f'update lastOpenEmuPage to {config.setting.lastOpenEmuPage}')
    dump_config()


__all__ = ['config', 'dump_config', 'update_yuzu_path', 'current_version', 'update_ryujinx_path',
           'update_last_open_emu_page']
