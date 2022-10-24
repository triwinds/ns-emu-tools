import json
import os
from dataclasses import dataclass
from typing import Optional
from pathlib import Path
from dataclasses_json import dataclass_json


yuzu_config_path = Path('yuzu-config.json')
yuzu_config = None


@dataclass_json
@dataclass
class YuzuConfig:
    yuzu_path: Optional[str] = 'D:/Yuzu'
    yuzu_version: Optional[str] = None
    yuzu_firmware: Optional[str] = None
    key_file: Optional[str] = None


if os.path.exists(yuzu_config_path):
    with open(yuzu_config_path, 'r', encoding='utf-8') as f:
        yuzu_config = YuzuConfig.schema().loads(f.read())
if not yuzu_config:
    yuzu_config = YuzuConfig()


def dump_yuzu_config():
    print(f'saving config to {yuzu_config_path.absolute()}')
    with open(yuzu_config_path, 'w', encoding='utf-8') as f:
        f.write(yuzu_config.to_json(ensure_ascii=False))


__all__ = ['yuzu_config', 'dump_yuzu_config']
