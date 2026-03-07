import os

import requests
import requests_cache
import re
import json


# requests_cache.install_cache('ryujinx_issue', expire_after=114514)
game_re = re.compile(r'^(.*?) - ([\da-zA-Z]{16})$')
gh_token = os.environ.get('gh_token')
headers = {
    'Authorization': f'Bearer {gh_token}'
} if gh_token else {}


def update_with_cheats_db(game_data):
    resp = requests.get('https://raw.githubusercontent.com/HamletDuFromage/switch-cheats-db/master/versions.json')
    data = resp.json()

    for title_id, item in data.items():
        if 'title' not in item:
            continue
        title = item['title']
        game_data[title_id] = title


def update_with_page(game_data, page):
    resp = requests.get(f'https://api.github.com/repos/Ryujinx/Ryujinx-Games-List/issues?page={page}',
                        headers=headers)
    # print(resp.headers)
    print(f'handle page {page}')
    issues = resp.json()
    print(f'issues size: {len(issues)}')
    if not issues:
        return False
    for issue in issues:
        title = issue['title']
        groups = game_re.findall(title)
        if not groups:
            continue
        title, game_id = groups[0]
        game_data[game_id] = title
    return True


def update_all():
    page = 0
    game_data = {}
    while True:
        page += 1
        if not update_with_page(game_data, page):
            break
    if game_data:
        with open('game_data.json', 'w', encoding='utf-8') as f:
            json.dump(game_data, f, ensure_ascii=False, indent=2)


def update_latest():
    import os
    game_data = {}
    if os.path.exists('game_data.json'):
        with open('game_data.json', 'r', encoding='utf-8') as f:
            game_data = json.load(f)
    # update_with_page(game_data, 1)
    update_with_cheats_db(game_data)
    with open('game_data.json', 'w', encoding='utf-8') as f:
        json.dump(game_data, f, ensure_ascii=False, indent=2)


if __name__ == '__main__':
    update_latest()
