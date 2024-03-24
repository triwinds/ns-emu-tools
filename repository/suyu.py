from module.network import session, get_finial_url


# Api doc: https://git.suyu.dev/api/swagger

def load_suyu_releases():
    resp = session.get(get_finial_url('https://git.suyu.dev/api/v1/repos/suyu/suyu/releases'))
    return resp.json()


def get_release_by_tag_name(tag_name: str):
    resp = session.get(get_finial_url(f'https://git.suyu.dev/api/v1/repos/suyu/suyu/releases/tags/{tag_name}'))
    return resp.json()
