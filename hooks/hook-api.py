from api import __all__


hiddenimports = []
for m in __all__:
    hiddenimports.append(f'api.{m}')
