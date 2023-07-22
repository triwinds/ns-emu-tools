import requests
import os


bot_token = os.environ['TELEGRAM_TOKEN']
send_to = os.environ['TG_SEND_TO']

message_template = """New release of  v%s

%s

Release page: [%s](%s)
"""

message_template2 = """New release of  v%s

```
%s
```

Release page: [%s](%s)
"""


def send_message(msg: str):
    data = {'text': msg, 'chat_id': send_to, 'parse_mode': 'markdown'}
    resp = requests.post(f'https://api.telegram.org/bot{bot_token}/sendMessage', json=data)
    data = resp.json()
    # print(data)
    # if not data.get('ok'):
    #     print(data)
    #     raise RuntimeError(data.get('description'))


def get_all_release():
    return requests.get('https://api.github.com/repos/MengNianxiaoyao/ns-emu-tools/releases').json()


def get_latest_release(prerelease=False):
    data = get_all_release()
    release_list = data if prerelease else [i for i in data if i['prerelease'] is False]
    return release_list[0]


def main():
    release_info = get_latest_release(True)
    print(release_info)
    message = message_template % (
        release_info['tag_name'],
        release_info['body'],
        release_info['tag_name'],
        release_info['html_url']
    )
    try:
        send_message(message)
        return
    except:
        pass
    message = message_template2 % (
        release_info['tag_name'],
        release_info['body'],
        release_info['tag_name'],
        release_info['html_url']
    )
    send_message(message)


if __name__ == '__main__':
    main()
