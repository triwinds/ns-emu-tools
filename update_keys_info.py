from module.google_drive_api import GetFileList
from os import path, environ
import json


info_file = 'keys_info.json'


# modify from https://stackoverflow.com/questions/67946374/download-an-entire-public-folder-from-google-drive-using-python-or-wget-curl-wit
def update_keys_info():
    gdrive_api_key = environ.get('GDRIVE_API_KEY')
    keys_info = {}
    if path.exists(info_file):
        with open(info_file, 'r') as f:
            keys_info = json.load(f)
    resource = {
        "api_key": gdrive_api_key,
        "id": '1KAym-RpGIDuJiSmMLmpCtGVbhLm4VjTZ',
        "fields": "files(name,id)",
    }
    res = GetFileList(resource)
    print('Found #%d files' % len(res['fileList'][0]['files']))
    for file_dict in res['fileList'][0]['files']:
        if file_dict['name'] in keys_info:
            print(f"keys of {file_dict['name']} already exist.")
            continue
        print('add file info: %s' % file_dict['name'])
        file_url = "https://drive.google.com/uc?id=%s&export=download" % file_dict['id']
        file_dict['url'] = file_url
        keys_info[file_dict['name']] = file_dict
        with open(info_file, 'w') as f:
            json.dump(keys_info, f)


if __name__ == '__main__':
    update_keys_info()
