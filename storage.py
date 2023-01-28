from dataclasses import dataclass, field
import json
import os
from pathlib import Path
from typing import Dict

from dataclasses_json import dataclass_json, Undefined
from config import config, YuzuConfig, RyujinxConfig
import logging


logger = logging.getLogger(__name__)
storage_path = Path('storage.json')
storage = None


@dataclass_json(undefined=Undefined.EXCLUDE)
@dataclass
class Storage:
    yuzu_history: Dict[str, YuzuConfig] = field(default_factory=dict)
    ryujinx_history: Dict[str, RyujinxConfig] = field(default_factory=dict)


def dump_storage():
    logger.info(f'saving storage to {storage_path.absolute()}')
    with open(storage_path, 'w', encoding='utf-8') as f:
        f.write(storage.to_json(ensure_ascii=False, indent=2))


if os.path.exists(storage_path):
    with open(storage_path, 'r', encoding='utf-8') as f:
        storage = Storage.from_dict(json.load(f))
if not storage:
    storage = Storage()
    dump_storage()


def add_yuzu_history(yuzu_config: YuzuConfig, dump=True):
    storage.yuzu_history[yuzu_config.yuzu_path] = yuzu_config
    if dump:
        dump_storage()


def add_ryujinx_history(ryujinx_config: RyujinxConfig, dump=True):
    storage.ryujinx_history[ryujinx_config.path] = ryujinx_config
    if dump:
        dump_storage()


if __name__ == '__main__':
    print(storage)
