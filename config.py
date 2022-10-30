import json
import os
from dataclasses import dataclass
from typing import Optional
from pathlib import Path
from dataclasses_json import dataclass_json
import logging
from logging.handlers import RotatingFileHandler
import sys


current_version = '0.0.2-fix2'


console = logging.StreamHandler()
console.setLevel(logging.DEBUG)
# logging.getLogger("requests").setLevel(logging.WARNING)
logging.getLogger("urllib3").setLevel(logging.WARNING)
# logging.getLogger("geventwebsocket.handler").setLevel(logging.WARNING)
logging.basicConfig(
    level=logging.DEBUG,
    format='%(asctime)s.%(msecs)03d|%(levelname)s|%(name)s|%(filename)s:%(lineno)s|%(funcName)s|%(message)s',
    datefmt='%Y-%m-%d %H:%M:%S',
    handlers=[RotatingFileHandler('yuzu-tools.log', encoding='utf-8', maxBytes=10 * 1024 * 1024, backupCount=10),
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
    key_file: Optional[str] = None


@dataclass_json
@dataclass
class Config:
    yuzu: YuzuConfig = YuzuConfig()


if os.path.exists(config_path):
    with open(config_path, 'r', encoding='utf-8') as f:
        config = Config.schema().loads(f.read())
if not config:
    config = Config()


def dump_config():
    logger.info(f'saving config to {config_path.absolute()}')
    with open(config_path, 'w', encoding='utf-8') as f:
        f.write(config.to_json(ensure_ascii=False))


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


__all__ = ['config', 'dump_config', 'update_yuzu_path', 'current_version']
