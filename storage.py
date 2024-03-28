from dataclasses import dataclass, field
import json
import os
from pathlib import Path
from typing import Dict

from dataclasses_json import dataclass_json, Undefined
from config import config, YuzuConfig, RyujinxConfig, SuyuConfig
import logging


logger = logging.getLogger(__name__)
storage_path = Path('storage.json')
storage = None


@dataclass_json(undefined=Undefined.EXCLUDE)
@dataclass
class Storage:
    yuzu_history: Dict[str, YuzuConfig] = field(default_factory=dict)
    suyu_history: Dict[str, SuyuConfig] = field(default_factory=dict)
    ryujinx_history: Dict[str, RyujinxConfig] = field(default_factory=dict)
    yuzu_save_backup_path: str = str(Path(r'D:\\yuzu_save_backup'))


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
    yuzu_path = Path(yuzu_config.yuzu_path)
    storage.yuzu_history[str(yuzu_path.absolute())] = yuzu_config
    if dump:
        dump_storage()


def add_ryujinx_history(ryujinx_config: RyujinxConfig, dump=True):
    ryujinx_path = Path(ryujinx_config.path)
    storage.ryujinx_history[str(ryujinx_path.absolute())] = ryujinx_config
    if dump:
        dump_storage()


def add_suyu_history(suyu_config: SuyuConfig, dump=True):
    suyu_path = Path(suyu_config.path)
    storage.suyu_history[str(suyu_path.absolute())] = suyu_config
    if dump:
        dump_storage()


def delete_history_path(emu_type: str, path_to_delete: str):
    if emu_type == 'yuzu':
        history = storage.yuzu_history
    elif emu_type == 'suyu':
        history = storage.suyu_history
    else:
        history = storage.ryujinx_history
    abs_path = str(Path(path_to_delete).absolute())
    if abs_path in history:
        del history[abs_path]
        logger.info(f'{emu_type} path {abs_path} deleted.')
        dump_storage()


if __name__ == '__main__':
    print(storage)
