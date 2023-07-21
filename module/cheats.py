import re
import shutil
import string
from pathlib import Path
from typing import List, Dict
from module.network import get_durable_cache_session
import logging
import time
from utils.string_util import auto_decode
from module.msg_notifier import send_notify
from exception.common_exception import IgnoredException


logger = logging.getLogger(__name__)
cheat_item_re = re.compile(r'\[(.*?)][\n\r]+([\n\ra-z0-9A-Z\s]+)', re.MULTILINE)
multi_new_line_re = re.compile('(\r\n|\n){2,}')
cheat_file_re = re.compile(r'^[\dA-Za-z]{16}.txt$')
game_id_re = re.compile(r'^[\dA-Za-z]{16}$')
cheat_name_re = re.compile(r'\{.*?}')


def get_game_data():
    res = {}
    try:
        resp = get_durable_cache_session().get(
            'https://ghproxy.net/https://raw.githubusercontent.com/MengNianxiaoyao/ns-emu-tools/main/game_data.json',
            timeout=5)
        return resp.json()
    except Exception as e:
        logger.warning(f'fail to load game data, ex: {e}')
    return res


def scan_all_cheats_folder(mod_path) -> List[Dict[str, str]]:
    root = Path(mod_path)
    logger.info(f'scanning cheats under path: {root}')
    cheats_folders = root.glob('**/cheats')
    # game_data = get_game_data()
    res = []
    for folder in cheats_folders:
        game_id = folder.parent.parent.name
        if game_id_re.match(game_id) is None:
            continue
        has_cheat_file = False
        for file in folder.glob('*.[tT][xX][tT]'):
            if cheat_file_re.match(file.name):
                has_cheat_file = True
                break
        if has_cheat_file:
            res.append({
                'game_id': game_id,
                'cheats_path': str(folder.absolute()),
                # 'game_name': game_data.get(game_id)
            })
    return res


def save_cheat_map_to_txt(cheats_map: Dict, txt_path: Path):
    with txt_path.open('w', encoding='utf-8') as f:
        for cheat_title in cheats_map:
            cheat_content = cheats_map[cheat_title]
            f.write(f'[{cheat_title}]\n')
            f.write(f'{cheat_content}\n')


def _parse_ryujinx_cheat_file():
    # ryujinx: https://github.com/Ryujinx/Ryujinx/blob/master/Ryujinx.HLE/HOS/ModLoader.cs#L312
    pass


def _parse_yuzu_cheat_file(cheat_file: Path):
    # yuzu: https://github.com/yuzu-emu/yuzu/blob/master/src/core/memory/cheat_engine.cpp#L100
    with open(cheat_file, 'rb') as f:
        data = auto_decode(f.read()).strip()
    if not data:
        return {}
    res = {}
    entry = {'title': 'Default', 'ops': []}
    i = 0
    while i < len(data):
        c = data[i]
        i += 1
        if c in string.whitespace:
            continue
        elif c in '{[':
            if entry['title'] != 'Default' or entry.get('ops'):
                res[entry['title']] = _convert_ops_to_content(entry.get('ops'))
            title, i = _find_next(data, ']}', i)
            if not title:
                return res
            entry = {'title': title, 'ops': []}
        elif c in string.hexdigits:
            if entry is None:
                return res
            s = c + data[i:i+7]
            if not all(c in string.hexdigits for c in s):
                return res
            ops = entry.get('ops', [])
            ops.append(s)
            entry['ops'] = ops
            i += 7
    if entry['title'] != 'Default' or entry.get('ops'):
        res[entry['title']] = _convert_ops_to_content(entry['ops'])
    return res


def _convert_ops_to_content(ops: List[str]):
    if not ops:
        return '\n'
    content = ''
    for i, op in enumerate(ops):
        content += op
        content += '\n' if i % 3 == 2 else ' '
    return content


def _find_next(s, tc, i):
    si = i
    while i < len(s):
        c = s[i]
        if c in tc:
            return s[si:i], i
        i += 1
    return None, si


def list_all_cheat_files_from_folder(folder_path: str):
    folder = Path(folder_path)
    if not folder.exists():
        raise IgnoredException(f'目录 {folder} 不存在.')
    res = []
    for txt_file in folder.glob('*.txt'):
        if cheat_file_re.match(txt_file.name):
            name = _read_cheat_name(txt_file)
            res.append({
                'path': str(txt_file.absolute()),
                'name': name
            })
    return res


