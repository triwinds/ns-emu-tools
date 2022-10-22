from functools import lru_cache
from module.downloader import download_path
import requests
import bs4


@lru_cache(1)
def get_firmware_infos():
    resp = requests.get('https://darthsternie.net/switch-firmwares/')
    soup = bs4.BeautifulSoup(resp.text, features="html.parser")
    table = soup.find('table', attrs={'id': 'tablepress-53'})
    body = table.find('tbody')
    res = []
    for tr in body.find_all('tr'):
        td_list = tr.find_all('td')
        res.append({
            'version': td_list[0].text[9:],
            'md5': td_list[1].text,
            'size': td_list[2].text,
            'url': td_list[4].a.attrs['href']
        })
    return res


@lru_cache(1)
def get_keys_info():
    resp = requests.get('https://rawgit.e6ex.com/triwinds/yuzu-tools/main/keys_info.json')
    return resp.json()


def download_keys_by_name(name):
    keys_info = get_keys_info()
    if name not in keys_info:
        raise RuntimeError(f'No such key [{name}].')
    key_info = keys_info[name]
    print(f'Downloading keys [{name}] from {key_info["url"]}')
    data = requests.get(key_info['url'])
    file = download_path.joinpath(name)
    with file.open('wb') as f:
        f.write(data.content)
    return file


if __name__ == '__main__':
    infos = get_firmware_infos()
    for info in infos:
        print(info)
