import requests
import bs4


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


if __name__ == '__main__':
    infos = get_firmware_infos()
    for info in infos:
        print(info)