def _read_cheat_name(txt_file: Path):
    with txt_file.open('rb') as f:
        text = auto_decode(f.read())
        res = cheat_name_re.findall(text)
        if res:
            return f'{txt_file.name} - {res[0]}'
    return txt_file.name


def load_cheat_chunk_info(cheat_file_path: str):
    cheat_file = Path(cheat_file_path)
    if not cheat_file.exists():
        raise IgnoredException(f'文件 {cheat_file} 不存在.')
    chunk_folder = cheat_file.parent.parent.joinpath('cheats_chunk')
    if not chunk_folder.exists():
        chunk_folder.mkdir(parents=True, exist_ok=True)
    chunk_file = chunk_folder.joinpath(cheat_file.name[:16] + '_chunk.txt')
    current_cheat_map = _parse_yuzu_cheat_file(cheat_file)
    logger.debug(f'current_cheat_map size: {len(current_cheat_map)}, '
                 f'current_cheat_map titles: {current_cheat_map.keys()}')
    if chunk_file.exists():
        chunk_cheat_map = _parse_yuzu_cheat_file(chunk_file)
        logger.debug(f'chunk_cheat_map titles: {chunk_cheat_map.keys()}')
        chunk_cheat_map.update(current_cheat_map)
        logger.info('chunk_cheat_map updated.')
    else:
        chunk_cheat_map = current_cheat_map.copy()
        logger.info('chunk_cheat_map inited.')
    logger.debug(f'chunk_cheat_map size: {len(chunk_cheat_map)}, '
                 f'chunk_cheat_map titles: {chunk_cheat_map.keys()}')
    res = []
    for title in chunk_cheat_map:
        enable = title in current_cheat_map
        res.append({
            'title': title,
            'enable': enable,
        })
    logger.info(f'saving chunk_cheat_map to {chunk_file}...')
    save_cheat_map_to_txt(chunk_cheat_map, chunk_file)
    logger.debug(f'res: {res}')
    return res


def update_current_cheats(enable_titles: List[str], cheat_file_path: str):
    cheat_file = Path(cheat_file_path)
    if not cheat_file.exists():
        raise IgnoredException(f'文件 {cheat_file} 不存在.')
    chunk_folder = cheat_file.parent.parent.joinpath('cheats_chunk')
    if not chunk_folder.exists():
        raise IgnoredException(f'仓库目录 {chunk_folder} 不存在.')
    chunk_file = chunk_folder.joinpath(cheat_file.name[:16] + '_chunk.txt')
    if not chunk_file.exists():
        raise IgnoredException(f'仓库文件 {chunk_file} 不存在.')
    backup_file = chunk_folder.joinpath(f'{cheat_file.name[:16]}_{int(time.time()*1000)}.txt')
    shutil.copy2(cheat_file, backup_file)
    logger.info(f'backup {cheat_file} to {backup_file}')
    send_notify(f'原文件已备份至 {backup_file}')
    cheat_map = {}
    chunk_map = _parse_yuzu_cheat_file(chunk_file)
    logger.debug(f'chunk_map size: {len(chunk_map)}, '
                 f'chunk_map titles: {chunk_map.keys()}')
    for title in enable_titles:
        if title in chunk_map:
            cheat_map[title] = chunk_map[title]
        else:
            logger.warning(f'title [{title}] not exist in chunk_map.')
    logger.debug(f'cheat_map size: {len(cheat_map)}, '
                 f'cheat_map titles: {cheat_map.keys()}')
    logger.info(f'saving cheat_map to {cheat_file}...')
    save_cheat_map_to_txt(cheat_map, cheat_file)


def open_cheat_mod_folder(folder_path: str):
    folder = Path(folder_path)
    if not folder.exists():
        raise IgnoredException(f'目录 {folder} 不存在.')
    import subprocess
    parent_folder = folder.parent
    logger.info(f'open folder [{parent_folder}] in explorer')
    subprocess.Popen(f'explorer "{str(parent_folder.absolute())}"')


def main():
    # cheats_folders = scan_all_cheats_folder(r'D:\Yuzu\user\load')
    # print(cheats_folders)
    # backup_original_cheats(cheats_folders)
    map = _parse_yuzu_cheat_file(Path(r'D:\Yuzu\user\load\0100F3400332C000\jinshouzhi\cheats_chunk\E3938FA78579C1CA_chunk.txt'))
    print(map)
    # print(get_game_data())


if __name__ == '__main__':
    main()
