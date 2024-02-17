import sentry_sdk
from config import current_version


def sampler(sample_data):
    if 'wsgi_environ' in sample_data and sample_data['wsgi_environ']['PATH_INFO'] == '/eel.js':
        return 0.1
    return 0


def init_sentry():
    sentry_sdk.init(
        dsn="https://022fb678c5bc4859b6052fc409506f23@o527477.ingest.sentry.io/4504689953472512",
        auto_session_tracking=False,
        traces_sampler=sampler,
        release=f'ns-emu-tools@{current_version}'
    )
    sentry_sdk.set_user({'ip_address': '{{auto}}'})
