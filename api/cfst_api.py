from config import config
from api.common_response import *

import eel


@eel.expose
def optimize_cloudflare_hosts():
    from module.cfst import optimize_cloudflare_hosts
    try:
        optimize_cloudflare_hosts()
        return success_response()
    except Exception as e:
        return exception_response(e)


@eel.expose
def remove_cloudflare_hosts():
    from module.cfst import remove_cloudflare_hosts
    try:
        remove_cloudflare_hosts()
        return success_response()
    except Exception as e:
        return exception_response(e)
